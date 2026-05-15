use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;

use super::web_common::WEB_CREDIBILITY_WARNING;

/// 单条结果文本截断上限（字符数）
const MAX_RESULT_TEXT_CHARS: usize = 500;

/// 搜索结果
pub(crate) struct SearchResult {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) snippet: Option<String>,
}

const WEBSEARCH_DESCRIPTION: &str = r#"Search the web using Bing search engine.

Usage:
- Provide a search query to find relevant web pages
- Returns results as a numbered Markdown list with titles, URLs, and text snippets
- Each result's text is truncated to 500 characters
- No API key required — uses Bing web search directly

IMPORTANT:
- Results may be irrelevant or low quality — always verify information before using it
- If results don't contain the information you need, do NOT fabricate or guess values
- Consider using WebFetch to directly access a specific URL for accurate information

Parameters:
- query (required): Search keywords
- num_results (optional): Number of results, default 10, max 20"#;

/// WebSearch 工具 — 通过 Bing 搜索网页
pub struct WebSearchTool;

/// Browser-like headers to avoid Bing's anti-bot JS-rendered response.
/// Header names MUST be lowercase — `http::HeaderName::from_static` panics otherwise.
/// Note: `accept-encoding` is omitted — reqwest handles decompression automatically
/// when its `gzip`/`brotli`/`deflate` features are enabled.
pub(crate) const BROWSER_HEADERS: &[(&str, &str)] = &[
    ("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0"),
    ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"),
    ("accept-language", "en-US,en;q=0.9"),
    ("cache-control", "no-cache"),
    ("pragma", "no-cache"),
    ("sec-ch-ua", r#""Microsoft Edge";v="131", "Chromium";v="131", "Not_A Brand";v="24""#),
    ("sec-ch-ua-mobile", "?0"),
    ("sec-ch-ua-platform", r#""macOS""#),
    ("sec-fetch-dest", "document"),
    ("sec-fetch-mode", "navigate"),
    ("sec-fetch-site", "none"),
    ("sec-fetch-user", "?1"),
    ("upgrade-insecure-requests", "1"),
];

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

/// HTML 实体解码（简化版，覆盖常见实体）
pub(crate) fn decode_html_entities(text: &str) -> String {
    let mut result = text.to_string();
    // 数字实体 &#1234; 和 &#x1F600;
    let re = regex::Regex::new(r"&#(x?[0-9a-fA-F]+);").unwrap();
    result = re
        .replace_all(&result, |caps: &regex::Captures| {
            let s = &caps[1];
            if let Some(hex) = s.strip_prefix('x') {
                u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map_or(String::new(), |c| c.to_string())
            } else {
                s.parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map_or(String::new(), |c| c.to_string())
            }
        })
        .to_string();
    // 命名实体
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&#39;", "'");
    result = result.replace("&apos;", "'");
    result = result.replace("&nbsp;", " ");
    result
}

/// 从 HTML 块中提取文本（去除所有标签）
fn strip_html_tags(html: &str) -> String {
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(html, "").to_string()
}

/// 解析 Bing 重定向 URL，返回实际目标 URL
pub(crate) fn resolve_bing_url(raw_url: &str) -> Option<String> {
    // 跳过相对/锚点链接
    if raw_url.starts_with('/') || raw_url.starts_with('#') {
        return None;
    }

    // 尝试从 Bing 重定向 URL 中提取 u 参数
    // Bing 格式: https://www.bing.com/ck/a?...&u=a1aHR0cHM6Ly9leGFtcGxlLmNvbQ...
    // u 参数前缀: a1=https, a0=http
    if let Some(u_match) = raw_url.find("?u=").or_else(|| raw_url.find("&u=")) {
        let encoded = &raw_url[u_match + 3..];
        let encoded = encoded.split('&').next().unwrap_or(encoded);
        if encoded.len() >= 3 {
            let b64 = &encoded[2..];
            // Base64url decode
            let padded = b64.replace('-', "+").replace('_', "/");
            if let Ok(decoded) =
                base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, padded)
            {
                if let Ok(decoded_str) = String::from_utf8(decoded) {
                    if decoded_str.starts_with("http") {
                        return Some(decoded_str);
                    }
                }
            }
        }
    }

    // 直接外部 URL（非 Bing 内部页面）
    if !raw_url.contains("bing.com") {
        return Some(raw_url.to_string());
    }

    None
}

/// 从 Bing HTML 中提取搜索结果
pub(crate) fn extract_bing_results(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();

    // 匹配 <li class="b_algo"> 块
    let block_re = regex::Regex::new(r#"(?i)<li\s+class="b_algo"[^>]*>([\s\S]*?)</li>"#).unwrap();

    // 从 <h2><a href="...">...</a></h2> 中提取链接
    let link_re =
        regex::Regex::new(r#"(?i)<h2[^>]*>\s*<a[^>]+href="([^"]+)"[^>]*>([\s\S]*?)</a>"#).unwrap();

    for caps in block_re.captures_iter(html) {
        let block = &caps[1];

        if let Some(link_caps) = link_re.captures(block) {
            let raw_url = decode_html_entities(&link_caps[1]);
            let title_html = &link_caps[2];

            let url = match resolve_bing_url(&raw_url) {
                Some(u) => u,
                None => continue,
            };

            let title = strip_html_tags(title_html).trim().to_string();
            if title.is_empty() {
                continue;
            }

            // 提取摘要：优先 b_lineclamp → b_caption <p> → b_caption fallback
            let snippet = extract_snippet(block);

            results.push(SearchResult {
                title,
                url,
                snippet,
            });
        }
    }

    results
}

fn extract_snippet(block: &str) -> Option<String> {
    // 1. <p class="b_lineclamp...">
    let re1 =
        regex::Regex::new(r#"(?i)<p[^>]*class="b_lineclamp[^"]*"[^>]*>([\s\S]*?)</p>"#).unwrap();
    if let Some(caps) = re1.captures(block) {
        let text = strip_html_tags(&caps[1]).trim().to_string();
        if !text.is_empty() {
            return Some(decode_html_entities(&text));
        }
    }

    // 2. <p> inside b_caption
    let re2 = regex::Regex::new(
        r#"(?i)<div[^>]*class="b_caption[^"]*"[^>]*>[\s\S]*?<p[^>]*>([\s\S]*?)</p>"#,
    )
    .unwrap();
    if let Some(caps) = re2.captures(block) {
        let text = strip_html_tags(&caps[1]).trim().to_string();
        if !text.is_empty() {
            return Some(decode_html_entities(&text));
        }
    }

    // 3. Fallback: any text inside b_caption <div>
    let re3 =
        regex::Regex::new(r#"(?i)<div[^>]*class="b_caption[^"]*"[^>]*>([\s\S]*?)</div>"#).unwrap();
    if let Some(caps) = re3.captures(block) {
        let text = strip_html_tags(&caps[1]).trim().to_string();
        if !text.is_empty() {
            return Some(decode_html_entities(&text));
        }
    }

    None
}

/// 将搜索结果格式化为 Markdown 编号列表
pub(crate) fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return format!("{WEB_CREDIBILITY_WARNING}No search results found.");
    }

    let mut output = format!("{WEB_CREDIBILITY_WARNING}## Search Results\n\n");
    for (i, r) in results.iter().enumerate() {
        output.push_str(&format!("{}. **{}** ({})\n", i + 1, r.title, r.url));
        if let Some(snippet) = &r.snippet {
            let truncated: String = snippet.chars().take(MAX_RESULT_TEXT_CHARS).collect();
            output.push_str(&format!("   {}\n\n", truncated.trim()));
        } else {
            output.push('\n');
        }
    }
    output
}

#[async_trait]
impl BaseTool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        WEBSEARCH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search keywords"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results, default 10, max 20"
                }
            },
            "required": ["query"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let query = input["query"]
            .as_str()
            .ok_or("Missing required parameter: query")?;
        let num_results = input["num_results"].as_u64().unwrap_or(10).clamp(1, 20) as usize;

        let search_url = format!(
            "https://www.bing.com/search?q={}&setmkt=en-US",
            urlencoding::encode(query)
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let mut headers = reqwest::header::HeaderMap::new();
        for (k, v) in BROWSER_HEADERS {
            headers.insert(
                reqwest::header::HeaderName::from_static(k),
                reqwest::header::HeaderValue::from_static(v),
            );
        }

        let resp = client
            .get(&search_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("Bing search request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("Bing search returned HTTP {status}").into());
        }

        let html = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read Bing response: {e}"))?;

        let mut results = extract_bing_results(&html);
        results.truncate(num_results);

        Ok(format_search_results(&results))
    }
}
