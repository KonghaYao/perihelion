//! TF-IDF 搜索索引 — 工具索引构建、混合搜索（TF-IDF + 关键词）、工具查找

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use rust_create_agent::tools::BaseTool;

use super::keyword_search;

/// 搜索结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub score: f64,
}

/// TF-IDF 索引内部结构
struct TfIdfIndex {
    /// 每个工具的词向量（词 → TF×IDF 权重）
    doc_vectors: HashMap<String, HashMap<String, f64>>,
}

/// 对文本进行分词
///
/// CJK 字符逐字分割，ASCII 按空格/下划线/连字符分割，全部转小写
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_ascii() {
            if ch == ' ' || ch == '_' || ch == '-' {
                if !current.is_empty() {
                    tokens.push(current.to_lowercase());
                    current = String::new();
                }
            } else {
                current.push(ch);
            }
        } else {
            // CJK 字符：先 flush ASCII buffer，然后每个字符一个 token
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
                current = String::new();
            }
            tokens.push(ch.to_lowercase().to_string());
        }
    }
    if !current.is_empty() {
        tokens.push(current.to_lowercase());
    }

    tokens
}

/// 构建 TF-IDF 索引
fn build_tfidf_index(tools: &[Arc<dyn BaseTool>]) -> TfIdfIndex {
    let total_docs = tools.len();
    if total_docs == 0 {
        return TfIdfIndex {
            doc_vectors: HashMap::new(),
        };
    }

    // 收集每个文档的词频
    let mut doc_term_freqs: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut doc_freqs: HashMap<String, usize> = HashMap::new();

    for tool in tools {
        let name = tool.name();
        let desc = tool.description();

        // 加权分词：name 权重 3.0，description 权重 2.5
        let name_tokens = tokenize(name);
        let desc_tokens = tokenize(desc);

        let mut term_freqs: HashMap<String, f64> = HashMap::new();

        for token in &name_tokens {
            *term_freqs.entry(token.clone()).or_insert(0.0) += 3.0;
        }
        for token in &desc_tokens {
            *term_freqs.entry(token.clone()).or_insert(0.0) += 2.5;
        }

        // 统计文档频率
        let seen_terms: std::collections::HashSet<&String> = term_freqs.keys().collect();
        for term in seen_terms {
            *doc_freqs.entry(term.clone()).or_insert(0) += 1;
        }

        doc_term_freqs.insert(name.to_string(), term_freqs);
    }

    // 计算 TF-IDF 向量
    let mut doc_vectors: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (doc_name, term_freqs) in &doc_term_freqs {
        let mut vector = HashMap::new();
        for (term, tf) in term_freqs {
            let df = *doc_freqs.get(term).unwrap_or(&1) as f64;
            let idf = (total_docs as f64 / (df + 1.0)).ln();
            vector.insert(term.clone(), tf * idf);
        }
        doc_vectors.insert(doc_name.clone(), vector);
    }

    TfIdfIndex { doc_vectors }
}

/// 余弦相似度
fn cosine_similarity(vec1: &HashMap<String, f64>, vec2: &HashMap<String, f64>) -> f64 {
    if vec1.is_empty() || vec2.is_empty() {
        return 0.0;
    }

    let mut dot_product = 0.0;
    let mut norm1 = 0.0;
    let mut norm2 = 0.0;

    for (term, w1) in vec1 {
        norm1 += w1 * w1;
        if let Some(w2) = vec2.get(term) {
            dot_product += w1 * w2;
        }
    }
    for w2 in vec2.values() {
        norm2 += w2 * w2;
    }

    let denom = norm1.sqrt() * norm2.sqrt();
    if denom == 0.0 {
        return 0.0;
    }
    dot_product / denom
}

/// 工具搜索索引
///
/// 使用 TF-IDF + 关键词混合搜索（0.6/0.4 加权），
/// 支持并发读（RwLock）。
pub struct ToolSearchIndex {
    tools: RwLock<HashMap<String, Arc<dyn BaseTool>>>,
    tfidf_index: RwLock<TfIdfIndex>,
    /// 缓存首次生成的 deferred tools 提示词，后续不再重新生成
    cached_prompt: RwLock<Option<String>>,
}

