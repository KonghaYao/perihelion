use crate::messages::BaseMessage;
use crate::tools::ToolDefinition;

/// LLM 请求
pub struct LlmRequest {
    pub messages: Vec<BaseMessage>,
    pub tools: Vec<ToolDefinition>,
    /// Anthropic system 字段（OpenAI 通过 System 消息传递）
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl LlmRequest {
    pub fn new(messages: Vec<BaseMessage>) -> Self {
        Self {
            messages,
            tools: Vec::new(),
            system: None,
            max_tokens: None,
            temperature: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// LLM 响应
pub struct LlmResponse {
    /// Ai 变体消息
    pub message: BaseMessage,
    pub stop_reason: StopReason,
}

/// 停止原因
#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other(String),
}

impl StopReason {
    pub fn from_openai(s: &str) -> Self {
        match s {
            "stop" => Self::EndTurn,
            "tool_calls" => Self::ToolUse,
            "length" => Self::MaxTokens,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn from_anthropic(s: &str) -> Self {
        match s {
            "end_turn" => Self::EndTurn,
            "tool_use" => Self::ToolUse,
            "max_tokens" => Self::MaxTokens,
            other => Self::Other(other.to_string()),
        }
    }
}

