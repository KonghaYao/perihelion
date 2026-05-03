use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::{info, warn};

const CALLBACK_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Error)]
pub enum CallbackError {
    #[error("回调服务器绑定失败: {0}")]
    BindFailed(String),
    #[error("回调服务器 IO 错误: {0}")]
    IoError(#[from] std::io::Error),
    #[error("回调服务器等待超时")]
    Timeout,
    #[error("回调 URL 解析失败: {0}")]
    ParseFailed(String),
}

pub struct OAuthCallbackServer {
    code_tx: Option<oneshot::Sender<(String, String)>>,
    code_rx: oneshot::Receiver<(String, String)>,
    listener: TcpListener,
    state_param: Arc<String>,
}

impl OAuthCallbackServer {
    pub async fn bind(state_param: String) -> Result<(Self, String), CallbackError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| CallbackError::BindFailed(e.to_string()))?;
        let addr = listener.local_addr().map_err(|e| CallbackError::BindFailed(e.to_string()))?;
        let redirect_uri = format!("http://{}/callback", addr);
        info!("OAuth 回调服务器已启动: {}", redirect_uri);
        let (code_tx, code_rx) = oneshot::channel();
        Ok((Self { code_tx: Some(code_tx), code_rx, listener, state_param: Arc::new(state_param) }, redirect_uri))
    }

    pub async fn wait_for_code(mut self) -> Result<(String, String), CallbackError> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS),
            self.wait_inner(),
        ).await;
        match result {
            Ok(Ok(pair)) => Ok(pair),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(CallbackError::Timeout),
        }
    }

    async fn wait_inner(&mut self) -> Result<(String, String), CallbackError> {
        let (mut stream, addr) = self.listener.accept().await.map_err(|e| CallbackError::IoError(e))?;
        info!("OAuth 回调服务器收到连接: {}", addr);

        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await.map_err(|e| CallbackError::IoError(e))?;

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await.map_err(|e| CallbackError::IoError(e))?;
            if line == "\r\n" || line == "\n" { break; }
        }

        let url_path = request_line.split_whitespace().nth(1).unwrap_or("");
        let callback_result = parse_callback_url(url_path, &self.state_param);

        let response = match &callback_result {
            Ok((code, _)) => { info!(code = %code, "OAuth 回调成功"); "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>OAuth 授权成功</h1><p>您可以关闭此窗口并返回终端。</p></body></html>" }
            Err(e) => { warn!(error = %e, "OAuth 回调处理失败"); &format!("HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>OAuth 授权失败</h1><p>{}</p></body></html>", e)[..] }
        };

        let resp_bytes = response.as_bytes();
        let resp_vec: Vec<u8> = resp_bytes.to_vec();
        stream.write_all(&resp_vec).await.map_err(|e| CallbackError::IoError(e))?;
        stream.shutdown().await.map_err(|e| CallbackError::IoError(e))?;

        let (code, state) = callback_result?;
        if let Some(tx) = self.code_tx.take() {
            let _ = tx.send((code, state));
        }
        self.code_rx.await.map_err(|_| CallbackError::ParseFailed("回调通道已关闭".into()))
    }
}

pub fn parse_callback_url(url_path: &str, expected_state: &str) -> Result<(String, String), CallbackError> {
    let url_str = if url_path.starts_with('/') { &format!("http://localhost{}", url_path)[..] } else { url_path };
    let parsed: url::Url = url_str.parse().map_err(|e| CallbackError::ParseFailed(format!("URL 解析失败: {}", e)))?;
    let pairs: HashMap<String, String> = parsed.query_pairs().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let code = pairs.get("code").ok_or_else(|| CallbackError::ParseFailed("回调 URL 缺少 code 参数".into()))?.clone();
    let state = pairs.get("state").ok_or_else(|| CallbackError::ParseFailed("回调 URL 缺少 state 参数".into()))?.clone();
    if state != expected_state {
        return Err(CallbackError::ParseFailed(format!("CSRF state 不匹配: 期望 {}, 收到 {}", expected_state, state)));
    }
    Ok((code, state))
}

pub fn parse_code_from_url(url: &str) -> Result<(String, String), CallbackError> {
    let parsed: url::Url = url.parse().map_err(|e| CallbackError::ParseFailed(format!("URL 解析失败: {}", e)))?;
    let pairs: HashMap<std::borrow::Cow<str>, std::borrow::Cow<str>> = parsed.query_pairs().collect();
    let code = pairs.get("code").ok_or_else(|| CallbackError::ParseFailed("URL 缺少 code 参数".into()))?.to_string();
    let state = pairs.get("state").ok_or_else(|| CallbackError::ParseFailed("URL 缺少 state 参数".into()))?.to_string();
    Ok((code, state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_failed_error_format() {
        let err = CallbackError::BindFailed("addr in use".to_string());
        assert!(err.to_string().contains("绑定失败"));
    }

    #[test]
    fn test_bind_returns_valid_redirect_uri() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(OAuthCallbackServer::bind("test-state".to_string()));
        assert!(result.is_ok());
        let (_server, uri) = result.unwrap();
        assert!(uri.starts_with("http://127.0.0.1:"));
        assert!(uri.ends_with("/callback"));
    }

    #[test]
    fn test_parse_callback_url_valid() {
        let result = parse_callback_url("/callback?code=abc123&state=mystate", "mystate");
        assert!(result.is_ok());
        let (code, state) = result.unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "mystate");
    }

    #[test]
    fn test_parse_callback_url_missing_code() {
        let result = parse_callback_url("/callback?state=mystate", "mystate");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_callback_url_missing_state() {
        let result = parse_callback_url("/callback?code=abc123", "mystate");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_callback_url_invalid_path() {
        let result = parse_callback_url("not-a-url", "mystate");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_callback_url_state_mismatch() {
        let result = parse_callback_url("/callback?code=abc&state=wrong", "correct");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_code_from_url_valid() {
        let result = parse_code_from_url("http://localhost:12345/callback?code=xyz&state=s");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_code_timeout() {
        let (server, _uri) = OAuthCallbackServer::bind("state".to_string()).await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), server.wait_for_code()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_bind_multiple_servers() {
        let (s1, uri1) = OAuthCallbackServer::bind("state1".to_string()).await.unwrap();
        let (s2, uri2) = OAuthCallbackServer::bind("state2".to_string()).await.unwrap();
        assert_ne!(uri1, uri2);
        drop(s1);
        drop(s2);
    }
}