impl ToolSearchIndex {
    /// 构造空索引
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            tfidf_index: RwLock::new(TfIdfIndex {
                doc_vectors: HashMap::new(),
            }),
            cached_prompt: RwLock::new(None),
        }
    }

    /// 使用 deferred tools 构建索引
    pub fn build(&self, deferred_tools: Vec<Arc<dyn BaseTool>>) {
        let mut tools_map = self.tools.write();
        let tfidf = build_tfidf_index(&deferred_tools);

        for tool in &deferred_tools {
            tools_map.insert(tool.name().to_string(), Arc::clone(tool));
        }

        // 将已有工具重新纳入索引
        *self.tfidf_index.write() = tfidf;
    }

    /// 混合搜索
    ///
    /// 查询语法：
    /// - `select:CronCreate,Snip` — 按精确名称查找，逗号分隔
    /// - `+slack message` — `+` 前缀词为必选，其余为可选关键词
    /// - `slack message` — 纯关键词搜索
    ///
    /// 评分：关键词分数 × 0.4 + TF-IDF 分数 × 0.6
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        // select: 前缀 — 按精确名称直接查找
        if let Some(names_str) = query.strip_prefix("select:") {
            let tools = self.tools.read();
            let names: Vec<&str> = names_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            return names
                .into_iter()
                .filter_map(|name| {
                    let tool = tools.get(name)?;
                    Some(SearchResult {
                        name: tool.name().to_string(),
                        description: tool.description().to_string(),
                        parameters: tool.parameters(),
                        score: 1.0,
                    })
                })
                .take(limit)
                .collect();
        }

        let (required, optional) = keyword_search::parse_query(query);
        let tools = self.tools.read();
        let tfidf = self.tfidf_index.read();

        // 构建查询向量
        let query_tokens = tokenize(query);
        let mut query_vector: HashMap<String, f64> = HashMap::new();
        for token in &query_tokens {
            *query_vector.entry(token.clone()).or_insert(0.0) += 1.0;
        }

        let mut results: Vec<SearchResult> = Vec::new();

        for (name, tool) in tools.iter() {
            let desc = tool.description();
            let params = tool.parameters();

            // 关键词分数
            let kw_score = keyword_search::keyword_score(name, desc, &required, &optional);

            // 必选词缺失时硬过滤：跳过该工具
            if !required.is_empty() && kw_score == 0.0 {
                continue;
            }

            // TF-IDF 分数
            let tfidf_score = if let Some(doc_vec) = tfidf.doc_vectors.get(name) {
                cosine_similarity(&query_vector, doc_vec)
            } else {
                0.0
            };

            // 混合分数
            let score = kw_score * 0.4 + tfidf_score * 0.6;

            results.push(SearchResult {
                name: name.clone(),
                description: desc.to_string(),
                parameters: params.clone(),
                score,
            });
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// 按名称查找工具
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn BaseTool>> {
        self.tools.read().get(name).cloned()
    }

    /// 返回所有工具的 (name, description) 列表
    pub fn list_names(&self) -> Vec<(String, String)> {
        self.tools
            .read()
            .iter()
            .map(|(name, tool)| (name.clone(), tool.description().to_string()))
            .collect()
    }

    /// 返回 Markdown 格式的延迟工具列表（按名称排序，保证跨进程稳定）
    pub fn format_deferred_list(&self) -> String {
        let tools = self.tools.read();
        if tools.is_empty() {
            return String::new();
        }

        let mut entries: Vec<_> = tools.iter().collect();
        entries.sort_by_key(|(name, _)| *name);

        let mut lines = String::from("## Deferred Tools\n\n");
        lines.push_str("The following tools are not in your direct tool list. Use `SearchExtraTools` to search for them, then `ExecuteExtraTool` to invoke.\n\n");
        for (name, tool) in entries {
            lines.push_str(&format!("- {}: {}\n", name, tool.description()));
            let params = tool.parameters();
            let props = params.get("properties");
            if let Some(props) = props.and_then(|p| p.as_object()) {
                if !props.is_empty() {
                    lines.push_str("  Parameters:\n");
                    for (param_name, param_schema) in props {
                        let desc = param_schema
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("");
                        let param_type = param_schema
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("any");
                        lines.push_str(&format!(
                            "    - `{}` ({}): {}\n",
                            param_name, param_type, desc
                        ));
                    }
                }
            }
        }
        lines
    }

    /// 返回索引中的工具总数
    pub fn total_count(&self) -> usize {
        self.tools.read().len()
    }

    /// 获取缓存的提示词
    pub fn cached_prompt(&self) -> Option<String> {
        self.cached_prompt.read().clone()
    }

    /// 缓存提示词（首次生成后调用）
    pub fn set_cached_prompt(&self, prompt: String) {
        *self.cached_prompt.write() = Some(prompt);
    }
}

