use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Max iterations exceeded ({0})")]
    MaxIterationsExceeded(usize),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool execution failed: {tool} - {reason}")]
    ToolExecutionFailed { tool: String, reason: String },

    #[error("LLM error: {0}")]
    LlmError(String),

    #[error("LLM HTTP 错误 ({status}): {message}")]
    LlmHttpError { status: u16, message: String },

    #[error("Middleware error: {middleware} - {reason}")]
    MiddlewareError { middleware: String, reason: String },

    #[error("Tool rejected: {tool} - {reason}")]
    ToolRejected { tool: String, reason: String },

    #[error("State error: {0}")]
    StateError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// 用户主动中断（Ctrl+C）
    #[error("Interrupted by user")]
    Interrupted,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AgentResult<T> = Result<T, AgentError>;

impl AgentError {
    /// 判断错误是否可重试（用于 LLM 调用重试机制）
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::LlmHttpError { status, .. } => {
                matches!(status, 408 | 429 | 500..=599)
            }
            Self::LlmError(msg) => {
                let msg_lower = msg.to_lowercase();
                msg_lower.contains("connection refused")
                    || msg_lower.contains("connection reset")
                    || msg_lower.contains("connection aborted")
                    || msg_lower.contains("connection timed out")
                    || msg_lower.contains("broken pipe")
                    || msg_lower.contains("timeout")
                    || msg_lower.contains("dns")
                    || msg_lower.contains("rate limit")
                    || msg_lower.contains("overloaded")
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_http_429() {
        let err = AgentError::LlmHttpError {
            status: 429,
            message: "rate limited".into(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_retryable_http_503() {
        let err = AgentError::LlmHttpError {
            status: 503,
            message: "unavailable".into(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_retryable_http_408() {
        let err = AgentError::LlmHttpError {
            status: 408,
            message: "timeout".into(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_not_retryable_http_400() {
        let err = AgentError::LlmHttpError {
            status: 400,
            message: "bad request".into(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_not_retryable_http_401() {
        let err = AgentError::LlmHttpError {
            status: 401,
            message: "unauthorized".into(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_not_retryable_http_404() {
        let err = AgentError::LlmHttpError {
            status: 404,
            message: "not found".into(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_retryable_network_connection() {
        let err = AgentError::LlmError("connection refused".into());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_retryable_connection_reset() {
        let err = AgentError::LlmError("connection reset by peer".into());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_not_retryable_connection_pool() {
        let err = AgentError::LlmError("connection pool is full".into());
        assert!(!err.is_retryable(), "connection pool 满不是临时网络错误");
    }

    #[test]
    fn test_retryable_network_timeout() {
        let err = AgentError::LlmError("reqwest timeout exceeded".into());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_not_retryable_parse_error() {
        let err = AgentError::LlmError("parse error".into());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_not_retryable_other_errors() {
        let err = AgentError::ToolNotFound("x".into());
        assert!(!err.is_retryable());
    }
}
