mod anthropic;
mod openai;

pub use anthropic::AnthropicAdapter;
pub use openai::OpenAiAdapter;

use anyhow::Result;
use serde_json::Value;

use crate::messages::BaseMessage;

/// MessageAdapter trait — BaseMessage 与 provider 原生格式之间的双向转换
pub trait MessageAdapter {
    /// BaseMessage 列表 → provider 原生 JSON messages 数组
    fn from_base_messages(messages: &[BaseMessage]) -> Value;

    /// provider 原生 JSON message → BaseMessage
    fn to_base_message(value: &Value) -> Result<BaseMessage>;
}
