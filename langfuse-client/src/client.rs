use crate::error::LangfuseError;
use crate::types::{ingestion_events_to_otel, IngestionEvent};
use base64::Engine;
use reqwest::Client;
use std::time::Duration;
use tracing::warn;

/// Langfuse OTLP Ingestion 客户端
///
/// 通过 OpenTelemetry OTLP 端点（/api/public/otel/v1/traces）发送追踪数据。
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

    /// 发送一批事件到 Langfuse OTLP 端点
    ///
    /// POST /api/public/otel/v1/traces
    /// 将 IngestionEvent 批量转换为 OTLP resourceSpans 格式发送。
    /// Headers:
    ///   - Authorization: Basic {base64(public_key:secret_key)}
    ///   - Content-Type: application/json
    ///   - x-langfuse-ingestion-version: 4
    ///
    /// 响应: 200 OK（空对象）表示成功
    /// 错误重试: 网络错误和 5xx 自动重试 max_retries 次，指数退避（1s, 2s, 4s...）
    /// 4xx 错误不重试，直接返回 LangfuseError::IngestionApi
    pub async fn ingest(
        &self,
        events: Vec<IngestionEvent>,
    ) -> Result<(), LangfuseError> {
        if events.is_empty() {
            return Ok(());
        }

        let url = format!("{}/api/public/otel/v1/traces", self.base_url);
        let otel_payload = ingestion_events_to_otel(&events);

        let mut attempt = 0;
        loop {
            let result = self
                .http
                .post(&url)
                .header("Authorization", &self.auth_header)
                .header("Content-Type", "application/json")
                .header("x-langfuse-ingestion-version", "4")
                .json(&otel_payload)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let _ = response.bytes().await;
                        return Ok(());
                    } else if status.is_client_error() {
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(LangfuseError::IngestionApi(format!(
                            "OTLP ingestion HTTP {}: {}",
                            status, error_text
                        )));
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        if attempt < self.max_retries {
                            attempt += 1;
                            let delay = Duration::from_secs(1 << (attempt - 1));
                            warn!(
                                "OTLP ingestion server error (attempt {}/{}), retrying in {:?}: HTTP {} {}",
                                attempt, self.max_retries, delay, status, error_text
                            );
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        return Err(LangfuseError::IngestionApi(format!(
                            "OTLP ingestion HTTP {} after {} retries: {}",
                            status, self.max_retries, error_text
                        )));
                    }
                }
                Err(e) => {
                    if attempt < self.max_retries {
                        attempt += 1;
                        let delay = Duration::from_secs(1 << (attempt - 1));
                        warn!(
                            "OTLP ingestion network error (attempt {}/{}), retrying in {:?}: {}",
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
    async fn test_ingest_success_200() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/otel/v1/traces")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .match_header("Authorization", "Basic cGs6c2s=")
            .match_header("x-langfuse-ingestion-version", "4")
            .match_header("Content-Type", "application/json")
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 0);
        let result = client.ingest(vec![create_test_event("evt-1")]).await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_empty_batch() {
        let client = create_test_client("http://unused", 0);
        let result = client.ingest(vec![]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ingest_4xx_no_retry() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/otel/v1/traces")
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
                assert!(msg.contains("OTLP"));
                assert!(msg.contains("HTTP 400"));
            }
            other => panic!("Expected IngestionApi, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_5xx_retries_then_success() {
        let mut server = mockito::Server::new_async().await;
        let mock_fail = server.mock("POST", "/api/public/otel/v1/traces")
            .with_status(500)
            .with_body("internal error")
            .expect(1)
            .create_async()
            .await;
        let mock_success = server.mock("POST", "/api/public/otel/v1/traces")
            .with_status(200)
            .with_body("{}")
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url(), 3);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_ok());
        mock_fail.assert_async().await;
        mock_success.assert_async().await;
    }

    #[tokio::test]
    async fn test_ingest_5xx_retries_exhausted() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/otel/v1/traces")
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
        let client = LangfuseClient::new("pk", "sk", "http://127.0.0.1:1", 1);
        let result = client.ingest(vec![create_test_event("1")]).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LangfuseError::Http(_)));
    }

    #[tokio::test]
    async fn test_ingest_payload_has_otel_format() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/otel/v1/traces")
            .with_status(200)
            .with_body("{}")
            .match_body(mockito::Matcher::Regex("\"resourceSpans\".*\"scopeSpans\".*\"spans\"".to_string()))
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
