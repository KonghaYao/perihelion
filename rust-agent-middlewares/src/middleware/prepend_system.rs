use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::AgentResult;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::middleware::r#trait::Middleware;

/// PrependSystemMiddleware - 在 before_agent 阶段将固定 system 内容注入 state 消息列表
///
/// 与直接使用 `BaseModelReactLLM::with_system()` 不同，此中间件将内容作为
/// `BaseMessage::System` 写入 state，使其对外可见（如 Langfuse 追踪、日志）。
///
/// 注入的 System 消息会与 Anthropic adapter 的 `request.system` 字段合并：
/// `system_from_msgs(本中间件) + "\n\n" + request_system`。
///
/// # 废弃说明
///
/// 请改用 `ReActAgent::with_system_prompt()`，它在 executor 内部固定于
/// 所有中间件 `before_agent` 执行完毕之后 prepend，无顺序约束。
///
/// 本类型保留用于需要动态 system prompt 或其他高级场景。
#[deprecated(since = "0.2.0", note = "改用 ReActAgent::with_system_prompt()")]
pub struct PrependSystemMiddleware {
    content: String,
}

#[allow(deprecated)]
impl PrependSystemMiddleware {
    pub fn new(content: impl Into<String>) -> Self {
        Self { content: content.into() }
    }
}

#[allow(deprecated)]
#[async_trait]
impl<S: State> Middleware<S> for PrependSystemMiddleware {
    fn name(&self) -> &str {
        "PrependSystemMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        if !self.content.is_empty() {
            state.prepend_message(BaseMessage::system(self.content.as_str()));
        }
        Ok(())
    }
}
