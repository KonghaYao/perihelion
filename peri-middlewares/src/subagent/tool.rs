use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use peri_agent::agent::events::{AgentEvent, AgentEventHandler};
use peri_agent::agent::react::{AgentInput, ReactLLM};
use peri_agent::agent::state::AgentState;
use peri_agent::agent::BackgroundTaskResult;
use peri_agent::agent::{AgentCancellationToken, ReActAgent};
use peri_agent::messages::BaseMessage;
use peri_agent::middleware::r#trait::Middleware;
use peri_agent::tools::BaseTool;

use crate::agent_define::{AgentDefineMiddleware, AgentOverrides};
use crate::agents_md::AgentsMdMiddleware;
use crate::claude_agent_parser::{parse_agent_file, ClaudeAgent, ToolsValue};
use crate::hooks::types::{HookEvent, RegisteredHook};
use crate::middleware::todo::TodoMiddleware;
use crate::skills::SkillsMiddleware;
use crate::subagent::background::{BackgroundTask, BackgroundTaskRegistry, BackgroundTaskStatus};
use crate::subagent::built_in_agents::get_built_in_agent;
use crate::subagent::skill_preload::SkillPreloadMiddleware;
use crate::subagent::SubAgentMiddlewareConfig;
use crate::tools::ArcToolWrapper;
use tokio::sync::mpsc;

/// 事件处理器包装器：为子 Agent 事件注入 source_agent_id
///
/// 当子 Agent 共享父 Agent 的 event_handler 时，事件无法区分来源。
/// 此包装器在转发前为 ToolStart/ToolEnd/TextChunk 事件注入 source_agent_id。
struct SourceAgentIdHandler {
    inner: Arc<dyn AgentEventHandler>,
    agent_id: String,
}

impl SourceAgentIdHandler {
    fn new(inner: Arc<dyn AgentEventHandler>, agent_id: String) -> Self {
        Self { inner, agent_id }
    }
}

impl AgentEventHandler for SourceAgentIdHandler {
    fn on_event(&self, event: AgentEvent) {
        let tagged = match event {
            AgentEvent::ToolStart {
                message_id,
                tool_call_id,
                name,
                input,
                ..
            } => AgentEvent::ToolStart {
                message_id,
                tool_call_id,
                name,
                input,
                source_agent_id: Some(self.agent_id.clone()),
            },
            AgentEvent::ToolEnd {
                message_id,
                tool_call_id,
                name,
                output,
                is_error,
                ..
            } => AgentEvent::ToolEnd {
                message_id,
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id: Some(self.agent_id.clone()),
            },
            AgentEvent::TextChunk {
                message_id, chunk, ..
            } => AgentEvent::TextChunk {
                message_id,
                chunk,
                source_agent_id: Some(self.agent_id.clone()),
            },
            other => other,
        };
        self.inner.on_event(tagged);
    }
}

/// 构造 SubAgent 标准中间件链
///
/// 顺序: AgentsMdMiddleware -> SkillsMiddleware -> [SkillPreloadMiddleware] -> TodoMiddleware
///
/// 四条执行路径（fork sync、background normal、background fork、normal sync）
/// 的唯一差异是是否包含 SkillPreloadMiddleware
pub(crate) fn build_subagent_middlewares(
    config: SubAgentMiddlewareConfig,
) -> Vec<Box<dyn Middleware<AgentState>>> {
    let mut middlewares: Vec<Box<dyn Middleware<AgentState>>> = Vec::new();
    // 1. AgentsMdMiddleware: 注入 AGENTS.md / CLAUDE.md 项目指南
    middlewares.push(Box::new(AgentsMdMiddleware::new()));
    // 2. SkillsMiddleware: 渐进式 skills 摘要注入
    middlewares.push(Box::new(SkillsMiddleware::new().with_global_config()));
    // 3. SkillPreloadMiddleware: 全文 skill 预加载（条件性）
    if !config.skill_names.is_empty() {
        middlewares.push(Box::new(SkillPreloadMiddleware::new(
            config.skill_names,
            &config.cwd,
        )));
    }
    // 4. TodoMiddleware: 提供 todo_write 工具
    middlewares.push(Box::new(TodoMiddleware::new({
        let (tx, _rx) = mpsc::channel(8);
        tx
    })));
    middlewares
}

