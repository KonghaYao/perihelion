use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;
use tokio::time::{timeout, Duration};

use super::web_common::{
    html_to_text, truncate_content, validate_url, MAX_RESPONSE_BYTES, WEB_CREDIBILITY_WARNING,
};

/// WebFetch 工具 — 抓取 URL 并返回文本内容
pub struct WebFetchTool;

const WEB_FETCH_DESCRIPTION: &str = r#"Fetches a web page by URL and returns its content as text.

Usage:
- Only http:// and https:// URLs are allowed
- HTML pages are converted to readable text; JSON is pretty-printed; plain text is returned as-is
- Binary content returns only type and size information
- Results are truncated at 2000 lines
- An optional 'prompt' parameter provides guidance for how to use the fetched content

Security:
- Internal/private/loopback IP addresses are blocked
- Maximum response size: 10MB
- Request timeout: 30 seconds
- Maximum redirects: 5"#;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseTool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        WEB_FETCH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "要抓取的完整 URL（http/https）"
                },
                "prompt": {
                    "type": "string",
                    "description": "可选。提取内容的指导提示，附在结果前供 LLM 参考"
                }
            },
            "required": ["url"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = input["url"].as_str().ok_or("Missing url parameter")?;
        let prompt = input["prompt"].as_str();

        let parsed_url = validate_url(url)?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("peri/1.0")
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let resp = client
            .get(parsed_url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        // 检查响应体大小
        if let Some(len) = resp.content_length() {
            if len > MAX_RESPONSE_BYTES {
                return Ok(format!("响应体超过 10MB 限制（{len} bytes）"));
            }
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // 读取 body（带超时）
        let body = timeout(Duration::from_secs(30), resp.text())
            .await
            .map_err(|_| "读取响应体超时（30秒）")?
            .map_err(|e| format!("读取响应体失败: {e}"))?;

        // 实际大小检查（当 content-length 不可用时）
        if body.len() as u64 > MAX_RESPONSE_BYTES {
            return Ok(format!("响应体超过 10MB 限制（{} bytes）", body.len()));
        }

        let processed = if content_type.contains("text/html") {
            html_to_text(&body)
        } else if content_type.contains("text/plain") {
            body
        } else if content_type.contains("application/json") {
            match serde_json::from_str::<Value>(&body) {
                Ok(v) => serde_json::to_string_pretty(&v).unwrap_or(body),
                Err(_) => body,
            }
        } else {
            format!(
                "Content-Type: {content_type}\nSize: {} bytes\n（不支持的内容类型）",
                body.len()
            )
        };

        let truncated = truncate_content(&processed, 2000);

        let result = match prompt {
            Some(p) => format!("{WEB_CREDIBILITY_WARNING}提示: {p}\n\n{truncated}"),
            None => format!("{WEB_CREDIBILITY_WARNING}{truncated}"),
        };

        Ok(result)
    }
}
