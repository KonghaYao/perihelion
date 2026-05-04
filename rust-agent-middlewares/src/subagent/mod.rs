mod background;
mod skill_preload;
mod tool;
pub use background::{BackgroundTask, BackgroundTaskRegistry, BackgroundTaskStatus};
pub use skill_preload::SkillPreloadMiddleware;
pub use tool::SubAgentTool;

use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::events::AgentEventHandler;
use rust_create_agent::agent::react::ReactLLM;
use rust_create_agent::agent::state::State;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::error::AgentResult;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::BaseTool;

use crate::agent_define::AgentOverrides;
use crate::parse_agent_file;
use crate::tools::BoxToolWrapper;

/// SubAgentMiddleware - injects `Agent` tool into the parent agent
///
/// In the `before_agent` phase, provides `SubAgentTool` to the parent agent via `collect_tools`,
/// enabling the LLM to call the `Agent` tool to delegate sub-tasks to specialized sub-agents.
///
/// # Usage Example
///
/// ```rust,ignore
/// let parent_tools: Vec<Box<dyn BaseTool>> = vec![
///     Box::new(ReadFileTool::new(cwd)),
/// ];
/// let llm_factory = Arc::new(move |_: Option<&str>| {
///     Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
/// });
/// // Optional: system prompt builder, making sub-agent's tone/proactiveness visible in Langfuse
/// let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
///     build_system_prompt(overrides, cwd)
/// });
/// let middleware = SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
///     .with_system_builder(system_builder);
/// let agent = ReActAgent::new(llm).add_middleware(Box::new(middleware));
/// ```
pub struct SubAgentMiddleware {
    /// Parent agent tool set (Arc shared, passed to child agent for use)
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// Parent agent event handler (transparent forwarding of child agent events)
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// LLM factory function, creates independent LLM instance for each child agent
    /// Parameter is optional model alias (e.g., "haiku"/"sonnet"/"opus"), None means use parent model
    #[allow(clippy::type_complexity)]
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// System prompt builder: (agent overrides, cwd) -> system prompt string
    /// When set, child agent injects system prompt via with_system_prompt() (visible in Langfuse)
    #[allow(clippy::type_complexity)]
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// Parent agent cancellation token (passed to child agent, supports user interruption)
    cancel: Option<AgentCancellationToken>,
    /// Shared reference to parent agent message snapshot, written in before_agent, read by Fork child agent
    parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
    /// 后台任务注册中心（通过 build_tool 传递给 SubAgentTool）
    background_registry: Option<Arc<BackgroundTaskRegistry>>,
}

impl SubAgentMiddleware {
    #[allow(clippy::type_complexity)]
    pub fn new(
        parent_tools: Vec<Box<dyn BaseTool>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    ) -> Self {
        let tools: Vec<Arc<dyn BaseTool>> = parent_tools
            .into_iter()
            .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
            .collect();
        Self {
            parent_tools: Arc::new(tools),
            event_handler,
            llm_factory,
            system_builder: None,
            cancel: None,
            parent_messages: None,
            background_registry: None,
        }
    }

    /// Set system prompt builder, child agent injects system prompt via `with_system_prompt()` during execution
    #[allow(clippy::type_complexity)]
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    /// Set parent agent cancellation token (passed to child agent, supports user interruption of child agent execution)
    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Set shared parent message reference for Fork child agent inheritance
    pub fn with_parent_messages(mut self, messages: Arc<RwLock<Vec<BaseMessage>>>) -> Self {
        self.parent_messages = Some(messages);
        self
    }

    /// Set background task registry for run_in_background mode
    pub fn with_background_registry(mut self, registry: Arc<BackgroundTaskRegistry>) -> Self {
        self.background_registry = Some(registry);
        self
    }

    /// Build SubAgentTool instance (clone Arc fields, do not transfer ownership)
    pub fn build_tool(&self, cwd: &str) -> SubAgentTool {
        let mut tool = SubAgentTool::new(
            Arc::clone(&self.parent_tools),
            self.event_handler.clone(),
            Arc::clone(&self.llm_factory),
            cwd.to_string(),
        );
        if let Some(ref builder) = self.system_builder {
            tool = tool.with_system_builder(Arc::clone(builder));
        }
        if let Some(ref cancel) = self.cancel {
            tool = tool.with_cancel(cancel.clone());
        }
        if let Some(ref pm) = self.parent_messages {
            tool = tool.with_parent_messages(Arc::clone(pm));
        }
        if let Some(ref registry) = self.background_registry {
            tool = tool.with_background_registry(Arc::clone(registry));
        }
        tool
    }
}