/// 独立（非方法）版本的 SubagentStart/SubagentStop hook 触发逻辑。
///
/// 同时用于两条路径：
/// - **Normal/Fork 路径**：通过 `&self.fire_subagent_lifecycle_hook()` 间接调用
/// - **Background 路径**：`tokio::spawn` 内无法持有 `&self` 引用，直接调用此函数
///
/// 统一入口确保三处触发点的行为一致（HookInput 构建、事件过滤、executor 调用）。
async fn fire_subagent_lifecycle_hooks_static(
    registered_hooks: &[RegisteredHook],
    event: HookEvent,
    cwd: &str,
    subagent_name: &str,
    result: Option<&str>,
) {
    let matching: Vec<&RegisteredHook> = registered_hooks
        .iter()
        .filter(|h| h.event == event)
        .collect();
    if matching.is_empty() {
        return;
    }

    let input = match &event {
        HookEvent::SubagentStart => {
            crate::hooks::types::HookInput::subagent_start("", "", cwd, subagent_name)
        }
        HookEvent::SubagentStop => crate::hooks::types::HookInput::subagent_stop(
            "",
            "",
            cwd,
            subagent_name,
            result.unwrap_or(""),
        ),
        _ => return,
    };

    for registered in &matching {
        let _action = match &registered.hook {
            crate::hooks::types::HookType::Command { .. } => {
                crate::hooks::executor::execute_command_hook(&registered.hook, &input, registered)
                    .await
            }
            crate::hooks::types::HookType::Http { .. } => {
                crate::hooks::executor::execute_http_hook(&registered.hook, &input).await
            }
            _ => crate::hooks::types::HookAction::Allow,
        };
    }
}

/// SubAgentTool - implements the `Agent` tool, allowing LLM to delegate sub-tasks to specialized sub-agents
///
/// LLM calls this tool with `subagent_type` and `prompt` to trigger execution of the corresponding agent definition file.
/// The sub-agent inherits the parent's tool set (filtered by tools/disallowedTools fields),
/// does not include HITL middleware, and returns execution results as a string to the parent agent.
const AGENT_DESCRIPTION: &str = r#"Launch a sub-agent with an independent context to handle a specialized sub-task. The sub-agent executes based on the configuration defined in .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md.

Fork mode (fork: true):
- Inherits the parent agent's full conversation history, system prompt, and tool set
- The prompt is treated as a directive within the existing context, not a standalone briefing
- Do NOT re-explain background that is already in the conversation history
- Use for tasks that require context from the ongoing conversation (e.g., continuing a multi-file refactor)
- The forked agent follows a structured output format: Scope, Result, Key files, Files changed

Usage:
- Provide a clear, self-contained task description via the prompt parameter. The sub-agent has no access to the parent conversation history
- Specify subagent_type matching an existing agent definition file. When not provided, creates a fork of the current agent
- The sub-agent inherits the parent's tool set by default, excluding Agent itself (to prevent recursion)
- Agent definitions may restrict available tools via the tools and disallowedTools fields in frontmatter
- The sub-agent executes in isolated state — it cannot access the parent's message history or intermediate results

When to use:
- For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
- For tasks requiring specialized persona or behavior defined in agent configuration files
- For parallelizable sub-tasks that do not depend on each other's results
- When you need to break a complex task into smaller, independently executable pieces

Return format:
- If the sub-agent made tool calls, the result includes a summary of tools used followed by the final response
- If no tool calls were made, only the final response text is returned

