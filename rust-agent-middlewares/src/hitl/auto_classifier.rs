use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use rust_create_agent::llm::types::LlmRequest;
use rust_create_agent::llm::BaseModel;
use rust_create_agent::messages::BaseMessage;
use tokio::sync::Mutex as AsyncMutex;

/// 分类结果枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// 允许执行
    Allow,
    /// 拒绝执行
    Deny,
    /// 不确定，回退到人工审批
    Unsure,
}

/// 自动分类器 trait — 根据工具名称和输入判断是否放行
#[async_trait]
pub trait AutoClassifier: Send + Sync {
    async fn classify(&self, tool_name: &str, tool_input: &serde_json::Value) -> Classification;
}

// ─── 缓存条目 ────────────────────────────────────────────────────────────────

/// 缓存条目：存储分类结果和过期时间
struct CacheEntry {
    classification: Classification,
    expires_at: Instant,
}

// ─── LlmAutoClassifier ───────────────────────────────────────────────────────

/// 基于 LLM 的自动分类器实现
///
/// 持有 `Arc<AsyncMutex<Box<dyn BaseModel>>>` 调用 LLM 做分类，
/// 内置基于 `(tool_name, input_hash)` 的缓存，有效期 5 分钟。
pub struct LlmAutoClassifier {
    model: Arc<AsyncMutex<Box<dyn BaseModel>>>,
    cache: Mutex<HashMap<(String, u64), CacheEntry>>,
    cache_ttl: Duration,
}

impl LlmAutoClassifier {
    /// 创建新的 LLM 分类器
    pub fn new(model: Arc<AsyncMutex<Box<dyn BaseModel>>>) -> Self {
        Self {
            model,
            cache: Mutex::new(HashMap::new()),
            cache_ttl: Duration::from_secs(5 * 60),
        }
    }

    /// 使用自定义缓存 TTL 创建分类器
    pub fn with_cache_ttl(model: Arc<AsyncMutex<Box<dyn BaseModel>>>, cache_ttl: Duration) -> Self {
        Self {
            model,
            cache: Mutex::new(HashMap::new()),
            cache_ttl,
        }
    }

    /// 计算缓存键：(tool_name, input 的 64 位 hash)
    fn cache_key(tool_name: &str, tool_input: &serde_json::Value) -> (String, u64) {
        let input_str = serde_json::to_string(tool_input).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        input_str.hash(&mut hasher);
        (tool_name.to_string(), hasher.finish())
    }

    /// 查询缓存，命中且未过期返回 Some，否则 None
    fn lookup_cache(&self, key: &(String, u64)) -> Option<Classification> {
        let cache = self.cache.lock();
        cache.get(key).and_then(|entry| {
            if entry.expires_at > Instant::now() {
                Some(entry.classification)
            } else {
                None
            }
        })
    }

    /// 写入缓存，同时淘汰过期条目
    fn insert_cache(&self, key: (String, u64), classification: Classification) {
        let mut cache = self.cache.lock();
        let now = Instant::now();
        cache.retain(|_, entry| entry.expires_at > now);
        cache.insert(
            key,
            CacheEntry {
                classification,
                expires_at: now + self.cache_ttl,
            },
        );
    }

    /// 调用 LLM 进行分类
    async fn call_llm(&self, tool_name: &str, tool_input: &serde_json::Value) -> Classification {
        let prompt = format!(
            "判断以下工具调用是否安全。工具: {}, 输入: {}\n只回答 ALLOW / DENY / UNSURE，不要解释。",
            tool_name,
            serde_json::to_string(tool_input).unwrap_or_default(),
        );

        let request = LlmRequest::new(vec![BaseMessage::human(prompt)])
            .with_system("你是一个工具调用安全分类器。只回答 ALLOW、DENY 或 UNSURE 中的一个词。")
            .with_max_tokens(32);

        let response = {
            let model = self.model.lock().await;
            model.invoke(request).await
        };

        match response {
            Ok(resp) => {
                let text = resp.message.content().trim().to_uppercase();
                // 提取所有纯字母单词
                let words: Vec<&str> = text
                    .split(|c: char| !c.is_alphabetic())
                    .filter(|w| !w.is_empty())
                    .collect();

                // 检查是否存在否定词（NOT, DON'T, WON'T, NEVER, etc.）
                let has_negation = words.iter().any(|w| {
                    matches!(
                        *w,
                        "NOT" | "DONT" | "WONT" | "CANT" | "NEVER" | "NO" | "NEITHER" | "NOR"
                    )
                });

                // 包含 DENY（无论有无否定）→ Deny；否定+ALLOW → Unsure
                if words.contains(&"DENY") {
                    Classification::Deny
                } else if words.contains(&"ALLOW") && !has_negation {
                    Classification::Allow
                } else {
                    Classification::Unsure
                }
            }
            Err(_) => Classification::Unsure,
        }
    }
}

