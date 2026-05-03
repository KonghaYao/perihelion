use super::events::OAuthCallbackResult;

/// OAuth 授权弹窗状态
pub struct OAuthPrompt {
    /// 服务器名称
    pub server_name: String,
    /// 浏览器授权 URL
    pub authorization_url: String,
    /// 用户手动粘贴的回调 URL（或含 code 的文本）
    pub input: String,
    /// 输入光标位置（字符索引）
    pub cursor: usize,
    /// 回调通道（传回后台 OAuth 流程）
    pub callback_tx: Option<tokio::sync::oneshot::Sender<OAuthCallbackResult>>,
    /// 错误提示信息（粘贴内容解析失败时显示）
    pub error_message: Option<String>,
}

impl OAuthPrompt {
    pub fn new(
        server_name: String,
        authorization_url: String,
        callback_tx: tokio::sync::oneshot::Sender<OAuthCallbackResult>,
    ) -> Self {
        Self {
            server_name,
            authorization_url,
            input: String::new(),
            cursor: 0,
            callback_tx: Some(callback_tx),
            error_message: None,
        }
    }

    /// 提交用户输入的回调 URL，返回 true 表示成功发送
    pub fn submit(&mut self) -> bool {
        use rust_agent_middlewares::mcp::parse_code_from_url;
        match parse_code_from_url(&self.input) {
            Ok((code, state)) => {
                if let Some(tx) = self.callback_tx.take() {
                    let _ = tx.send(OAuthCallbackResult { code, state });
                }
                true
            }
            Err(e) => {
                self.error_message = Some(format!("无法解析回调 URL: {}", e));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_prompt_new() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let prompt = OAuthPrompt::new("test-server".into(), "http://example.com/auth".into(), tx);
        assert!(prompt.input.is_empty());
        assert_eq!(prompt.cursor, 0);
        assert!(prompt.error_message.is_none());
        assert_eq!(prompt.server_name, "test-server");
    }

    #[test]
    fn test_oauth_prompt_submit_valid_url() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = "http://localhost:12345/callback?code=abc&state=xyz".to_string();
        assert!(prompt.submit());
        let result = rx.blocking_recv().unwrap();
        assert_eq!(result.code, "abc");
        assert_eq!(result.state, "xyz");
    }

    #[test]
    fn test_oauth_prompt_submit_full_url() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = "http://localhost:9999/callback?code=test_code&state=test_state".to_string();
        assert!(prompt.submit());
        let result = rx.blocking_recv().unwrap();
        assert_eq!(result.code, "test_code");
        assert_eq!(result.state, "test_state");
    }

    #[test]
    fn test_oauth_prompt_submit_invalid_url() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = "not a valid url".to_string();
        assert!(!prompt.submit());
        assert!(prompt.error_message.is_some());
    }

    #[test]
    fn test_oauth_prompt_submit_empty() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = String::new();
        assert!(!prompt.submit());
        assert!(prompt.error_message.is_some());
    }
}
