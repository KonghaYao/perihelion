use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Agent 侧发送给 relay 的事件（独立于 rust-create-agent 内部类型）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayAgentEvent {
    AiReasoning {
        text: String,
    },
    TextChunk {
        message_id: String,
        chunk: String,
    },
    ToolStart {
        message_id: String,
        tool_call_id: String,
        name: String,
        input: Value,
    },
    ToolEnd {
        message_id: String,
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    StepDone {
        step: usize,
    },
    LlmCallStart {
        step: usize,
        messages: Vec<Value>,
        tools: Vec<Value>,
    },
    LlmCallEnd {
        step: usize,
        model: String,
        output: String,
        usage: Option<Value>,
    },
}
