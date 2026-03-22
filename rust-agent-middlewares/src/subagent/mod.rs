mod tool;
pub use tool::SubAgentTool;

use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::events::AgentEventHandler;
use rust_create_agent::agent::react::ReactLLM;
use rust_create_agent::agent::state::State;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::BaseTool;

/// SubAgentMiddleware - 向父 agent 注入 `launch_agent` 工具
///
/// 在 `before_agent` 阶段通过 `collect_tools` 将 `SubAgentTool` 提供给父 agent，
/// 使 LLM 可调用 `launch_agent` 工具将子任务委派给专门的子 agent。
///
/// # 使用示例
///
/// ```rust,ignore
/// let parent_tools: Arc<Vec<Arc<dyn BaseTool>>> = Arc::new(vec![
///     Arc::new(ReadFileTool::new(cwd)),
/// ]);
/// let llm_factory = Arc::new(move || {
///     Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
/// });
/// let middleware = SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory);
/// let agent = ReActAgent::new(llm).add_middleware(Box::new(middleware));
/// ```
pub struct SubAgentMiddleware {
    /// 父 agent 工具集（Arc 共享，传给子 agent 使用）
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// 父 agent 事件处理器（子 agent 事件透传）
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// LLM 工厂函数，每次为子 agent 创建独立 LLM 实例
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
}

impl SubAgentMiddleware {
    pub fn new(
        parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    ) -> Self {
        Self {
            parent_tools,
            event_handler,
            llm_factory,
        }
    }

    /// 构建 SubAgentTool 实例（克隆 Arc 字段，不转移所有权）
    pub fn build_tool(&self) -> SubAgentTool {
        SubAgentTool::new(
            Arc::clone(&self.parent_tools),
            self.event_handler.clone(),
            Arc::clone(&self.llm_factory),
        )
    }
}

#[async_trait]
impl<S: State> Middleware<S> for SubAgentMiddleware {
    fn name(&self) -> &str {
        "SubAgentMiddleware"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(self.build_tool())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::react::{ReactLLM, Reasoning};
    use rust_create_agent::agent::state::AgentState;
    use rust_create_agent::messages::BaseMessage;
    use rust_create_agent::middleware::r#trait::Middleware;

    struct EchoLLM;

    #[async_trait::async_trait]
    impl ReactLLM for EchoLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> rust_create_agent::error::AgentResult<Reasoning> {
            let last = messages.last().map(|m| m.content()).unwrap_or_default();
            Ok(Reasoning::with_answer("", format!("echo: {}", last)))
        }
    }

    #[test]
    fn test_middleware_name() {
        let m = SubAgentMiddleware::new(
            Arc::new(vec![]),
            None,
            Arc::new(|| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        // 通过 Middleware<AgentState> 调用，明确泛型参数
        assert_eq!(<SubAgentMiddleware as Middleware<AgentState>>::name(&m), "SubAgentMiddleware");
    }

    #[test]
    fn test_middleware_collect_tools() {
        let m = SubAgentMiddleware::new(
            Arc::new(vec![]),
            None,
            Arc::new(|| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let tools = <SubAgentMiddleware as Middleware<AgentState>>::collect_tools(&m, "/tmp");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "launch_agent");
    }

    #[test]
    fn test_build_tool_returns_subagent_tool() {
        let m = SubAgentMiddleware::new(
            Arc::new(vec![]),
            None,
            Arc::new(|| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let tool = m.build_tool();
        assert_eq!(tool.name(), "launch_agent");
    }
}
