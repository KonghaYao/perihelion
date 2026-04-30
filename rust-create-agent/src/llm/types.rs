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

/// Token 使用量（来自 LLM API 响应，用于 Langfuse Generation 追踪）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Anthropic Prompt Cache：写入缓存的 token 数（首次缓存）
    pub cache_creation_input_tokens: Option<u32>,
    /// Anthropic Prompt Cache：命中缓存读取的 token 数
    pub cache_read_input_tokens: Option<u32>,
}

/// LLM 响应
pub struct LlmResponse {
    /// Ai 变体消息
    pub message: BaseMessage,
    pub stop_reason: StopReason,
    /// Token 使用量（可选，不支持的 LLM 为 None）
    pub usage: Option<TokenUsage>,
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
