use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::error::AgentResult;
use crate::messages::BaseMessage;
use crate::tools::BaseTool;

/// Mock ReactLLM - 用于测试，按预设脚本返回推理结果
pub struct MockLLM {
    script: Arc<Mutex<Vec<Reasoning>>>,
    index: Arc<Mutex<usize>>,
}

impl MockLLM {
    pub fn new(script: Vec<Reasoning>) -> Self {
        Self {
            script: Arc::new(Mutex::new(script)),
            index: Arc::new(Mutex::new(0)),
        }
    }

    pub fn always_answer(answer: impl Into<String>) -> Self {
        let answer = answer.into();
        Self::new(vec![Reasoning::with_answer("Thinking...", answer)])
    }

    pub fn tool_then_answer(
        tool_name: impl Into<String>,
        tool_input: serde_json::Value,
        answer: impl Into<String>,
    ) -> Self {
        let call = ToolCall::new("call_1", tool_name, tool_input);
        Self::new(vec![
            Reasoning::with_tools("I need to use a tool", vec![call]),
            Reasoning::with_answer("Based on the tool result", answer),
        ])
    }
}

#[async_trait]
impl ReactLLM for MockLLM {
    async fn generate_reasoning(
        &self,
        _messages: &[BaseMessage],
        _tools: &[&dyn BaseTool],
    ) -> AgentResult<Reasoning> {
        let script = self.script.lock().unwrap();
        let mut index = self.index.lock().unwrap();

        let reasoning = if *index < script.len() {
            script[*index].clone()
        } else {
            script
                .last()
                .cloned()
                .unwrap_or_else(|| Reasoning::with_answer("(no more script)", "Done"))
        };

        *index += 1;
        Ok(reasoning)
    }
}
