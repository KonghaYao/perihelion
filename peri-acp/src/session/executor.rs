//! Shared prompt execution logic.
//!
//! Provides [`execute_prompt`] which encapsulates the common agent execution
//! pipeline used by both TUI (via [`TransportEventSink`]) and stdio (via
//! [`StdioEventSink`]) paths.
//!
//! Compact 由 CompactMiddleware（before_model 钩子）在 ReAct 循环内原地处理，
//! 不再需要外层 loop + resubmit。

use std::sync::Arc;

use peri_agent::{
    agent::{
        events::{AgentEvent as ExecutorEvent, AgentEventHandler},
        state::AgentState,
        token::ContextBudget,
        AgentCancellationToken, State,
    },
    error::AgentError,
    interaction::{ChannelState, UserInteractionBroker},
    messages::{BaseMessage, ContentBlock, MessageContent, MessageId},
};
use tokio::sync::oneshot;
use tracing::{debug, error};

use crate::{
    agent::builder::{self, AcpAgentConfig},
    langfuse::{LangfuseSession, LangfuseTracer},
    prompt::{build_system_prompt, PromptFeatures},
    provider::LlmProvider,
    session::{
        agent_pool::AgentPool,
        agent_runtime::{AgentRuntime, CancelPolicy},
        event_sink::EventSink,
        SessionManager,
    },
};

/// High-level reason why prompt execution stopped, used to derive ACP `StopReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStopReason {
    /// Normal completion — the agent finished its turn.
    EndTurn,
    /// The user cancelled via `session/cancel`.
    Cancelled,
    /// The agent reached the maximum number of iterations.
    MaxTurnRequests,
}

/// Result of prompt execution.
pub struct PromptResult {
    /// Updated message history after execution.
    pub messages: Vec<BaseMessage>,
    /// Whether execution succeeded.
    pub ok: bool,
    /// Why the prompt execution stopped.
    pub stop_reason: PromptStopReason,
    /// Recall items collected during execution (for next turn injection).
    pub recall_items: Vec<String>,
}

/// Session-scoped frozen data that locks system prompt stability.
///
/// Populated at session creation time by `session/new`, passed through to
/// every turn's agent build to guarantee the system prompt never changes
/// within a session.
#[derive(Clone)]
pub struct FrozenSessionData {
    /// Full system prompt string built at session creation.
    pub system_prompt: String,
    /// Frozen content of CLAUDE.md (with resolved `@import`), None if no file.
    pub claude_md: Option<String>,
    /// Frozen content of CLAUDE.local.md, None if no file.
    pub claude_local_md: Option<String>,
    /// Frozen skills summary string, None if no skills.
    pub skill_summary: Option<String>,
    /// Session creation date in YYYY-MM-DD format.
    pub date: String,
    /// Whether cwd was a git repo at session creation time.
    pub is_git_repo: bool,
    /// Session creation language preference (e.g. "zh-CN", "en").
    /// None = auto-detect from user input (no explicit instruction).
    pub language: Option<String>,
}