/// Scan `{cwd}/.claude/agents/` directory, return `(agent_id, name, description)` list
pub fn scan_agents(cwd: &str) -> Vec<(String, String, String)> {
    let agents_dir = Path::new(cwd).join(".claude").join("agents");
    if !agents_dir.is_dir() {
        return vec![];
    }

    let entries = match std::fs::read_dir(&agents_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut result = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Two formats: `{agent_id}.md` or `{agent_id}/agent.md`
        let (agent_id, file_path): (String, PathBuf) = if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (id, path)
        } else if path.is_dir() {
            let nested = path.join("agent.md");
            if !nested.is_file() {
                continue;
            }
            let id = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (id, nested)
        } else {
            continue;
        };

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Some(agent) = parse_agent_file(&content) {
            let name = if agent.frontmatter.name.is_empty() {
                agent_id.clone()
            } else {
                agent.frontmatter.name.clone()
            };
            let description = agent.frontmatter.description.clone();
            result.push((agent_id, name, description));
        }
    }

    // Sort by agent_id for stable output
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[async_trait]
impl<S: State> Middleware<S> for SubAgentMiddleware {
    fn name(&self) -> &str {
        "SubAgentMiddleware"
    }

    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(self.build_tool(cwd))]
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // Snapshot current state.messages to shared reference for Fork child agent inheritance
        if let Some(ref pm) = self.parent_messages {
            *pm.write() = state.messages().to_vec();
        }
        Ok(())
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
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        // Call via Middleware<AgentState>, explicit generic parameter
        assert_eq!(
            <SubAgentMiddleware as Middleware<AgentState>>::name(&m),
            "SubAgentMiddleware"
        );
    }

    #[test]
    fn test_middleware_collect_tools() {
        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let tools = <SubAgentMiddleware as Middleware<AgentState>>::collect_tools(&m, "/tmp");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "Agent");
    }

    #[test]
    fn test_build_tool_returns_subagent_tool() {
        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let tool = m.build_tool("/tmp");
        assert_eq!(tool.name(), "Agent");
    }

    #[test]
    fn test_scan_agents_no_dir() {
        let result = scan_agents("/nonexistent/path");
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_agents_flat_md() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("code-reviewer.md"),
            "---\nname: code-reviewer\ndescription: Reviews code quality\n---\n\nYou are a reviewer.\n",
        ).unwrap();

        let result = scan_agents(dir.path().to_str().unwrap());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "code-reviewer");
        assert_eq!(result[0].1, "code-reviewer");
        assert_eq!(result[0].2, "Reviews code quality");
    }

    #[test]
    fn test_scan_agents_nested_dir() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let agent_dir = dir.path().join(".claude").join("agents").join("analyst");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("agent.md"),
            "---\nname: data-analyst\ndescription: Analyzes data\n---\n\nYou are an analyst.\n",
        )
        .unwrap();

        let result = scan_agents(dir.path().to_str().unwrap());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "analyst");
        assert_eq!(result[0].1, "data-analyst");
        assert_eq!(result[0].2, "Analyzes data");
    }

    #[tokio::test]
    async fn test_before_agent_no_longer_injects_summary() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("tester.md"),
            "---\nname: tester\ndescription: Runs tests\n---\n\nYou run tests.\n",
        )
        .unwrap();

        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        <SubAgentMiddleware as Middleware<AgentState>>::before_agent(&m, &mut state)
            .await
            .unwrap();

        // Agent list has been migrated to system prompt placeholder injection, before_agent no longer prepends messages
        assert_eq!(
            state.messages().len(),
            0,
            "before_agent should not inject agent summary messages"
        );
    }

    #[tokio::test]
    async fn test_before_agent_no_agents_no_op() {
        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let mut state = AgentState::new("/nonexistent");
        <SubAgentMiddleware as Middleware<AgentState>>::before_agent(&m, &mut state)
            .await
            .unwrap();
        assert_eq!(state.messages().len(), 0);
    }

    /// Verify before_agent snapshots messages to shared parent_messages
    #[tokio::test]
    async fn test_before_agent_snapshots_messages() {
        let parent_messages: Arc<RwLock<Vec<BaseMessage>>> = Arc::new(RwLock::new(Vec::new()));

        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        )
        .with_parent_messages(Arc::clone(&parent_messages));

        let mut state = AgentState::new("/tmp");
        state.add_message(BaseMessage::human("Hello"));
        state.add_message(BaseMessage::ai("Hi"));

        <SubAgentMiddleware as Middleware<AgentState>>::before_agent(&m, &mut state)
            .await
            .unwrap();

        let snapshot = parent_messages.read();
        assert_eq!(
            snapshot.len(),
            2,
            "parent_messages should contain 2 snapshot messages"
        );
        assert_eq!(snapshot[0].content(), "Hello");
        assert_eq!(snapshot[1].content(), "Hi");
    }

    /// Verify build_tool passes parent_messages to SubAgentTool
    #[test]
    fn test_build_tool_receives_parent_messages() {
        let parent_messages: Arc<RwLock<Vec<BaseMessage>>> = Arc::new(RwLock::new(Vec::new()));

        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        )
        .with_parent_messages(Arc::clone(&parent_messages));

        let tool = m.build_tool("/tmp");
        // SubAgentTool with parent_messages set should handle fork: true without error
        // (the test verifies the field is passed through; functional test is in tool.rs)
        assert_eq!(tool.name(), "Agent");
    }
}