impl Default for ToolSearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct MockTool {
        name_str: String,
        desc_str: String,
        params: serde_json::Value,
    }

    impl MockTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
                params: json!({"type": "object", "properties": {}}),
            }
        }
    }

    #[async_trait::async_trait]
    impl BaseTool for MockTool {
        fn name(&self) -> &str {
            &self.name_str
        }
        fn description(&self) -> &str {
            &self.desc_str
        }
        fn parameters(&self) -> serde_json::Value {
            self.params.clone()
        }
        async fn invoke(
            &self,
            _input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("mock".to_string())
        }
    }

    fn make_mock_tools() -> Vec<Arc<dyn BaseTool>> {
        vec![
            Arc::new(MockTool::new(
                "CronRegister",
                "Register a cron scheduled task",
            )),
            Arc::new(MockTool::new("CronList", "List all cron tasks")),
            Arc::new(MockTool::new("CronRemove", "Remove a cron task by ID")),
            Arc::new(MockTool::new(
                "mcp__slack__send_message",
                "Send a message to Slack channel",
            )),
            Arc::new(MockTool::new(
                "mcp__github__create_issue",
                "Create a GitHub issue",
            )),
        ]
    }

    #[test]
    fn test_build_index() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);
        assert_eq!(index.list_names().len(), 5);
    }

    #[test]
    fn test_keyword_search() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let results = index.search("cron create", 3);
        assert!(!results.is_empty());
        // CronRegister should rank high
        assert!(results[0].name.contains("Cron"));
    }

    #[test]
    fn test_tfidf_search() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let results = index.search("schedule task", 3);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_hybrid_search() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let results = index.search("+slack message", 5);
        // Required word "slack" should filter to only slack tools
        assert!(results
            .iter()
            .all(|r| r.name.to_lowercase().contains("slack")));
    }

    #[test]
    fn test_get_tool() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        assert!(index.get_tool("CronRegister").is_some());
        assert!(index.get_tool("NonExistent").is_none());
    }

    #[test]
    fn test_format_deferred_list() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let list = index.format_deferred_list();
        assert!(list.contains("CronRegister"));
        assert!(list.contains("mcp__slack__send_message"));
    }

    #[test]
    fn test_total_count() {
        let index = ToolSearchIndex::new();
        assert_eq!(index.total_count(), 0);

        let tools = make_mock_tools();
        index.build(tools);
        assert_eq!(index.total_count(), 5);
    }

    #[test]
    fn test_select_exact_match() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let results = index.search("select:CronRegister,CronList", 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "CronRegister");
        assert_eq!(results[1].name, "CronList");
    }

    #[test]
    fn test_select_partial_miss() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let results = index.search("select:CronRegister,NonExistent", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "CronRegister");
    }

    #[test]
    fn test_select_empty_result() {
        let index = ToolSearchIndex::new();
        let tools = make_mock_tools();
        index.build(tools);

        let results = index.search("select:NonExistent", 10);
        assert!(results.is_empty());
    }
}
