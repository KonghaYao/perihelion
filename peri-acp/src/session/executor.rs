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
use peri_agent::error::AgentError;
use peri_agent::interaction::UserInteractionBroker;
use peri_agent::messages::BaseMessage;
use tokio::sync::oneshot;
use tracing::{debug, error};

use crate::agent::builder::{self, AcpAgentConfig};
use crate::prompt::{build_system_prompt, PromptFeatures};
use crate::provider::LlmProvider;
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
    content: String,
    history: Vec<BaseMessage>,
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
) -> PromptResult {
    let agent_input = peri_agent::agent::react::AgentInput::text(content);

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

    // Background event pump
    let sink = event_sink;
    let sid = session_id.clone();
    let (pump_done_tx, pump_done_rx) = oneshot::channel();
    let pump_cw = effective_context_window;
    tokio::spawn(async move {
        while let Some(exec_event) = event_rx.recv().await {
            sink.push_event(&sid, &exec_event, pump_cw).await;
        }
        sink.push_done(&sid).await;
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

    let features = PromptFeatures::detect();
    let system_prompt = build_system_prompt(None, cwd, features, &plugin_agent_dirs);

    // Compact model（用于 CompactMiddleware 的 full compact 摘要生成）
    let compact_model: Arc<dyn peri_agent::llm::BaseModel> = provider.clone().into_model().into();

    let agent_output = builder::build_agent(AcpAgentConfig {
        provider: provider.clone(),
        cwd: cwd.to_string(),
        system_prompt,
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
        compact_model: if disable_compact {
            None
        } else {
            Some(compact_model)
        },
        compact_event_tx: Some(event_tx.clone()),
    });

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

    PromptResult {
        messages: agent_state.into_messages(),
        ok,
        stop_reason,
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
