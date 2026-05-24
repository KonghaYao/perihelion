use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::agent::events::{AgentEvent, AgentEventHandler};
use peri_agent::agent::react::{AgentInput, ReactLLM};
use peri_agent::agent::state::AgentState;
use peri_agent::agent::BackgroundTaskResult;
use peri_agent::agent::State as _;
use peri_agent::agent::{AgentCancellationToken, ReActAgent};
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadMeta, ThreadStore};
use peri_agent::tools::BaseTool;

use crate::agent_define::{AgentDefineMiddleware, AgentOverrides};
use crate::claude_agent_parser::{parse_agent_file, ClaudeAgent, ToolsValue};
use crate::hooks::types::{HookEvent, RegisteredHook};
use crate::subagent::background::{BackgroundTask, BackgroundTaskRegistry, BackgroundTaskStatus};
use crate::subagent::built_in_agents::get_built_in_agent;
use crate::subagent::SubAgentMiddlewareConfig;
use crate::tools::ArcToolWrapper;
use parking_lot::RwLock;

use super::build_subagent_middlewares;
use super::fire_subagent_lifecycle_hooks_static;
use super::format_subagent_result;
use super::SourceAgentIdHandler;

/// RAII guard that calls deregister on drop (panic-safe cleanup).
struct DeregisterGuard {
    thread_id: String,
    deregister: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl Drop for DeregisterGuard {
    fn drop(&mut self) {
        if let Some(ref deregister) = self.deregister {
            deregister(&self.thread_id);
        }
    }
}

/// SubAgentTool - implements the `Agent` tool, allowing LLM to delegate sub-tasks to specialized sub-agents
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
    #[allow(clippy::type_complexity)]
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// System prompt builder: (agent overrides, cwd) -> system prompt string
    #[allow(clippy::type_complexity)]
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// Optional cancellation token for interrupting sub-agent execution
    cancel: Option<AgentCancellationToken>,
    /// Shared reference to parent agent message snapshot (used by Fork path)
    parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
    /// 后台任务注册中心（run_in_background 模式使用）
    background_registry: Option<Arc<BackgroundTaskRegistry>>,
    /// 子 agent 生命周期 hook（SubagentStart/SubagentStop）
    registered_hooks: Arc<Vec<RegisteredHook>>,
    /// Per-child event handler factory
    #[allow(clippy::type_complexity)]
    child_handler_factory: Option<Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>>,
    /// 后台任务完成事件的独立发送通道（不随 executor 生命周期销毁）
    bg_event_sender:
        Option<tokio::sync::mpsc::UnboundedSender<peri_agent::agent::events::AgentEvent>>,
    /// Thread persistence store for child threads
    thread_store: Option<Arc<dyn ThreadStore>>,
    /// Parent thread ID for child thread hierarchy
    parent_thread_id: Option<String>,
    /// Register callback: (thread_id, cancel_token, cancel_policy_str) → inserts into active_agents map
    #[allow(clippy::type_complexity)]
    register_runtime: Option<Arc<dyn Fn(String, AgentCancellationToken, String) + Send + Sync>>,
    /// Deregister callback: removes from active_agents map by thread_id
    deregister_runtime: Option<Arc<dyn Fn(&str) + Send + Sync>>,
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
            bg_event_sender: None,
            thread_store: None,
            parent_thread_id: None,
            register_runtime: None,
            deregister_runtime: None,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn with_parent_messages(mut self, messages: Arc<RwLock<Vec<BaseMessage>>>) -> Self {
        self.parent_messages = Some(messages);
        self
    }

    pub fn with_background_registry(mut self, registry: Arc<BackgroundTaskRegistry>) -> Self {
        self.background_registry = Some(registry);
        self
    }

    pub fn with_registered_hooks(mut self, hooks: Vec<RegisteredHook>) -> Self {
        self.registered_hooks = Arc::new(hooks);
        self
    }

    #[allow(clippy::type_complexity)]
    pub fn with_child_handler_factory(
        mut self,
        factory: Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>,
    ) -> Self {
        self.child_handler_factory = Some(factory);
        self
    }

    pub fn with_bg_event_sender(
        mut self,
        sender: tokio::sync::mpsc::UnboundedSender<peri_agent::agent::events::AgentEvent>,
    ) -> Self {
        self.bg_event_sender = Some(sender);
        self
    }

    pub fn with_thread_store(mut self, store: Arc<dyn ThreadStore>) -> Self {
        self.thread_store = Some(store);
        self
    }

