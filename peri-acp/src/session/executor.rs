//! Shared prompt execution logic.
//!
//! Provides [`execute_prompt`] which encapsulates the common agent execution
//! pipeline used by both TUI (via [`TransportEventSink`]) and stdio (via
//! [`StdioEventSink`]) paths.
//!
//! Compact 由 CompactMiddleware（before_model 钩子）在 ReAct 循环内原地处理，
//! 不再需要外层 loop + resubmit。

use std::sync::Arc;

use peri_agent::agent::events::{AgentEvent as ExecutorEvent, AgentEventHandler};
use peri_agent::agent::state::AgentState;
use peri_agent::agent::token::ContextBudget;
use peri_agent::agent::AgentCancellationToken;
use peri_agent::agent::State;
use peri_agent::error::AgentError;
use peri_agent::interaction::UserInteractionBroker;
use peri_agent::messages::{BaseMessage, MessageContent};
use tokio::sync::oneshot;
use tracing::{debug, error};

use crate::agent::builder::{self, AcpAgentConfig};
use crate::langfuse::{LangfuseSession, LangfuseTracer};
use crate::prompt::{build_system_prompt, PromptFeatures};
use crate::provider::LlmProvider;
use crate::session::agent_pool::AgentPool;
use crate::session::event_sink::EventSink;

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
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    langfuse_session: Option<Arc<LangfuseSession>>,
    pool: Arc<parking_lot::Mutex<AgentPool>>,
) -> PromptResult {
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

    // Compact config and context budget (computed once)
    let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();
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

    let disable_compact = std::env::var("DISABLE_COMPACT").is_ok()
        || std::env::var("DISABLE_AUTO_COMPACT").is_ok()
        || !compact_config.auto_compact_enabled;

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
        let sp = build_system_prompt(None, cwd, features, &plugin_agent_dirs, None);
        (sp, None, None, None, None)
    };

    // Compact model（用于 CompactMiddleware 的 full compact 摘要生成）
    // 从 AgentPool 复用缓存的 LLM 实例，避免每轮重建 reqwest::Client
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
        },
        cached_llm.as_ref(),
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
            while let Some(bg_event) = bg_event_rx.recv().await {
                bg_sink.push_event(&bg_session_id, &bg_event, bg_cw).await;
            }
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
