use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use crate::ask_user::{ask_user_tool_definition, parse_ask_user};
use rust_create_agent::agent::react::ToolCall;

// AskUserInvoker 从核心库导入并重导出
pub use rust_create_agent::ask_user::AskUserInvoker;

// ─── AskUserTool ──────────────────────────────────────────────────────────────

/// `ask_user` 工具的 BaseTool 实现
///
/// 将 ask_user LLM 工具调用转化为对 [`AskUserInvoker`] 的调用，
/// 挂起等待用户通过 UI 提供答案后恢复。
pub struct AskUserTool {
    invoker: Arc<dyn AskUserInvoker>,
}

impl AskUserTool {
    pub fn new(invoker: Arc<dyn AskUserInvoker>) -> Self {
        Self { invoker }
    }
}

#[async_trait]
impl BaseTool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        ask_user_tool_definition().description.leak()
    }

    fn parameters(&self) -> Value {
        ask_user_tool_definition().parameters
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 构造一个临时 ToolCall 来复用 parse_ask_user 的解析逻辑
        let tc = ToolCall { id: "ask_user".to_string(), name: "ask_user".to_string(), input };
        let question = parse_ask_user(&tc)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
            .ok_or("ask_user: parse returned None")?;

        let mut answers = self.invoker.ask_batch(vec![question]).await;
        Ok(answers.pop().unwrap_or_default())
    }
}