    pub fn with_parent_thread_id(mut self, id: String) -> Self {
        self.parent_thread_id = Some(id);
        self
    }

    #[allow(clippy::type_complexity)]
    pub fn with_register_runtime(
        mut self,
        cb: Arc<dyn Fn(String, AgentCancellationToken, String) + Send + Sync>,
    ) -> Self {
        self.register_runtime = Some(cb);
        self
    }

    pub fn with_deregister_runtime(mut self, cb: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.deregister_runtime = Some(cb);
        self
    }

    pub(crate) fn load_agent_def(&self, agent_id: &str, cwd: &str) -> Result<ClaudeAgent, String> {
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

        let built_in = get_built_in_agent(agent_id)
            .ok_or_else(|| format!("Error: cannot find agent definition '{}'. Check .claude/agents/ directory or use a built-in agent (explore, plan, general-purpose, verification)", agent_id))?;
        parse_agent_file(built_in.content).ok_or_else(|| {
            format!(
                "Error: failed to parse built-in agent definition '{}'",
                agent_id
            )
        })
    }

    pub(crate) fn overrides_from_agent_def(
        system_prompt: &str,
        tone: &Option<String>,
        proactiveness: &Option<String>,
    ) -> Option<AgentOverrides> {
        crate::subagent::fork::overrides_from_agent_def(system_prompt, tone, proactiveness)
    }

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

    pub(crate) fn filter_tools(
        &self,
        allowed: &ToolsValue,
        disallowed: &ToolsValue,
    ) -> Vec<Box<dyn BaseTool>> {
        crate::subagent::fork::filter_tools(&self.parent_tools, allowed, disallowed)
    }

    async fn invoke_fork(
        &self,
        prompt: &str,
        cwd: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Ok(
                "Error: Fork path requires parent message history, but parent_messages is not set"
                    .to_string(),
            ),
        };

        // Create child thread for fork mode
        let child_thread_id = uuid::Uuid::now_v7().to_string();
        if let Some(ref store) = self.thread_store {
            let snapshot_id = parent_msgs.last().map(|m| m.id().as_uuid().to_string());
            let mut child_meta = ThreadMeta::new(cwd);
            child_meta.id = child_thread_id.clone();
            child_meta.parent_thread_id = self.parent_thread_id.clone();
            child_meta.snapshot_at_message_id = snapshot_id;
            child_meta.hidden = true;
            child_meta.cancel_policy = "cascade".to_string();
            child_meta.title = Some("fork".to_string());
            store
                .create_thread(child_meta)
                .await
                .map_err(|e| format!("Failed to create child thread: {}", e))?;
        }