#[async_trait]
impl AutoClassifier for LlmAutoClassifier {
    async fn classify(&self, tool_name: &str, tool_input: &serde_json::Value) -> Classification {
        let key = Self::cache_key(tool_name, tool_input);

        if let Some(cached) = self.lookup_cache(&key) {
            return cached;
        }

        let result = self.call_llm(tool_name, tool_input).await;

        self.insert_cache(key, result);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::error::{AgentError, AgentResult};
    use rust_create_agent::llm::types::{LlmResponse, StopReason};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockClassifyModel {
        response: std::sync::Mutex<String>,
        call_count: AtomicUsize,
        should_fail: std::sync::Mutex<bool>,
    }

    impl MockClassifyModel {
        fn new(response: &str) -> Self {
            Self {
                response: std::sync::Mutex::new(response.to_string()),
                call_count: AtomicUsize::new(0),
                should_fail: std::sync::Mutex::new(false),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }

        fn set_should_fail(&self, fail: bool) {
            *self.should_fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl BaseModel for MockClassifyModel {
        async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
            if *self.should_fail.lock().unwrap() {
                return Err(AgentError::LlmError("mock failure".into()));
            }
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(LlmResponse {
                message: BaseMessage::ai(self.response.lock().unwrap().clone()),
                stop_reason: StopReason::EndTurn,
                usage: None,
            })
        }
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_id(&self) -> &str {
            "mock-classifier"
        }
    }

    #[test]
    fn test_classification_variants() {
        assert_ne!(Classification::Allow, Classification::Deny);
        assert_ne!(Classification::Allow, Classification::Unsure);
        assert_ne!(Classification::Deny, Classification::Unsure);
        let _ = Classification::Unsure;
    }

    #[test]
    fn test_cache_key_same_input() {
        let input = serde_json::json!({"cmd": "ls"});
        let (name1, hash1) = LlmAutoClassifier::cache_key("Bash", &input);
        let (name2, hash2) = LlmAutoClassifier::cache_key("Bash", &input);
        assert_eq!(name1, name2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_cache_key_different_input() {
        let input1 = serde_json::json!({"cmd": "ls"});
        let input2 = serde_json::json!({"cmd": "rm -rf /"});
        let (_, hash1) = LlmAutoClassifier::cache_key("Bash", &input1);
        let (_, hash2) = LlmAutoClassifier::cache_key("Bash", &input2);
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_classify_allow() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("ALLOW")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "ls"}))
            .await;
        assert_eq!(result, Classification::Allow);
    }

    #[tokio::test]
    async fn test_classify_deny() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("DENY")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "rm -rf /"}))
            .await;
        assert_eq!(result, Classification::Deny);
    }

    #[tokio::test]
    async fn test_classify_unsure() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("UNSURE")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "ls"}))
            .await;
        assert_eq!(result, Classification::Unsure);
    }

    #[tokio::test]
    async fn test_classify_garbage_response() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("xyz123")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "ls"}))
            .await;
        assert_eq!(result, Classification::Unsure);
    }

    #[tokio::test]
    async fn test_classify_llm_failure() {
        let mock = MockClassifyModel::new("ALLOW");
        mock.set_should_fail(true);
        let model = Arc::new(AsyncMutex::new(Box::new(mock) as Box<dyn BaseModel>));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "ls"}))
            .await;
        assert_eq!(result, Classification::Unsure);
    }

    #[tokio::test]
    async fn test_cache_hit() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("ALLOW")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::new(model);
        let input = serde_json::json!({"cmd": "ls"});
        classifier.classify("Bash", &input).await;
        // 缓存命中验证通过 cache_key + lookup_cache 间接测试
        let key = LlmAutoClassifier::cache_key("Bash", &input);
        assert!(classifier.lookup_cache(&key).is_some());
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("ALLOW")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::with_cache_ttl(model, Duration::from_millis(50));
        let input = serde_json::json!({"cmd": "ls"});
        classifier.classify("Bash", &input).await;
        // 等待缓存过期
        tokio::time::sleep(Duration::from_millis(60)).await;
        let key = LlmAutoClassifier::cache_key("Bash", &input);
        assert!(classifier.lookup_cache(&key).is_none(), "缓存应已过期");
    }

    // ─── 子串误判防护测试 ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_not_allow_should_not_match() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("NOT ALLOW")) as Box<dyn BaseModel>,
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "rm -rf /"}))
            .await;
        assert_ne!(result, Classification::Allow, "NOT ALLOW 不应被判为 Allow");
        assert_eq!(result, Classification::Unsure);
    }

    #[tokio::test]
    async fn test_disallow_should_not_match() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("DISALLOW")) as Box<dyn BaseModel>,
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "rm -rf /"}))
            .await;
        assert_ne!(result, Classification::Allow, "DISALLOW 不应被判为 Allow");
        assert_eq!(
            result,
            Classification::Unsure,
            "DISALLOW 应判为 Unsure（无独立 DENY/ALLOW）"
        );
    }

    #[tokio::test]
    async fn test_allow_as_standalone_word() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("ALLOW")) as Box<dyn BaseModel>
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "ls"}))
            .await;
        assert_eq!(result, Classification::Allow, "独立 ALLOW 应判为 Allow");
    }

    #[tokio::test]
    async fn test_i_allow_this() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("I ALLOW THIS")) as Box<dyn BaseModel>,
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "ls"}))
            .await;
        assert_eq!(result, Classification::Allow, "I ALLOW THIS 应判为 Allow");
    }

    #[tokio::test]
    async fn test_i_deny_this() {
        let model = Arc::new(AsyncMutex::new(
            Box::new(MockClassifyModel::new("I DENY THIS")) as Box<dyn BaseModel>,
        ));
        let classifier = LlmAutoClassifier::new(model);
        let result = classifier
            .classify("Bash", &serde_json::json!({"cmd": "rm -rf /"}))
            .await;
        assert_eq!(result, Classification::Deny, "I DENY THIS 应判为 Deny");
    }
}