Background execution (run_in_background: true):
- The sub-agent runs asynchronously in the background while the main agent continues
- Maximum 3 concurrent background tasks
- The main agent will be notified when the background task completes via a system message
- Use for long-running tasks that don't block the main workflow (e.g., code review, batch operations)
- Background tasks share the same working directory as the main agent"#;
pub struct SubAgentTool {
    /// Parent agent tool set (Arc shared, read-only)
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// Parent agent event handler (transparent forwarding of sub-agent events)
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// Parent agent working directory (inherited when LLM does not specify cwd)
    parent_cwd: String,
    /// LLM factory function, creates independent LLM instance for each sub-agent (no system, injected via with_system_prompt())
    /// Parameter is optional model alias (e.g., "haiku"/"sonnet"/"opus"), None means inherit parent model
    #[allow(clippy::type_complexity)]
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// System prompt builder: (agent overrides, cwd) -> system prompt string
    ///
    /// The returned content is injected into the sub-agent's state messages via `with_system_prompt()`,
    /// making it visible in Langfuse and other tracing tools. When None, no system prompt is injected.
    #[allow(clippy::type_complexity)]
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// Optional cancellation token for interrupting sub-agent execution
    cancel: Option<AgentCancellationToken>,
    /// Shared reference to parent agent message snapshot (used by Fork path)
    /// RwLock.read() obtains a deep copy, RwLock.write() is updated by SubAgentMiddleware::before_agent
    parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
    /// 后台任务注册中心（run_in_background 模式使用）
    background_registry: Option<Arc<BackgroundTaskRegistry>>,
    /// 子 agent 生命周期 hook（SubagentStart/SubagentStop）。
    /// 从父 agent 的 HookMiddleware 中提��，由 `with_registered_hooks` 注入。
    /// Background 路径通过 Arc clone 传入 spawn 闭包。
    registered_hooks: Arc<Vec<RegisteredHook>>,
    /// Per-child event handler factory: takes agent_id → returns handler for that child.
    /// When set, used instead of wrapping parent's event_handler for child agent execution,
    /// avoiding shared Lock contention (e.g., Langfuse Mutex) in concurrent SubAgent scenarios.
    #[allow(clippy::type_complexity)]
    child_handler_factory: Option<Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>>,
}

impl SubAgentTool {
    #[allow(clippy::type_complexity)]
    pub fn new(
        parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        parent_cwd: String,
    ) -> Self {
        Self {
            parent_tools,
            event_handler,
            llm_factory,
            parent_cwd,
            system_builder: None,
            cancel: None,
            parent_messages: None,
            background_registry: None,
            registered_hooks: Arc::new(Vec::new()),
            child_handler_factory: None,
        }
    }

    /// Set system prompt builder for injecting full system prompt including tone/proactiveness to sub-agent
    #[allow(clippy::type_complexity)]
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    /// Set cancellation token for supporting user interruption of sub-agent execution
    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Set shared parent message reference, Fork path obtains deep copy via RwLock.read()
    pub fn with_parent_messages(mut self, messages: Arc<RwLock<Vec<BaseMessage>>>) -> Self {
        self.parent_messages = Some(messages);
        self
    }

    /// Set background task registry for run_in_background mode
    pub fn with_background_registry(mut self, registry: Arc<BackgroundTaskRegistry>) -> Self {
        self.background_registry = Some(registry);
        self
    }

    /// Set registered hooks for SubagentStart/SubagentStop lifecycle events.
    /// Hooks are extracted from the parent HookMiddleware and injected here.
    pub fn with_registered_hooks(mut self, hooks: Vec<RegisteredHook>) -> Self {
        self.registered_hooks = Arc::new(hooks);
        self
    }

    /// Set per-child event handler factory.
    /// When set, `invoke`/`invoke_fork` uses `factory(agent_id)` instead of wrapping
    /// the parent's event_handler, avoiding shared Lock contention in concurrent execution.
    #[allow(clippy::type_complexity)]
    pub fn with_child_handler_factory(
        mut self,
        factory: Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>,
    ) -> Self {
        self.child_handler_factory = Some(factory);
        self
    }

    /// Load and parse an agent definition, falling back to built-in agents.
    ///
    /// Search order:
    /// 1. `{cwd}/.claude/agents/{agent_id}/agent.md` or `{cwd}/.claude/agents/{agent_id}.md`
    /// 2. Built-in agent registry (compile-time embedded)
    fn load_agent_def(&self, agent_id: &str, cwd: &str) -> Result<ClaudeAgent, String> {
        // Try filesystem first (project-level takes precedence)
        let agent_path = AgentDefineMiddleware::candidate_paths(cwd, agent_id)
            .into_iter()
            .find(|p| p.is_file());

        if let Some(path) = agent_path {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Error: failed to read agent definition file: {}", e))?;
            return parse_agent_file(&content).ok_or_else(|| {
                format!(
                    "Error: failed to parse agent definition file '{}'",
                    path.display()
                )
            });
        }

        // Fallback to built-in agents
        let built_in = get_built_in_agent(agent_id)
            .ok_or_else(|| format!("Error: cannot find agent definition '{}'. Check .claude/agents/ directory or use a built-in agent (explore, plan, general-purpose, verification)", agent_id))?;
        parse_agent_file(built_in.content).ok_or_else(|| {
            format!(
                "Error: failed to parse built-in agent definition '{}'",
                agent_id
            )
        })
    }

