use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::messages::{BaseMessage, MessageContent};
use crate::tools::BaseTool;

/// Agent 输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    /// 输入内容（支持纯文字或多模态 MessageContent）
    pub content: MessageContent,
    /// 附加参数
    pub params: HashMap<String, serde_json::Value>,
}

impl AgentInput {
    /// 纯文本输入（最常见场景）
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: MessageContent::text(text.into()),
            params: HashMap::new(),
        }
    }

    /// 多模态输入（图片 + 文字等）
    pub fn blocks(content: MessageContent) -> Self {
        Self {
            content,
            params: HashMap::new(),
        }
    }

    pub fn with_param(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// Agent 输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub text: String,
    pub steps: usize,
    pub tool_calls: Vec<(ToolCall, ToolResult)>,
}

impl AgentOutput {
    pub fn new(text: impl Into<String>, steps: usize) -> Self {
        Self {
            text: text.into(),
            steps,
            tool_calls: Vec::new(),
        }
    }
}

/// 工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
        }
    }
}

/// 工具调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub output: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        output: impl Into<String>,
    ) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            tool_name: tool_name.into(),
            output: output.into(),
            is_error: false,
        }
    }

    pub fn error(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            tool_name: tool_name.into(),
            output: message.into(),
            is_error: true,
        }
    }
}

/// LLM 推理结果（ReAct 单步）
#[derive(Debug, Clone)]
pub struct Reasoning {
    pub thought: String,
    pub tool_calls: Vec<ToolCall>,
    pub final_answer: Option<String>,
    /// 原始 LLM 响应消息（含 Reasoning/Text blocks），优先用于存 state
    pub source_message: Option<BaseMessage>,
}

impl Reasoning {
    pub fn with_tools(thought: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            thought: thought.into(),
            tool_calls,
            final_answer: None,
            source_message: None,
        }
    }

    pub fn with_answer(thought: impl Into<String>, answer: impl Into<String>) -> Self {
        Self {
            thought: thought.into(),
            tool_calls: Vec::new(),
            final_answer: Some(answer.into()),
            source_message: None,
        }
    }

    pub fn needs_tool_call(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// ReAct LLM trait
#[async_trait::async_trait]
pub trait ReactLLM: Send + Sync {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
    ) -> crate::error::AgentResult<Reasoning>;
}


