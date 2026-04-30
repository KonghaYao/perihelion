mod tool;
mod skill_preload;
pub use tool::SubAgentTool;
pub use skill_preload::SkillPreloadMiddleware;

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

/// SubAgentMiddleware - 向父 agent 注入 `launch_agent` 工具
///
/// 在 `before_agent` 阶段通过 `collect_tools` 将 `SubAgentTool` 提供给父 agent，
/// 使 LLM 可调用 `launch_agent` 工具将子任务委派给专门的子 agent。
///
/// # 使用示例
///
/// ```rust,ignore
/// let parent_tools: Vec<Box<dyn BaseTool>> = vec![
///     Box::new(ReadFileTool::new(cwd)),
/// ];
/// let llm_factory = Arc::new(move |_: Option<&str>| {
///     Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
/// });
/// // 可选：系统提示构建器，使子 agent 的 tone/proactiveness 在 Langfuse 中可见
/// let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
///     build_system_prompt(overrides, cwd)
/// });
/// let middleware = SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
///     .with_system_builder(system_builder);
/// let agent = ReActAgent::new(llm).add_middleware(Box::new(middleware));
/// ```
pub struct SubAgentMiddleware {
    /// 父 agent 工具集（Arc 共享，传给子 agent 使用）
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// 父 agent 事件处理器（子 agent 事件透传）
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// LLM 工厂函数，每次为子 agent 创建独立 LLM 实例
    /// 参数为可选的 model alias（如 "haiku"/"sonnet"/"opus"），None 时使用父模型
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// 系统提示构建器：(agent overrides, cwd) → system prompt 字符串
    /// 设置后，子 agent 通过 with_system_prompt() 注入系统提示（Langfuse 可见）
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// 父 agent 取消令牌（传递给子 agent，支持用户中断）
    cancel: Option<AgentCancellationToken>,
}

impl SubAgentMiddleware {
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
        }
    }

    /// 设置系统提示构建器，子 agent 执行时通过 `with_system_prompt()` 注入系统提示词
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    /// 设置父 agent 取消令牌（传递给子 agent，支持用户中断子 agent 执行）
    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// 构建 SubAgentTool 实例（克隆 Arc 字段，不转移所有权）
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
        tool
    }
}

/// 扫描 `{cwd}/.claude/agents/` 目录，返回 `(agent_id, name, description)` 列表
fn scan_agents(cwd: &str) -> Vec<(String, String, String)> {
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

        // 两种格式：`{agent_id}.md` 或 `{agent_id}/agent.md`
        let (agent_id, file_path): (String, PathBuf) = if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let id = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            (id, path)
        } else if path.is_dir() {
            let nested = path.join("agent.md");
            if !nested.is_file() {
                continue;
            }
            let id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            (id, nested)
        } else {
            continue;
        };

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Some(agent) = parse_agent_file(&content) {
            let name = if agent.frontmatter.name.is_empty() { agent_id.clone() } else { agent.frontmatter.name.clone() };
            let description = agent.frontmatter.description.clone();
            result.push((agent_id, name, description));
        }
    }

    // 按 agent_id 排序保证稳定输出
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// 生成 agents 摘要系统消息
fn build_agents_summary(agents: &[(String, String, String)]) -> String {
    let mut lines = vec![
        "你可以使用 `launch_agent` 工具委派子任务给以下专门 Agent：".to_string(),
        String::new(),
    ];

    for (agent_id, name, description) in agents {
        lines.push(format!("- **{}** (`{}`): {}", name, agent_id, description));
    }

    lines.push(String::new());
    lines.push("调用时传入 `agent_id` 字段（括号内的标识符）和 `task` 字段（任务描述）。".to_string());

    lines.join("\n")
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
        let cwd = state.cwd().to_string();
        let agents = tokio::task::spawn_blocking(move || scan_agents(&cwd))
            .await
            .map_err(|e| rust_create_agent::error::AgentError::MiddlewareError {
                middleware: "SubAgentMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?;

        if agents.is_empty() {
            return Ok(());
        }

        let summary = build_agents_summary(&agents);
        state.prepend_message(BaseMessage::system(summary));

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
        // 通过 Middleware<AgentState> 调用，明确泛型参数
        assert_eq!(<SubAgentMiddleware as Middleware<AgentState>>::name(&m), "SubAgentMiddleware");
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
        assert_eq!(tools[0].name(), "launch_agent");
    }

    #[test]
    fn test_build_tool_returns_subagent_tool() {
        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let tool = m.build_tool("/tmp");
        assert_eq!(tool.name(), "launch_agent");
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
        ).unwrap();

        let result = scan_agents(dir.path().to_str().unwrap());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "analyst");
        assert_eq!(result[0].1, "data-analyst");
        assert_eq!(result[0].2, "Analyzes data");
    }

    #[tokio::test]
    async fn test_before_agent_injects_summary() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("tester.md"),
            "---\nname: tester\ndescription: Runs tests\n---\n\nYou run tests.\n",
        ).unwrap();

        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        <SubAgentMiddleware as Middleware<AgentState>>::before_agent(&m, &mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        let content = state.messages()[0].content();
        assert!(content.contains("tester"));
        assert!(content.contains("Runs tests"));
        assert!(content.contains("launch_agent"));
    }

    #[tokio::test]
    async fn test_before_agent_no_agents_no_op() {
        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let mut state = AgentState::new("/nonexistent");
        <SubAgentMiddleware as Middleware<AgentState>>::before_agent(&m, &mut state).await.unwrap();
        assert_eq!(state.messages().len(), 0);
    }
}
