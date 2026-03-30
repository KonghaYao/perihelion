use crate::error::LangfuseError;
use crate::types::{IngestionEvent, IngestionResponse};
use base64::Engine;
use reqwest::Client;
use std::time::Duration;
use tracing::warn;

/// Langfuse V4 Ingestion API 底层客户端
///
/// 持有 reqwest::Client（复用连接池），封装认证、请求构建、重试逻辑。
#[derive(Clone)]
pub struct LangfuseClient {
    http: Client,
    base_url: String,
    auth_header: String,
    max_retries: usize,
}

impl LangfuseClient {
    /// 构造 LangfuseClient
    ///
    /// - `public_key`: Langfuse 公钥
    /// - `secret_key`: Langfuse 秘钥
    /// - `base_url`: Langfuse 服务地址（如 "https://cloud.langfuse.com"）
    /// - `max_retries`: 网络错误最大重试次数（0 = 不重试）
    pub fn new(public_key: &str, secret_key: &str, base_url: &str, max_retries: usize) -> Self {
        let credentials = format!("{}:{}", public_key, secret_key);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        let auth_header = format!("Basic {}", encoded);

        // 配置 reqwest Client 超时：连接超时 5s，请求超时 30s
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");

        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header,
            max_retries,
        }
    }

    /// 从 ClientConfig 构造（便捷方法）
    pub fn from_config(config: &crate::config::ClientConfig, max_retries: usize) -> Self {
        Self::new(&config.public_key, &config.secret_key, &config.base_url, max_retries)
    }

    /// 发送一批 ingestion 事件到 Langfuse API
    ///
    /// POST /api/public/ingestion
    /// Headers:
    ///   - Authorization: Basic {base64(public_key:secret_key)}
    ///   - Content-Type: application/json
    ///   - x-langfuse-ingestion-version: 4
    ///
    /// 响应: 207 Multi-Status → 解析 IngestionResponse
    /// 错误重试: 网络错误（连接失败、超时等）自动重试 max_retries 次，指数退避（1s, 2s, 4s...）
    /// 4xx 错误不重试，直接返回 LangfuseError::IngestionApi
    pub async fn ingest(
        &self,
        batch: Vec<IngestionEvent>,
    ) -> Result<IngestionResponse, LangfuseError> {
        let url = format!("{}/api/public/ingestion", self.base_url);
        let body = serde_json::json!({ "batch": batch });

        let mut attempt = 0;
        loop {
            let result = self
                .http
                .post(&url)
                .header("Authorization", &self.auth_header)
                .header("Content-Type", "application/json")
                .header("x-langfuse-ingestion-version", "4")
                .json(&body)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() || status.as_u16() == 207 {
                        // 207 Multi-Status 或 2xx: 解析响应体
                        let response_text = response.text().await?;
                        let ingestion_response: IngestionResponse =
                            serde_json::from_str(&response_text)?;

                        // 如果有错误项，记录 warn 日志但仍返回
                        if !ingestion_response.errors.is_empty() {
                            warn!(
                                "Langfuse ingestion partial failure: {} errors out of {} events",
                                ingestion_response.errors.len(),
                                ingestion_response.successes.len() + ingestion_response.errors.len()
                            );
                        }

                        return Ok(ingestion_response);
                    } else if status.is_client_error() {
                        // 4xx: 不重试
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(LangfuseError::IngestionApi(format!(
                            "HTTP {}: {}",
                            status, error_text
                        )));
                    } else {
                        // 5xx: 可重试
                        let error_text = response.text().await.unwrap_or_default();
                        if attempt < self.max_retries {
                            attempt += 1;
                            let delay = Duration::from_secs(1 << (attempt - 1));
                            warn!(
                                "Langfuse ingestion server error (attempt {}/{}), retrying in {:?}: HTTP {} {}",
                                attempt, self.max_retries, delay, status, error_text
                            );
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        return Err(LangfuseError::IngestionApi(format!(
                            "HTTP {} after {} retries: {}",
                            status, self.max_retries, error_text
                        )));
                    }
                }
                Err(e) => {
                    // 网络错误: 可重试
                    if attempt < self.max_retries {
                        attempt += 1;
                        let delay = Duration::from_secs(1 << (attempt - 1));
                        warn!(
                            "Langfuse ingestion network error (attempt {}/{}), retrying in {:?}: {}",
                            attempt, self.max_retries, delay, e
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(LangfuseError::Http(e));
                }
            }
        }
    }

    /// 便利方法：发送单个 ingestion 事件
    pub async fn ingest_single(
        &self,
        event: IngestionEvent,
    ) -> Result<IngestionResponse, LangfuseError> {
        self.ingest(vec![event]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TraceBody;

    fn create_test_client(server_url: &str, max_retries: usize) -> LangfuseClient {
        LangfuseClient::new("pk", "sk", server_url, max_retries)
    }

    fn create_test_event(id: &str) -> IngestionEvent {
        IngestionEvent::TraceCreate {
            id: id.to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            body: TraceBody {
                id: Some(format!("trace-{}", id)),
                name: Some("test".into()),
                ..Default::default()
            },
            metadata: None,
        }
    }

    fn create_207_response() -> String {
        r#"{"successes":[{"id":"evt-1","status":200}],"errors":[]}"#.to_string()
    }

    #[test]
    fn test_new_creates_client_with_correct_auth() {
        let client = create_test_client("http://localhost", 3);
        assert_eq!(client.auth_header, "Basic cGs6c2s=");
        assert_eq!(client.base_url, "http://localhost");
        assert_eq!(client.max_retries, 3);
    }

    #[test]
    fn test_new_trims_trailing_slash() {
        let client = create_test_client("http://localhost/", 0);
        assert_eq!(client.base_url, "http://localhost");
    }

    #[tokio::test]
    async fn test_ingest_success_207() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(create_207_response())
            .match_header("Authorization", "Basic cGs6c2s=")
            .match_header("x-langfuse-ingestion-version", "4")
            .match_header("Content-Type", "application/json")
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 0);
        let result = client.ingest(vec![create_test_event("evt-1")]).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.successes.len(), 1);
        assert_eq!(resp.errors.len(), 0);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_partial_failure_207() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[{"id":"1","status":200}],"errors":[{"id":"2","status":400,"message":"invalid","error":null}]}"#)
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 0);
        let result = client.ingest(vec![create_test_event("1"), create_test_event("2")]).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.successes.len(), 1);
        assert_eq!(resp.errors.len(), 1);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_4xx_no_retry() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(400)
            .with_body(r#"{"error":"bad request"}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 3);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            LangfuseError::IngestionApi(msg) => {
                assert!(msg.contains("HTTP 400"));
            }
            other => panic!("Expected IngestionApi, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_5xx_retries_then_success() {
        let mut server = mockito::Server::new_async().await;
        let mock_fail1 = server.mock("POST", "/api/public/ingestion")
            .with_status(500)
            .with_body("internal error")
            .expect(1)
            .create_async()
            .await;
        let mock_fail2 = server.mock("POST", "/api/public/ingestion")
            .with_status(500)
            .with_body("internal error")
            .expect(1)
            .create_async()
            .await;
        let mock_success = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(create_207_response())
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 3);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_ok());
        mock_fail1.assert_async().await;
        mock_fail2.assert_async().await;
        mock_success.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_5xx_retries_exhausted() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(500)
            .with_body("internal error")
            .expect(3) // 1 initial + 2 retries
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 2);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            LangfuseError::IngestionApi(msg) => {
                assert!(msg.contains("after 2 retries"));
            }
            other => panic!("Expected IngestionApi, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_network_error_retries() {
        // Connect to a port that's not listening
        let client = LangfuseClient::new("pk", "sk", "http://127.0.0.1:1", 1);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LangfuseError::Http(_)));
    }

    #[tokio::test]
    async fn test_ingest_single_convenience() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(create_207_response())
            .match_body(mockito::Matcher::Regex("\"batch\":\\[".to_string()))
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 0);
        let result = client.ingest_single(create_test_event("evt-1")).await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_empty_batch() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[],"errors":[]}"#)
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 0);
        let result = client.ingest(vec![]).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.successes.is_empty());
        assert!(resp.errors.is_empty());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_request_body_format() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(create_207_response())
            .match_body(mockito::Matcher::Regex("\"batch\".*\"type\":\"trace-create\"".to_string()))
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 0);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[test]
    fn test_from_config() {
        let config = crate::config::ClientConfig {
            public_key: "pk".into(),
            secret_key: "sk".into(),
            base_url: "https://cloud.langfuse.com".into(),
        };
        let client = LangfuseClient::from_config(&config, 2);
        assert_eq!(client.auth_header, "Basic cGs6c2s=");
        assert_eq!(client.max_retries, 2);
    }
}