/// Shared agent execution pipeline with auto-compact support.
///
/// This function encapsulates steps 2-7 of the prompt execution flow:
/// 1. Create event channel + cancel token
/// 2. Build agent via [`build_system_prompt`] + [`builder::build_agent`]
/// 3. Spawn background event pump using the provided [`EventSink`]
/// 4. Execute agent
/// 5. Auto-compact handled by CompactMiddleware (before_model hook)
/// 6. Wait for pump to drain
/// 7. Return updated messages
///
/// The caller is responsible for:
/// - Session management (storing/retrieving cwd, history, cancel_token)
/// - Choosing the broker (HITL/AskUser handler)
/// - Providing the correct `EventSink` implementation
#[allow(clippy::too_many_arguments)]
pub async fn execute_prompt(
    provider: &LlmProvider,
    peri_config: Arc<crate::provider::PeriConfig>,
    cwd: &str,
    content: MessageContent,
    frozen: Option<FrozenSessionData>,
    history: Vec<BaseMessage>,
    incoming_recalls: Vec<String>,
    is_empty_history: bool,
    permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    event_sink: Arc<dyn EventSink>,
    cancel: AgentCancellationToken,
    broker: Arc<dyn UserInteractionBroker>,
    plugin_skill_dirs: Vec<std::path::PathBuf>,
    plugin_agent_dirs: Vec<std::path::PathBuf>,
    hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    session_id: String,
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    channel_state: Option<Arc<ChannelState>>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    langfuse_session: Option<Arc<LangfuseSession>>,
    pool: Arc<parking_lot::Mutex<AgentPool>>,
    thread_store: Option<Arc<dyn peri_agent::thread::ThreadStore>>,
    thread_id: Option<String>,
    session_manager: Option<SessionManager>,
    bg_results: Vec<peri_agent::agent::events::BackgroundTaskResult>,
) -> PromptResult {
    // Inject synthetic AgentResult tool_use + tool_result messages when bg_results present
    let (history, content) = if !bg_results.is_empty() {
        inject_bg_result_messages(history, content, &bg_results)
    } else {
        (history, content)
    };

    // Compact config — computed early for command interception and agent building.
    let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();
    let disable_compact = std::env::var("DISABLE_COMPACT").is_ok()
        || std::env::var("DISABLE_AUTO_COMPACT").is_ok()
        || !compact_config.auto_compact_enabled;

    // Compact model — reuse AgentPool cache if available, otherwise create fresh.
    let cached_llm = {
        let pool_guard = pool.lock();
        if pool_guard.has_valid_cache(provider) {
            pool_guard.get_cached_llm().cloned()
        } else {
            None
        }
    };
    let compact_model: Option<Arc<dyn peri_agent::llm::BaseModel>> = if disable_compact {
        None
    } else {
        cached_llm
            .as_ref()
            .map(|c| c.compact_model.clone())
            .or_else(|| Some(provider.clone().into_model().into()))
    };

    // Command interception — check if content is a slash command before building agent.
    if let Some(text) = content.text_content().strip_prefix('/') {
        if !text.is_empty() {
            let command_registry = crate::session::command::default_command_registry();
            if let Some((cmd, args)) = command_registry.find(&content.text_content()) {
                if cmd.kind() == crate::session::command::CommandKind::Immediate {
                    tracing::debug!(
                        command = %cmd.name(),
                        history_len = history.len(),
                        "Immediate command intercepted"
                    );
                    let ctx = crate::session::command::CommandContext {
                        session_id: session_id.clone(),
                        history: history.clone(),
                        cwd: cwd.to_string(),
                        peri_config: Arc::new(peri_config.as_ref().clone()),
                        compact_model: compact_model.clone(),
                        event_sink: event_sink.clone(),
                        args: args.to_string(),
                        cancel_token: cancel.clone(),
                        thread_store: thread_store.clone(),
                        thread_id: thread_id.clone(),
                    };
                    let result = tokio::select! {
                        r = cmd.execute(ctx) => r,
                        _ = cancel.cancelled() => {
                            tracing::info!(session_id = %session_id, "Immediate command cancelled");
                            crate::session::command::CommandResult {
                                messages: history,
                                stop_reason: PromptStopReason::Cancelled,
                            }
                        }
                    };
                    // Immediate 命令跳过 agent event pump，必须手动发送 push_done
                    // 通知 TUI agent 执行完成，否则界面永久卡在 loading 状态。
                    event_sink.push_done(&session_id).await;
                    return PromptResult {
                        messages: result.messages,
                        ok: true,
                        stop_reason: result.stop_reason,
                        recall_items: Vec::new(),
                    };
                }
                // Passthrough/Transform → fall through to normal agent flow
            }
        }
    }

    let trace_input = content.text_content();
    let agent_input = if incoming_recalls.is_empty() {
        peri_agent::agent::react::AgentInput::blocks(content)
    } else {
        use peri_agent::messages::ContentBlock;
        let reminder_text = format!(
            "<system-reminder>\n{}\n</system-reminder>",
            incoming_recalls.join("\n")
        );
        let mut blocks = content.content_blocks();
        blocks.push(ContentBlock::text(reminder_text));
        peri_agent::agent::react::AgentInput::blocks(MessageContent::blocks(blocks))
    };

    // Context budget (computed once, uses compact_config from above)
    let context_window = provider.context_window();
    let context_1m = peri_config.config.context_1m.unwrap_or(false);
    let effective_context_window = if context_1m {
        1_000_000
    } else {
        context_window
    };
    let budget = ContextBudget::new(effective_context_window)
        .with_auto_compact_threshold(compact_config.auto_compact_threshold)
        .with_warning_threshold(compact_config.micro_compact_threshold);

    // Event channel (lives for entire execute_prompt lifetime)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));

    // Main event pump
    let sink = event_sink;
    let bg_sink = Arc::clone(&sink);
    let sid = session_id.clone();
    let (pump_done_tx, pump_done_rx) = oneshot::channel();
    let pump_cw = effective_context_window;

    // Langfuse per-turn tracer
    let langfuse_tracer = langfuse_session
        .as_ref()
        .map(|s| parking_lot::Mutex::new(LangfuseTracer::new(Arc::clone(s), session_id.clone())));
    if langfuse_tracer.is_some() {
        debug!(session_id = %session_id, "Langfuse tracer created for turn");
    }

    let provider_display_name = provider.display_name().to_string();

    tokio::spawn(async move {
        // Start Langfuse trace
        if let Some(ref tracer) = langfuse_tracer {
            tracer.lock().on_trace_start(&trace_input);
        }

        while let Some(exec_event) = event_rx.recv().await {
            // Langfuse tracing
            if let Some(ref tracer) = langfuse_tracer {
                match &exec_event {
                    ExecutorEvent::LlmCallStart {
                        step,
                        messages,
                        tools,
                    } => {
                        tracer.lock().on_llm_start(*step, messages, tools);
                    }
                    ExecutorEvent::LlmCallEnd {
                        step,
                        model,
                        output,
                        usage,
                        stop_reason: _,
                    } => {
                        tracer.lock().on_llm_end(
                            *step,
                            model,
                            &provider_display_name,
                            output,
                            usage.as_ref(),
                        );
                    }
                    ExecutorEvent::ToolStart {
                        tool_call_id,
                        name,
                        input,
                        ..
                    } => {
                        tracer.lock().on_tool_start(tool_call_id, name, input);
                    }
                    ExecutorEvent::ToolEnd {
                        tool_call_id,
                        output,
                        is_error,
                        ..
                    } => {
                        tracer.lock().on_tool_end(tool_call_id, output, *is_error);
                    }
                    ExecutorEvent::TextChunk { chunk, .. } => {
                        tracer.lock().on_text_chunk(chunk);
                    }
                    ExecutorEvent::LlmRetrying {
                        attempt,
                        max_attempts,
                        delay_ms,
                        error,
                    } => {
                        tracer
                            .lock()
                            .on_llm_retrying(*attempt, *max_attempts, *delay_ms, error);
                    }
                    ExecutorEvent::CompactStarted => {
                        tracer.lock().on_compact_start();
                    }
                    ExecutorEvent::CompactCompleted {
                        summary,
                        files,
                        skills,
                        micro_cleared,
                        ..
                    } => {
                        tracer.lock().on_compact_end(
                            summary,
                            files.len(),
                            skills.len(),
                            *micro_cleared,
                            false,
                            "",
                        );
                    }
                    ExecutorEvent::CompactError { message } => {
                        tracer.lock().on_compact_end("", 0, 0, 0, true, message);
                    }
                    _ => {}
                }
            }

            sink.push_event(&sid, &exec_event, pump_cw).await;
        }

        // End Langfuse trace and flush
        let langfuse_flush = if let Some(tracer) = langfuse_tracer {
            let handle = tracer.into_inner().on_trace_end(None);
            Some(handle)
        } else {
            None
        };

        sink.push_done(&sid).await;

        // Wait for Langfuse flush before exiting pump
        if let Some(handle) = langfuse_flush {
            let _ = handle.await;
        }

        let _ = pump_done_tx.send(());
    });

    // 单次 Agent 执行（compact 由 CompactMiddleware 在循环内处理）
    let event_handler: Arc<dyn AgentEventHandler> =
        Arc::new(peri_agent::agent::events::FnEventHandler({
            let tx = event_tx.clone();
            move |event: ExecutorEvent| {
                if let Some(tx) = tx.lock().unwrap().as_ref() {
                    let _ = tx.send(event);
                }
            }
        }));

    let language = frozen
        .as_ref()
        .and_then(|f| f.language.clone())
        .or_else(|| peri_config.config.language.clone());

    let (
        system_prompt,
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
    ) = if let Some(ref f) = frozen {
        // 使用 session 创建时冻结的数据，跳过重建
        (
            f.system_prompt.clone(),
            f.claude_md.clone(),
            f.claude_local_md.clone(),
            f.skill_summary.clone(),
            Some(f.date.clone()),
        )
    } else {
        // Legacy: per-turn rebuild（子 Agent 等场景未提供 frozen 数据时使用）
        let features = PromptFeatures::detect();
        let sp = build_system_prompt(
            None,
            cwd,
            features,
            &plugin_agent_dirs,
            None,
            language.as_deref(),
        );
        (sp, None, None, None, None)
    };

    // Build register/deregister closures for SubAgentMiddleware
    let register_runtime = session_manager.clone().map(|sm| {
        let sid = session_id.clone();
        Arc::new(
            move |thread_id: String, cancel_token: AgentCancellationToken, policy: String| {
                if let Some(mut session) = sm.get_session_mut(&sid) {
                    let runtime =
                        AgentRuntime::new(thread_id.clone(), CancelPolicy::from_str(&policy));
                    // Store the provided cancel_token so external cancellation works
                    let rt = AgentRuntime {
                        thread_id,
                        cancel_token,
                        cancel_policy: runtime.cancel_policy,
                        status: runtime.status,
                    };
                    session.active_agents.insert(rt.thread_id.clone(), rt);
                }
            },
        ) as crate::agent::builder::RegisterRuntimeFn
    });
    let deregister_runtime = session_manager.clone().map(|sm| {
        let sid = session_id.clone();
        Arc::new(move |thread_id: &str| {
            if let Some(mut session) = sm.get_session_mut(&sid) {
                session.active_agents.remove(thread_id);
            }
        }) as crate::agent::builder::DeregisterRuntimeFn
    });

    let (agent_output, new_cache) = builder::build_agent(
        AcpAgentConfig {
            provider: provider.clone(),
            cwd: cwd.to_string(),
            system_prompt,
            frozen_claude_md,
            frozen_claude_local_md,
            frozen_skill_summary,
            frozen_date,
            event_handler,
            cancel: cancel.clone(),
            permission_mode: permission_mode.clone(),
            peri_config: Arc::new(peri_config.as_ref().clone()),
            cron_scheduler: cron_scheduler.clone(),
            agent_overrides: None,
            preload_skills: Vec::new(),
            session_id: Some(session_id.clone()),
            broker: broker.clone(),
            plugin_skill_dirs: plugin_skill_dirs.clone(),
            plugin_agent_dirs: plugin_agent_dirs.clone(),
            hook_groups: hook_groups.clone(),
            hook_session_start: is_empty_history,
            mcp_pool: mcp_pool.clone(),
            channel_state: channel_state.clone(),
            tool_search_index: tool_search_index.clone(),
            shared_tools: shared_tools.clone(),
            child_handler_factory: None,
            lsp_servers: lsp_servers.clone(),
            compact_config: if disable_compact {
                None
            } else {
                Some(compact_config)
            },
            compact_budget: if disable_compact { None } else { Some(budget) },
            compact_model, // already Option<Arc<dyn BaseModel>> from pool/fresh logic
            compact_event_tx: Some(event_tx.clone()),
            thread_store,
            parent_thread_id: thread_id,
            register_runtime,
            deregister_runtime,
        },
        cached_llm.as_ref(),
        &pool,
    );

    // Store updated cache back into pool
    if let Some(cache) = new_cache {
        pool.lock().store_llm(cache);
    }

    // Phase 2: bg event pump — starts before executor runs so events arrive
    // promptly even for tasks completing mid-execution. Outlives executor;
    // exits when all bg spawn closures finish and drop their senders.
    {
        let mut bg_event_rx = agent_output.bg_event_rx;
        let bg_session_id = session_id.clone();
        let bg_cw = effective_context_window;
        tokio::spawn(async move {
            let mut bg_event_count: u64 = 0;
            while let Some(bg_event) = bg_event_rx.recv().await {
                bg_event_count += 1;
                if matches!(&bg_event, ExecutorEvent::BackgroundTaskCompleted(_)) {
                    tracing::info!(
                        count = bg_event_count,
                        "[bg-diag] bg-event-pump: received BackgroundTaskCompleted"
                    );
                }
                bg_sink.push_event(&bg_session_id, &bg_event, bg_cw).await;
            }
            tracing::info!(
                total_bg_events = bg_event_count,
                "[bg-diag] bg-event-pump: channel closed, exiting"
            );
        });
    }

    // 转发 todo 更新为 ExecutorEvent::TodoUpdate
    let mut todo_rx = agent_output.todo_rx;
    let tx_for_todo = event_tx.clone();
    tokio::spawn(async move {
        while let Some(todos) = todo_rx.recv().await {
            let entries: Vec<peri_agent::agent::events::TodoEntry> = todos
                .into_iter()
                .map(|t| peri_agent::agent::events::TodoEntry {
                    content: t.content,
                    active_form: t.active_form,
                    status: match t.status {
                        peri_middlewares::tools::todo::TodoStatus::Pending => {
                            peri_agent::agent::events::TodoStatus::Pending
                        }
                        peri_middlewares::tools::todo::TodoStatus::InProgress => {
                            peri_agent::agent::events::TodoStatus::InProgress
                        }
                        peri_middlewares::tools::todo::TodoStatus::Completed => {
                            peri_agent::agent::events::TodoStatus::Completed
                        }
                    },
                })
                .collect();
            if let Some(tx) = tx_for_todo.lock().unwrap().as_ref() {
                let _ = tx.send(ExecutorEvent::TodoUpdate(entries));
            }
        }
    });

    // Execute agent
    let mut agent_state = AgentState::with_messages(cwd.to_string(), history);
    let result = agent_output
        .executor
        .execute(agent_input.clone(), &mut agent_state, Some(cancel.clone()))
        .await;
    drop(agent_output.executor);

    let ok = result.is_ok();
    if let Err(e) = &result {
        error!(session_id = %session_id, error = %e, "Agent execution failed");
        if let Some(tx) = event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(ExecutorEvent::AgentExecutionFailed {
                message: e.to_string(),
            });
        }
    }

    let stop_reason = if cancel.is_cancelled() {
        PromptStopReason::Cancelled
    } else if matches!(&result, Err(AgentError::MaxIterationsExceeded(_))) {
        PromptStopReason::MaxTurnRequests
    } else if matches!(&result, Err(AgentError::Interrupted)) {
        PromptStopReason::Cancelled
    } else {
        PromptStopReason::EndTurn
    };

    // Cancel cascade children when this agent is cancelled
    if stop_reason == PromptStopReason::Cancelled {
        if let Some(ref sm) = session_manager {
            if let Some(session) = sm.get_session(&session_id) {
                session.cancel_cascade_children();
            }
        }
    }

    close_channel(&event_tx);
    wait_for_pump(pump_done_rx, &session_id).await;

    let recall_items = agent_state.drain_recall();
    PromptResult {
        messages: agent_state.into_messages(),
        ok,
        stop_reason,
        recall_items,
    }
}