    /// Extract AgentOverrides from already-parsed agent_def to avoid redundant I/O.
    ///
    /// Delegates to [`super::fork::overrides_from_agent_def`].
    fn overrides_from_agent_def(
        system_prompt: &str,
        tone: &Option<String>,
        proactiveness: &Option<String>,
    ) -> Option<AgentOverrides> {
        super::fork::overrides_from_agent_def(system_prompt, tone, proactiveness)
    }

    /// Fire SubagentStart/SubagentStop hooks if any matching hooks are registered.
    async fn fire_subagent_lifecycle_hook(
        &self,
        event: HookEvent,
        cwd: &str,
        subagent_name: &str,
        result: Option<&str>,
    ) {
        fire_subagent_lifecycle_hooks_static(
            &self.registered_hooks,
            event,
            cwd,
            subagent_name,
            result,
        )
        .await;
    }

    /// Filter available tools from parent tool set based on agent definition's tools/disallowedTools fields.
    ///
    /// Delegates to [`super::fork::filter_tools`].
    fn filter_tools(
        &self,
        allowed: &ToolsValue,
        disallowed: &ToolsValue,
    ) -> Vec<Box<dyn BaseTool>> {
        super::fork::filter_tools(&self.parent_tools, allowed, disallowed)
    }

    /// Fork path: sub-agent inherits parent's full message history + system prompt + tool set
    async fn invoke_fork(
        &self,
        prompt: &str,
        cwd: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Obtain deep copy of parent messages
        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Ok(
                "Error: Fork path requires parent message history, but parent_messages is not set"
                    .to_string(),
            ),
        };

        // 2. Build fork directive Human message
        let fork_directive = super::fork::build_fork_directive(prompt);

        // 3. Build child AgentState using deep copy of parent messages
        let mut fork_state = AgentState::with_messages(cwd.to_string(), parent_msgs);

