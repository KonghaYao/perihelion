use thiserror::Error;

#[derive(Debug, Error)]
pub enum LangfuseError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization failed: {0}")]
    JsonSerialize(#[from] serde_json::Error),

    #[error("Ingestion API returned errors: {0}")]
    IngestionApi(String),

    #[error("Batch sender dropped, batcher is shut down")]
    ChannelClosed,

    #[error("Invalid configuration: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_ingestion_api() {
        let err = LangfuseError::IngestionApi("HTTP 400: bad request".into());
        let msg = format!("{}", err);
        assert!(msg.contains("Ingestion API returned errors"), "got: {}", msg);
        assert!(msg.contains("HTTP 400"), "got: {}", msg);
    }

    #[test]
    fn test_error_display_config() {
        let err = LangfuseError::Config("test".into());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid configuration"), "got: {}", msg);
        assert!(msg.contains("test"), "got: {}", msg);
    }

    #[test]
    fn test_error_display_channel_closed() {
        let err = LangfuseError::ChannelClosed;
        let msg = format!("{}", err);
        assert!(msg.contains("shut down") || msg.contains("ChannelClosed"), "got: {}", msg);
    }

    #[test]
    fn test_error_display_json_serialize() {
        let err = LangfuseError::JsonSerialize(serde_json::from_str::<i32>("not a number").unwrap_err());
        let msg = format!("{}", err);
        assert!(msg.contains("JSON serialization failed"), "got: {}", msg);
    }
}