        let fork_directive = crate::subagent::fork::build_fork_directive(prompt);
        let mut fork_state = if let Some(ref store) = self.thread_store {
            AgentState::new(cwd).with_persistence(Arc::clone(store), child_thread_id.clone())
        } else {
            AgentState::new(cwd)
        };
        // For immediate execution, inject parent messages into state
        for msg in parent_msgs {
            fork_state.add_message(msg);
        }
        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);
        // instance_id 统一使用 child_thread_id（UUID v7，持久化线程标识）
        let instance_id = child_thread_id.clone();

        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_fork(cwd)) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder
                .register_tool(Box::new(ArcToolWrapper(Arc::clone(tool))) as Box<dyn BaseTool>);
        }

        if let Some(ref factory) = self.child_handler_factory {
            agent_builder = agent_builder.with_event_handler(factory(instance_id.clone()));
        } else if let Some(handler) = &self.event_handler {
            let tagged = Arc::new(SourceAgentIdHandler::new(
                Arc::clone(handler),
                instance_id.clone(),
            ));
            agent_builder = agent_builder.with_event_handler(tagged);
        }

        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: "fork".to_string(),
                instance_id: instance_id.clone(),
                is_background: false,
            });
        }
        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStart,
            cwd,
            "fork",
            None,
        )
        .await;

        // Register AgentRuntime: only when thread_store is present (non-legacy path)
        // Panic-safe: DeregisterGuard ensures deregister runs on drop (panic or early return)
        let _deregister_guard = if self.thread_store.is_some() {
            if let Some(ref register) = self.register_runtime {
                let child_cancel = AgentCancellationToken::new();
                register(child_thread_id.clone(), child_cancel, "cascade".to_string());
            }
            DeregisterGuard {
                thread_id: child_thread_id.clone(),
                deregister: self.deregister_runtime.clone(),
            }
        } else {
            DeregisterGuard {
                thread_id: child_thread_id.clone(),
                deregister: None,
            }
        };

        let fork_result = agent_builder
            .execute(
                AgentInput::text(fork_directive),
                &mut fork_state,
                self.cancel.clone(),
            )
            .await;

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
                instance_id: instance_id.clone(),
            });
        }
        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStop,
            cwd,
            "fork",
            Some(&output_summary),
        )
        .await;

        match fork_result {
            Ok(output) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store.update_thread_status(&child_thread_id, "done").await;
                }
                let result_text = format_subagent_result(&output);
                if self.thread_store.is_some() {
                    Ok(format!(
                        "child_thread_id: {}\n{}",
                        child_thread_id, result_text
                    ))
                } else {
                    Ok(result_text)
                }
            }
            Err(peri_agent::error::AgentError::Interrupted) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store
                        .update_thread_status(&child_thread_id, "cancelled")
                        .await;
                }
                Ok("Fork sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store.update_thread_status(&child_thread_id, "error").await;
                }
                let msg = format!("Fork sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }

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

        if registry.active_count() >= 3 {
            return Ok("Error: maximum 3 concurrent background tasks reached. \
                 Wait for a running task to complete before starting a new one."
                .to_string());
        }

        let task_id = format!("bg-{}", uuid::Uuid::new_v4());

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

        let cancel_token = self.cancel.clone();
        let spawn_task_id = task_id.clone();
        let spawn_agent_name = agent_name.clone();
        let spawn_prompt_summary = prompt_summary.clone();
        let spawn_registry = Arc::clone(registry);
        let spawn_hooks = Arc::clone(&self.registered_hooks);
        let spawn_bg_sender = self.bg_event_sender.clone();

        // Create child thread for background mode
        let bg_child_thread_id = uuid::Uuid::now_v7().to_string();
        if let Some(ref store) = self.thread_store {
            let mut child_meta = ThreadMeta::new(&cwd);
            child_meta.id = bg_child_thread_id.clone();
            child_meta.parent_thread_id = self.parent_thread_id.clone();
            child_meta.hidden = true;
            child_meta.cancel_policy = "independent".to_string();
            child_meta.title = Some(format!("bg-{}", task_id));
            store
                .create_thread(child_meta)
                .await
                .map_err(|e| format!("Failed to create child thread: {}", e))?;
        }
        let spawn_thread_store = self.thread_store.clone();
        let spawn_child_thread_id = bg_child_thread_id.clone();
        let _spawn_register_runtime = self.register_runtime.clone();
        let spawn_deregister_runtime = self.deregister_runtime.clone();
        let has_thread_store = self.thread_store.is_some();

        // Register AgentRuntime before spawning
        if let Some(ref register) = self.register_runtime {
            if self.thread_store.is_some() {
                let child_cancel = AgentCancellationToken::new();
                register(
                    bg_child_thread_id.clone(),
                    child_cancel,
                    "independent".to_string(),
                );
            }
        }

        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStart,
            &cwd,
            &agent_name,
            None,
        )
        .await;

        let handle = tokio::spawn(async move {
            let mut state = if let Some(ref store) = spawn_thread_store {
                AgentState::new(&cwd)
                    .with_persistence(Arc::clone(store), spawn_child_thread_id.clone())
            } else {
                AgentState::new(&cwd)
            };
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

            // Update child thread status
            if let Some(ref store) = spawn_thread_store {
                let status = if result.success { "done" } else { "error" };
                let _ = store
                    .update_thread_status(&spawn_child_thread_id, status)
                    .await;
            }

            spawn_registry.complete(&spawn_task_id, result.clone());

            fire_subagent_lifecycle_hooks_static(
                &spawn_hooks,
                crate::hooks::types::HookEvent::SubagentStop,
                &cwd,
                &spawn_agent_name,
                Some(&result.output),
            )
            .await;

            // 通过独立通道发送完成事件（不依赖 event_tx，不受 close_channel 影响）
            if let Some(ref sender) = spawn_bg_sender {
                tracing::info!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    success = result.success,
                    "[bg-diag] bg-task sending BackgroundTaskCompleted via bg_event_tx"
                );
                let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result));
            } else {
                tracing::warn!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    "[bg-diag] bg-task spawn_bg_sender is None — NOT sent"
                );
            }

            // Deregister AgentRuntime after execution completes
            if let Some(ref deregister) = spawn_deregister_runtime {
                if has_thread_store {
                    deregister(&spawn_child_thread_id);
                }
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

        // 通知 TUI background agent 启动（递增 background_task_count）。
        // 必须在 registry.register() 成功之后发送，防止注册失败留下幽灵计数。
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: agent_name.clone(),
                instance_id: task_id.clone(),
                is_background: true,
            });
        }

        if self.thread_store.is_some() {
            Ok(format!(
                "Background task {} started (thread: {}). You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id, bg_child_thread_id
            ))
        } else {
            Ok(format!(
                "Background task {} started. You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id
            ))
        }
    }

    async fn invoke_background_fork(
        &self,
        prompt: String,
        cwd: String,
        task_id: String,
        registry: &Arc<BackgroundTaskRegistry>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let agent_name = "fork".to_string();
        let prompt_summary: String = prompt.chars().take(100).collect();
        let fork_directive = crate::subagent::fork::build_fork_directive(&prompt);

        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Ok(
                "Error: Fork path requires parent message history, but parent_messages is not set"
                    .to_string(),
            ),
        };

        // Create child thread for background fork
        let bg_fork_child_thread_id = uuid::Uuid::now_v7().to_string();
        if let Some(ref store) = self.thread_store {
            let snapshot_id = parent_msgs.last().map(|m| m.id().as_uuid().to_string());
            let mut child_meta = ThreadMeta::new(&cwd);
            child_meta.id = bg_fork_child_thread_id.clone();
            child_meta.parent_thread_id = self.parent_thread_id.clone();
            child_meta.snapshot_at_message_id = snapshot_id;
            child_meta.hidden = true;
            child_meta.cancel_policy = "independent".to_string();
            child_meta.title = Some(format!("bg-fork-{}", task_id));
            store
                .create_thread(child_meta)
                .await
                .map_err(|e| format!("Failed to create child thread: {}", e))?;
        }

        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);
        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_fork(&cwd)) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder.register_tool(Box::new(ArcToolWrapper(Arc::clone(tool))));
        }

        let cancel_token = self.cancel.clone();
        let spawn_registry = Arc::clone(registry);
        let spawn_hooks = Arc::clone(&self.registered_hooks);
        let spawn_bg_sender = self.bg_event_sender.clone();
        let spawn_task_id = task_id.clone();
        let spawn_agent_name = agent_name.clone();
        let spawn_prompt_summary = prompt_summary.clone();
        let spawn_thread_store = self.thread_store.clone();
        let spawn_child_thread_id = bg_fork_child_thread_id.clone();
        let _spawn_register_runtime = self.register_runtime.clone();
        let spawn_deregister_runtime = self.deregister_runtime.clone();
        let has_thread_store = self.thread_store.is_some();

        // Register AgentRuntime before spawning
        if let Some(ref register) = self.register_runtime {
            if self.thread_store.is_some() {
                let child_cancel = AgentCancellationToken::new();
                register(
                    bg_fork_child_thread_id.clone(),
                    child_cancel,
                    "independent".to_string(),
                );
            }
        }

        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStart,
            &cwd,
            &agent_name,
            None,
        )
        .await;

        let handle = tokio::spawn(async move {
            let mut fork_state = if let Some(ref store) = spawn_thread_store {
                AgentState::new(&cwd)
                    .with_persistence(Arc::clone(store), spawn_child_thread_id.clone())
            } else {
                AgentState::new(&cwd)
            };
            // Inject parent messages for immediate execution
            for msg in parent_msgs {
                fork_state.add_message(msg);
            }
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

            // Update child thread status
            if let Some(ref store) = spawn_thread_store {
                let status = if result.success { "done" } else { "error" };
                let _ = store
                    .update_thread_status(&spawn_child_thread_id, status)
                    .await;
            }

            spawn_registry.complete(&spawn_task_id, result.clone());

            fire_subagent_lifecycle_hooks_static(
                &spawn_hooks,
                crate::hooks::types::HookEvent::SubagentStop,
                &cwd,
                &spawn_agent_name,
                Some(&result.output),
            )
            .await;

            // 通过独立通道发送完成事件（不依赖 event_tx，不受 close_channel 影响）
            if let Some(ref sender) = spawn_bg_sender {
                tracing::info!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    success = result.success,
                    "[bg-diag] bg-task sending BackgroundTaskCompleted via bg_event_tx"
                );
                let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result));
            } else {
                tracing::warn!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    "[bg-diag] bg-task spawn_bg_sender is None — NOT sent"
                );
            }

            // Deregister AgentRuntime after execution completes
            if let Some(ref deregister) = spawn_deregister_runtime {
                if has_thread_store {
                    deregister(&spawn_child_thread_id);
                }
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

        // 通知 TUI background agent 启动（递增 background_task_count）。
        // 必须在 registry.register() 成功之后发送，防止注册失败留下幽灵计数。
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: agent_name.clone(),
                instance_id: task_id.clone(),
                is_background: true,
            });
        }

        if self.thread_store.is_some() {
            Ok(format!(
                "Background task {} started (thread: {}). You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id, bg_fork_child_thread_id
            ))
        } else {
            Ok(format!(
                "Background task {} started. You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id
            ))
        }
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
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.parent_cwd)
            .to_string();
        let is_fork = input.get("fork").and_then(|v| v.as_bool()).unwrap_or(false)
            || subagent_type.as_deref() == Some("fork");

        if run_in_background && self.background_registry.is_some() {
            return self
                .invoke_background(prompt, subagent_type, cwd, is_fork)
                .await;
        }

        if is_fork {
            return self.invoke_fork(&prompt, &cwd).await;
        }

        let agent_id = match &subagent_type {
            Some(id) => id.clone(),
            None => {
                return Ok(
                    "Error: please provide subagent_type parameter to specify the agent type, or use fork: true for fork mode"
                        .to_string(),
                )
            }
        };

        // Create child thread for normal (non-fork, non-background) mode
        // instance_id 统一使用 child_thread_id（UUID v7，持久化线程标识）
        let child_thread_id = uuid::Uuid::now_v7().to_string();
        let instance_id = child_thread_id.clone();
        if let Some(ref store) = self.thread_store {
            let mut child_meta = ThreadMeta::new(&cwd);
            child_meta.id = child_thread_id.clone();
            child_meta.parent_thread_id = self.parent_thread_id.clone();
            child_meta.snapshot_at_message_id = None;
            child_meta.hidden = true;
            child_meta.cancel_policy = "cascade".to_string();
            child_meta.title = Some(agent_id.clone());
            store
                .create_thread(child_meta)
                .await
                .map_err(|e| format!("Failed to create child thread: {}", e))?;
        }

        let agent_def = match self.load_agent_def(&agent_id, &cwd) {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };

        let filtered_tools = self.filter_tools(
            &agent_def.frontmatter.tools,
            &agent_def.frontmatter.disallowed_tools,
        );

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

        if let Some(ref factory) = self.child_handler_factory {
            agent_builder = agent_builder.with_event_handler(factory(instance_id.clone()));
        } else if let Some(handler) = &self.event_handler {
            let tagged = Arc::new(SourceAgentIdHandler::new(
                Arc::clone(handler),
                instance_id.clone(),
            ));
            agent_builder = agent_builder.with_event_handler(tagged);
        }

        let mut state = if let Some(ref store) = self.thread_store {
            AgentState::new(cwd.clone())
                .with_persistence(Arc::clone(store), child_thread_id.clone())
        } else {
            AgentState::new(cwd.clone())
        };

        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: agent_id.clone(),
                instance_id: instance_id.clone(),
                is_background: false,
            });
        }
        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStart,
            &cwd,
            &agent_id,
            None,
        )
        .await;

        // Register AgentRuntime: only when thread_store is present (non-legacy path)
        // Panic-safe: DeregisterGuard ensures deregister runs on drop (panic or early return)
        let _deregister_guard = if self.thread_store.is_some() {
            if let Some(ref register) = self.register_runtime {
                let child_cancel = AgentCancellationToken::new();
                register(child_thread_id.clone(), child_cancel, "cascade".to_string());
            }
            DeregisterGuard {
                thread_id: child_thread_id.clone(),
                deregister: self.deregister_runtime.clone(),
            }
        } else {
            DeregisterGuard {
                thread_id: child_thread_id.clone(),
                deregister: None,
            }
        };

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
                instance_id: instance_id.clone(),
            });
        }
        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStop,
            &cwd,
            &agent_id,
            Some(&output_summary),
        )
        .await;

        match exec_result {
            Ok(output) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store.update_thread_status(&child_thread_id, "done").await;
                }
                let result_text = format_subagent_result(&output);
                if self.thread_store.is_some() {
                    Ok(format!(
                        "child_thread_id: {}
{}",
                        child_thread_id, result_text
                    ))
                } else {
                    Ok(result_text)
                }
            }
            Err(peri_agent::error::AgentError::Interrupted) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store
                        .update_thread_status(&child_thread_id, "cancelled")
                        .await;
                }
                Ok("Sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store.update_thread_status(&child_thread_id, "error").await;
                }
                let msg = format!("Sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }
}