        // 4. Assemble child ReActAgent (same middleware chain as Normal path)
        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);

        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_fork(cwd)) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        // 5. Inject system prompt (obtained via system_builder, consistent with Normal path)
        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // 6. Register full parent tools (no filtering, including Agent itself to maintain cache hit)
        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder
                .register_tool(Box::new(ArcToolWrapper(Arc::clone(tool))) as Box<dyn BaseTool>);
        }

        // 7. Forward event handler: use dedicated child handler if factory available,
        //    otherwise wrap parent handler with source_agent_id tagging
        if let Some(ref factory) = self.child_handler_factory {
            agent_builder = agent_builder.with_event_handler(factory("fork".to_string()));
        } else if let Some(handler) = &self.event_handler {
            let tagged = Arc::new(SourceAgentIdHandler::new(
                Arc::clone(handler),
                "fork".to_string(),
            ));
            agent_builder = agent_builder.with_event_handler(tagged);
        }

        // 8. Execute (input = fork directive, appended as Human message by execute())
        // Emit SubagentStarted event + fire SubagentStart hooks
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: "fork".to_string(),
            });
        }
        self.fire_subagent_lifecycle_hook(HookEvent::SubagentStart, cwd, "fork", None)
            .await;

        let fork_result = agent_builder
            .execute(
                AgentInput::text(fork_directive),
                &mut fork_state,
                self.cancel.clone(),
            )
            .await;

        // Emit SubagentStopped event + fire SubagentStop hooks
        let (output_summary, stopped_is_error) = match &fork_result {
            Ok(output) => (output.text.chars().take(500).collect::<String>(), false),
            Err(e) => (
                format!("Error: {}", e)
                    .chars()
                    .take(500)
                    .collect::<String>(),
                true,
            ),
        };
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStopped {
                agent_name: "fork".to_string(),
                result: output_summary.clone(),
                is_error: stopped_is_error,
            });
        }
        self.fire_subagent_lifecycle_hook(
            HookEvent::SubagentStop,
            cwd,
            "fork",
            Some(&output_summary),
        )
        .await;

        match fork_result {
            Ok(output) => Ok(format_subagent_result(&output)),
            Err(peri_agent::error::AgentError::Interrupted) => {
                Ok("Fork sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                let msg = format!("Fork sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }

    /// Background path: spawn sub-agent as a background task, return immediately
    ///
    /// When `is_fork` is true, the background agent inherits parent messages and
    /// system prompt (fork semantics) instead of loading an agent definition file.
    async fn invoke_background(
        &self,
        prompt: String,
        subagent_type: Option<String>,
        cwd: String,
        is_fork: bool,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let registry = self
            .background_registry
            .as_ref()
            .ok_or("Background tasks not available: no registry configured")?;

        // 检查并发上限
        if registry.active_count() >= 3 {
            return Ok("Error: maximum 3 concurrent background tasks reached. \
                 Wait for a running task to complete before starting a new one."
                .to_string());
        }

        let task_id = format!("bg-{}", uuid::Uuid::new_v4());

        // Fork mode: no agent definition needed, use parent context
        if is_fork {
            return self
                .invoke_background_fork(prompt, cwd, task_id, registry)
                .await;
        }

        let agent_id =
            match &subagent_type {
                Some(id) => id.clone(),
                None => return Ok(
                    "Error: background mode requires subagent_type parameter (or use fork: true)"
                        .to_string(),
                ),
            };

        let agent_def = match self.load_agent_def(&agent_id, &cwd) {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };

        let filtered_tools = self.filter_tools(
            &agent_def.frontmatter.tools,
            &agent_def.frontmatter.disallowed_tools,
        );

        tracing::debug!(
            agent_id = %agent_id,
            parent_count = self.parent_tools.len(),
            filtered_count = filtered_tools.len(),
            filtered_names = ?filtered_tools.iter().map(|t| t.name()).collect::<Vec<_>>(),
            allowed = ?agent_def.frontmatter.tools,
            disallowed = ?agent_def.frontmatter.disallowed_tools,
            "background agent: tool filter results"
        );

        let agent_name = agent_id.clone();
        let prompt_summary: String = prompt.chars().take(100).collect();

        // Build child agent before spawn (avoid capturing self references across await)
        let model_alias: Option<&str> = agent_def
            .frontmatter
            .model
            .as_deref()
            .filter(|m| !m.is_empty() && *m != "inherit");
        let llm = (self.llm_factory)(model_alias);
        let raw_turns = agent_def.frontmatter.max_turns.unwrap_or(200);
        let max_iterations = if raw_turns == 0 {
            200
        } else {
            raw_turns as usize
        };

        let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);
        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_agent_def(
            agent_def.frontmatter.skills.clone(),
            &cwd,
        )) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        if let Some(ref builder) = self.system_builder {
            let overrides = Self::overrides_from_agent_def(
                &agent_def.system_prompt,
                &agent_def.frontmatter.tone,
                &agent_def.frontmatter.proactiveness,
            );
            let system_content = builder(overrides.as_ref(), &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        for tool in filtered_tools {
            agent_builder = agent_builder.register_tool(tool);
        }

        // Background agent 不共享父的 event_handler，避免子 agent 的事件
        // （TextChunk、ToolStart、Done 等）混入父 agent 的消息流。
        // 完成通知通过 spawn 后的 BackgroundTaskCompleted 事件单独发送。

        // Pass cancel token to child agent
        let cancel_token = self.cancel.clone();

        // Clone values needed inside the spawn closure
        let spawn_task_id = task_id.clone();
        let spawn_agent_name = agent_name.clone();
        let spawn_prompt_summary = prompt_summary.clone();

        // Spawn background task
        let event_handler = self.event_handler.clone();
        let spawn_registry = Arc::clone(registry);
        let spawn_hooks = Arc::clone(&self.registered_hooks);

        // Fire SubagentStart hook before spawning
        self.fire_subagent_lifecycle_hook(HookEvent::SubagentStart, &cwd, &agent_name, None)
            .await;

        let handle = tokio::spawn(async move {
            let mut state = AgentState::new(&cwd);
            let start = std::time::Instant::now();

            let result = match agent_builder
                .execute(AgentInput::text(&prompt), &mut state, cancel_token)
                .await
            {
                Ok(output) => {
                    let tool_calls_count = state
                        .messages
                        .iter()
                        .filter(|m| matches!(m, BaseMessage::Tool { .. }))
                        .count();
                    BackgroundTaskResult {
                        task_id: spawn_task_id.clone(),
                        agent_name: spawn_agent_name.clone(),
                        prompt_summary: spawn_prompt_summary.clone(),
                        success: true,
                        output: output.text,
                        tool_calls_count,
                        duration_ms: start.elapsed().as_millis() as u64,
                    }
                }
                Err(e) => BackgroundTaskResult {
                    task_id: spawn_task_id.clone(),
                    agent_name: spawn_agent_name.clone(),
                    prompt_summary: spawn_prompt_summary.clone(),
                    success: false,
                    output: e.to_string(),
                    tool_calls_count: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            };

            // Push notification to channel + update registry status
            spawn_registry.complete(&spawn_task_id, result.clone());

            // Fire SubagentStop hook
            fire_subagent_lifecycle_hooks_static(
                &spawn_hooks,
                HookEvent::SubagentStop,
                &cwd,
                &spawn_agent_name,
                Some(&result.output),
            )
            .await;

            // Emit event for TUI
            if let Some(ref handler) = event_handler {
                handler.on_event(AgentEvent::BackgroundTaskCompleted(result));
            }
        });

        // Register task (values still available since we cloned for spawn)
        registry.register(BackgroundTask {
            id: task_id.clone(),
            agent_name: agent_name.clone(),
            prompt_summary: prompt_summary.clone(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: handle,
        })?;

        Ok(format!(
            "Background task {} started. You will be notified when it completes. \
             You can continue with other tasks in the meantime.",
            task_id
        ))
    }

    /// Background fork: spawn a fork-style sub-agent as a background task.
    ///
    /// Combines fork semantics (inherit parent messages + system prompt + tools)
    /// with background execution (return immediately, notify on completion).
    async fn invoke_background_fork(
        &self,
        prompt: String,
        cwd: String,
        task_id: String,
        registry: &Arc<BackgroundTaskRegistry>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let agent_name = "fork".to_string();
        let prompt_summary: String = prompt.chars().take(100).collect();

        // Build fork directive
        let fork_directive = super::fork::build_fork_directive(&prompt);

        // Obtain parent messages
        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Ok(
                "Error: Fork path requires parent message history, but parent_messages is not set"
                    .to_string(),
            ),
        };

        // Assemble child ReActAgent with fork semantics
        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);
        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_fork(&cwd)) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // Register all parent tools (fork inherits everything)
        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder.register_tool(Box::new(ArcToolWrapper(Arc::clone(tool))));
        }

        let cancel_token = self.cancel.clone();
        let event_handler = self.event_handler.clone();
        let spawn_registry = Arc::clone(registry);
        let spawn_hooks = Arc::clone(&self.registered_hooks);
        let spawn_task_id = task_id.clone();
        let spawn_agent_name = agent_name.clone();
        let spawn_prompt_summary = prompt_summary.clone();

        self.fire_subagent_lifecycle_hook(HookEvent::SubagentStart, &cwd, &agent_name, None)
            .await;

        let handle = tokio::spawn(async move {
            let mut fork_state = AgentState::with_messages(cwd.clone(), parent_msgs);
            let start = std::time::Instant::now();

            let result = match agent_builder
                .execute(
                    AgentInput::text(&fork_directive),
                    &mut fork_state,
                    cancel_token,
                )
                .await
            {
                Ok(output) => {
                    let tool_calls_count = fork_state
                        .messages
                        .iter()
                        .filter(|m| matches!(m, BaseMessage::Tool { .. }))
                        .count();
                    BackgroundTaskResult {
                        task_id: spawn_task_id.clone(),
                        agent_name: spawn_agent_name.clone(),
                        prompt_summary: spawn_prompt_summary.clone(),
                        success: true,
                        output: output.text,
                        tool_calls_count,
                        duration_ms: start.elapsed().as_millis() as u64,
                    }
                }
                Err(e) => BackgroundTaskResult {
                    task_id: spawn_task_id.clone(),
                    agent_name: spawn_agent_name.clone(),
                    prompt_summary: spawn_prompt_summary.clone(),
                    success: false,
                    output: e.to_string(),
                    tool_calls_count: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            };

            spawn_registry.complete(&spawn_task_id, result.clone());

            fire_subagent_lifecycle_hooks_static(
                &spawn_hooks,
                HookEvent::SubagentStop,
                &cwd,
                &spawn_agent_name,
                Some(&result.output),
            )
            .await;

            if let Some(ref handler) = event_handler {
                handler.on_event(AgentEvent::BackgroundTaskCompleted(result));
            }
        });

        registry.register(BackgroundTask {
            id: task_id.clone(),
            agent_name: agent_name.clone(),
            prompt_summary: prompt_summary.clone(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: handle,
        })?;

        Ok(format!(
            "Background task {} started. You will be notified when it completes. \
             You can continue with other tasks in the meantime.",
            task_id
        ))
    }
}

#[async_trait]
impl BaseTool for SubAgentTool {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        AGENT_DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task description to delegate to the sub-agent. Must be clear and self-contained, as the sub-agent has no access to the parent conversation history. Include all necessary context"
                },
                "description": {
                    "type": "string",
                    "description": "A short description of the task (3-5 words), used for UI display and logging"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The agent ID from the available agents list (e.g., 'code-reviewer', 'explorer'). Must exactly match an agent definition file at .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md. When empty or not provided, creates a fork of the current agent with all tools"
                },
                "name": {
                    "type": "string",
                    "description": "A short alias for the sub-agent, used for UI identification"
                },
                "isolation": {
                    "type": "string",
                    "description": "Isolation mode for the sub-agent. Use 'worktree' to create an isolated git worktree. Currently reserved for future use"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run the sub-agent in the background. The main agent continues immediately and receives a notification when the background task completes. Maximum 3 concurrent background tasks"
                },
                "cwd": {
                    "type": "string",
                    "description": "The working directory for the sub-agent. Defaults to inheriting the parent agent's current working directory if not specified"
                },
                "fork": {
                    "type": "boolean",
                    "description": "Set to true to fork the current agent with full conversation context. The forked agent inherits all messages, tools, and system prompt from the parent. Use when the task requires context from the ongoing conversation"
                }
            }
        })
    }

    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let prompt = match input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return Ok("Error: missing required parameter prompt".to_string()),
        };
        let subagent_type = input
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let _description = input.get("description").and_then(|v| v.as_str());
        let _name = input.get("name").and_then(|v| v.as_str());
        let _isolation = input.get("isolation").and_then(|v| v.as_str());
        let run_in_background = input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // cwd defaults to inheriting parent agent's working directory
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.parent_cwd)
            .to_string();

        // Fork detection: explicit fork:true OR subagent_type="fork" (common LLM mistake)
        let is_fork = input.get("fork").and_then(|v| v.as_bool()).unwrap_or(false)
            || subagent_type.as_deref() == Some("fork");

        // Background path takes priority when run_in_background is set and registry exists.
        // This handles both regular background agents AND fork+background combinations.
        // Previously fork+background fell through to invoke_fork (synchronous), which
        // produced a phantom background_task_count (from SubAgentStart with is_background=true)
        // but no BackgroundTaskCompleted event, breaking the continuation flow.
        if run_in_background && self.background_registry.is_some() {
            return self
                .invoke_background(prompt, subagent_type, cwd, is_fork)
                .await;
        }

        // Fork without background: synchronous path
        if is_fork {
            return self.invoke_fork(&prompt, &cwd).await;
        }
        // No registry configured: fall through to normal execution

        // 1. Load agent definition (filesystem with built-in fallback)
        let agent_id = match &subagent_type {
            Some(id) => id.clone(),
            None => {
                return Ok(
                    "Error: please provide subagent_type parameter to specify the agent type, or use fork: true for fork mode"
                        .to_string(),
                )
            }
        };

        let agent_def = match self.load_agent_def(&agent_id, &cwd) {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };

        // 2. Tool filtering
        let filtered_tools = self.filter_tools(
            &agent_def.frontmatter.tools,
            &agent_def.frontmatter.disallowed_tools,
        );

        // 4. Assemble child ReActAgent
        // Extract model alias: non-"inherit" and non-empty passed to factory, None means inherit parent model
        let model_alias: Option<&str> = agent_def
            .frontmatter
            .model
            .as_deref()
            .filter(|m| !m.is_empty() && *m != "inherit");
        let llm = (self.llm_factory)(model_alias);
        let raw_turns = agent_def.frontmatter.max_turns.unwrap_or(200);
        let max_iterations = if raw_turns == 0 {
            200
        } else {
            raw_turns as usize
        };

        let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);

        // 5. Add standard sub-agent middleware chain
        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_agent_def(
            agent_def.frontmatter.skills.clone(),
            &cwd,
        )) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        // 6. Inject system prompt via with_system_prompt (visible in Langfuse tracing)
        //    System prompt = build_system_prompt(agent overrides, cwd), includes tone/proactiveness
        //    Reuse already-parsed agent_def to extract overrides, avoiding redundant I/O
        if let Some(ref builder) = self.system_builder {
            let overrides = Self::overrides_from_agent_def(
                &agent_def.system_prompt,
                &agent_def.frontmatter.tone,
                &agent_def.frontmatter.proactiveness,
            );
            let system_content = builder(overrides.as_ref(), &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // Register filtered tools
        for tool in filtered_tools {
            agent_builder = agent_builder.register_tool(tool);
        }

        // Forward event handler: use dedicated child handler if factory available,
        // otherwise wrap parent handler with source_agent_id tagging
        if let Some(ref factory) = self.child_handler_factory {
            agent_builder = agent_builder.with_event_handler(factory(agent_id.clone()));
        } else if let Some(handler) = &self.event_handler {
            let tagged = Arc::new(SourceAgentIdHandler::new(
                Arc::clone(handler),
                agent_id.clone(),
            ));
            agent_builder = agent_builder.with_event_handler(tagged);
        }

        // 7. Execute child agent
        let mut state = AgentState::new(cwd.clone());

        // Emit SubagentStarted event + fire SubagentStart hooks
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: agent_id.clone(),
            });
        }
        self.fire_subagent_lifecycle_hook(HookEvent::SubagentStart, &cwd, &agent_id, None)
            .await;

        tracing::info!(
            "[DEADLOCK] SubAgentTool: START child execute, agent_id={}, prompt_len={}",
            agent_id,
            prompt.len()
        );
        let exec_start = std::time::Instant::now();
        let exec_result = agent_builder
            .execute(AgentInput::text(prompt), &mut state, self.cancel.clone())
            .await;
        tracing::info!(
            "[DEADLOCK] SubAgentTool: END child execute ({:.1?}), agent_id={}, is_ok={}",
            exec_start.elapsed(),
            agent_id,
            exec_result.is_ok()
        );

        // Emit SubagentStopped event + fire SubagentStop hooks
        let (output_summary, stopped_is_error) = match &exec_result {
            Ok(output) => (output.text.chars().take(500).collect::<String>(), false),
            Err(e) => (
                format!("Error: {}", e)
                    .chars()
                    .take(500)
                    .collect::<String>(),
                true,
            ),
        };
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStopped {
                agent_name: agent_id.clone(),
                result: output_summary.clone(),
                is_error: stopped_is_error,
            });
        }
        self.fire_subagent_lifecycle_hook(
            HookEvent::SubagentStop,
            &cwd,
            &agent_id,
            Some(&output_summary),
        )
        .await;

        match exec_result {
            Ok(output) => Ok(format_subagent_result(&output)),
            Err(peri_agent::error::AgentError::Interrupted) => {
                Ok("Sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                let msg = format!("Sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }
}

/// Format sub-agent execution result as a summary string returned to the parent agent.
///
/// Summary format:
/// - If tool calls exist, aggregate tool calls by name with count (e.g., "Glob 5 times, Read 20 times")
/// - Preserve final answer text
///
/// **注意**：输出格式被 TUI (`message_view.rs`) 解析以提取工具调用次数。
/// 修改此格式时需同步更新 `parse_subagent_tool_count()`。
fn format_subagent_result(output: &peri_agent::agent::react::AgentOutput) -> String {
    if output.tool_calls.is_empty() {
        return output.text.clone();
    }

    // 统计各工具调用次数
    let mut tool_counts: HashMap<&str, usize> = HashMap::new();
    for (call, _) in &output.tool_calls {
        *tool_counts.entry(call.name.as_str()).or_insert(0) += 1;
    }

    // 按调用次数降序排序
    let mut tools: Vec<_> = tool_counts.into_iter().collect();
    tools.sort_by_key(|b| std::cmp::Reverse(b.1));

    // 格式化为 "Glob 5 times, Grep 12 times, Read 74 times"
    let tool_summary = tools
        .into_iter()
        .map(|(name, count)| format!("{} {} times", name, count))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "[Sub-agent executed {} tool calls: {}]\n\n{}",
        output.tool_calls.len(),
        tool_summary,
        output.text
    )
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "tool_test.rs"]
mod tests;
