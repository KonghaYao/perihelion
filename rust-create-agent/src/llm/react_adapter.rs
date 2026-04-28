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
        let provider = self.model.provider_name();
        let msg_count = messages.len();
        let tool_count = tools.len();
        let start = std::time::Instant::now();

        let response = self.model.invoke(request).await.map_err(|e| {
            tracing::error!(
                provider = provider,
                model = %model_name,
                elapsed_ms = start.elapsed().as_millis() as u64,
                msg_count,
                tool_count,
                error = %e,
                "generate_reasoning 失败"
            );
            e
        })?;

        tracing::debug!(
            provider = provider,
            model = %model_name,
            elapsed_ms = start.elapsed().as_millis() as u64,
            msg_count,
            stop_reason = ?response.stop_reason,
            "generate_reasoning 完成"
        );

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
            let mut text = response.message.content();
            if response.stop_reason == StopReason::MaxTokens {
                tracing::warn!("LLM 输出因 max_tokens 截断，回答可能不完整");
                text.push_str("\n\n[⚠ 回答因输出长度限制被截断]");
            }
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

    fn context_window(&self) -> u32 {
        let model = self.model.model_id();
        // Claude 系列: 200K
        if model.contains("claude") { return 200_000; }
        // DeepSeek 系列: 128K
        if model.starts_with("deepseek") { return 128_000; }
        // GPT-4o / o-series: 128K
        if model.contains("gpt-4o") || model.starts_with("o1") || model.starts_with("o3") { return 128_000; }
        // GPT-4-turbo: 128K
        if model.contains("gpt-4-turbo") { return 128_000; }
        // 默认: 200K
        200_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBaseModel { id: &'static str }
    #[async_trait::async_trait]
    impl super::super::BaseModel for MockBaseModel {
        async fn invoke(&self, _: super::super::types::LlmRequest) -> crate::error::AgentResult<super::super::types::LlmResponse> { unimplemented!() }
        fn provider_name(&self) -> &str { "mock" }
        fn model_id(&self) -> &str { self.id }
    }

    #[test]
    fn test_context_window_claude() {
        let llm = BaseModelReactLLM::new(Box::new(MockBaseModel { id: "claude-sonnet-4-20250514" }));
        assert_eq!(llm.context_window(), 200_000);
    }

    #[test]
    fn test_context_window_deepseek() {
        let llm = BaseModelReactLLM::new(Box::new(MockBaseModel { id: "deepseek-r1" }));
        assert_eq!(llm.context_window(), 128_000);
    }

    #[test]
    fn test_context_window_gpt4o() {
        let llm = BaseModelReactLLM::new(Box::new(MockBaseModel { id: "gpt-4o" }));
        assert_eq!(llm.context_window(), 128_000);
    }

    #[test]
    fn test_context_window_default() {
        let llm = BaseModelReactLLM::new(Box::new(MockBaseModel { id: "unknown-model" }));
        assert_eq!(llm.context_window(), 200_000);
    }
}
