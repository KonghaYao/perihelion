use async_trait::async_trait;

use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::error::AgentResult;
use crate::llm::types::{LlmRequest, StopReason};
use crate::messages::{BaseMessage, ContentBlock};
use crate::tools::BaseTool;
use super::BaseModel;

/// BaseModelReactLLM - 将 BaseModel 适配为 ReactLLM
pub struct BaseModelReactLLM {
    pub model: Box<dyn BaseModel>,
    pub system: Option<String>,
}

impl BaseModelReactLLM {
    pub fn new(model: Box<dyn BaseModel>) -> Self {
        Self { model, system: None }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }
}

#[async_trait]
impl ReactLLM for BaseModelReactLLM {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
    ) -> AgentResult<Reasoning> {
        let tool_defs = tools.iter().map(|t| t.definition()).collect();

        let mut request = LlmRequest::new(messages.to_vec()).with_tools(tool_defs);

        if let Some(system) = &self.system {
            request = request.with_system(system.clone());
        }

        let model_name = self.model.model_id().to_string();
        let response = self.model.invoke(request).await?;
        let usage = response.usage.clone();

        if response.stop_reason == StopReason::ToolUse {
            // 从 content_blocks() 提取 ToolUse blocks（跨 provider 兼容）
            let blocks = response.message.content_blocks();
            let thought = blocks
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("");

            let calls: Vec<ToolCall> = blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolUse { id, name, input } = b {
                        Some(ToolCall::new(id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            if !calls.is_empty() {
                let mut r = Reasoning::with_tools(thought, calls);
                r.source_message = Some(response.message);
                r.usage = usage;
                r.model = model_name;
                return Ok(r);
            }

            // fallback：从 tool_calls() 读（兼容旧路径）
            let calls: Vec<ToolCall> = response
                .message
                .tool_calls()
                .iter()
                .map(|tc| ToolCall::new(tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                .collect();
            let mut r = Reasoning::with_tools(thought, calls);
            r.source_message = Some(response.message);
            r.usage = usage;
            r.model = model_name;
            Ok(r)
        } else {
            // 最终答案：text_content() 提取所有文字（跳过 reasoning block）
            let text = response.message.content();
            let mut r = Reasoning::with_answer("", text);
            r.source_message = Some(response.message);
            r.usage = usage;
            r.model = model_name;
            Ok(r)
        }
    }

    fn model_name(&self) -> String {
        self.model.model_id().to_string()
    }
}