fn close_channel(
    event_tx: &Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<ExecutorEvent>>>>,
) {
    let mut tx_guard = event_tx.lock().unwrap();
    *tx_guard = None;
}

async fn wait_for_pump(pump_done_rx: oneshot::Receiver<()>, session_id: &str) {
    match pump_done_rx.await {
        Ok(()) => debug!(session_id, "Event pump done"),
        Err(_) => error!(session_id, "Event pump done channel closed unexpectedly"),
    }
}

/// Inject synthetic AgentResult tool_use + tool_result messages into the conversation history.
///
/// When background agents complete, the TUI sends a `prompt_with_bg_results` request.
/// This function prepends synthetic messages to the history so the LLM sees them as
/// prior tool calls and results:
///
/// ```text
/// history (prepended):
///   AI: [ToolUse(AgentResult, task_id=xxx), ToolUse(AgentResult, task_id=yyy)]
///   Tool: [tool_result for xxx]
///   Tool: [tool_result for yyy]
/// ```
///
/// Returns modified history with synthetic messages prepended.
fn inject_bg_result_messages(
    mut history: Vec<BaseMessage>,
    user_content: MessageContent,
    bg_results: &[peri_agent::agent::events::BackgroundTaskResult],
) -> (Vec<BaseMessage>, MessageContent) {
    // Build tool_use blocks (one per bg result) and collect ID mappings
    let mut tool_use_blocks = Vec::new();
    let mut tool_result_data: Vec<(String, &peri_agent::agent::events::BackgroundTaskResult)> =
        Vec::new();

    for result in bg_results {
        let tool_use_id = MessageId::new().as_uuid().to_string();
        tool_use_blocks.push(ContentBlock::ToolUse {
            id: tool_use_id.clone(),
            name: "AgentResult".to_string(),
            input: serde_json::json!({
                "task_id": result.task_id,
            }),
        });
        tool_result_data.push((tool_use_id, result));
    }

    // 1. AI message with tool_use blocks
    let ai_msg = BaseMessage::ai_from_blocks(tool_use_blocks);
    history.push(ai_msg);

    // 2. One tool_result message per bg result
    for (tool_use_id, result) in tool_result_data {
        let result_text = result.to_notification();
        let tool_msg = if result.success {
            BaseMessage::tool_result(&tool_use_id, result_text)
        } else {
            BaseMessage::tool_error(&tool_use_id, result_text)
        };
        history.push(tool_msg);
    }

    (history, user_content)
}
