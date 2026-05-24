//! ACP Prompt execution — builds and executes the agent via peri_acp::executor.
//! Extracted from original acp_server.rs (2026-05-20 split).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::Value;
use tracing::info;

use peri_acp::broker::AcpTransportBroker;
use peri_acp::langfuse::LangfuseSession;
use peri_acp::session::event_sink::TransportEventSink;
use peri_acp::session::executor;
use peri_acp::transport::types::AcpError;
use peri_agent::agent::AgentCancellationToken;
use peri_middlewares::prelude::*;

use agent_client_protocol::schema::{PromptResponse, StopReason};

use crate::app::agent::LlmProvider;
use crate::config::PeriConfig;

use super::SharedSessions;

// ── Prompt execution (spawned into background task) ──────────────────────────

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_prompt(
    params: Value,
    sessions: &SharedSessions,
    provider: &Arc<RwLock<LlmProvider>>,
    peri_config: &Arc<RwLock<PeriConfig>>,
    permission_mode: &Arc<SharedPermissionMode>,
    cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    plugin_skill_dirs: &[std::path::PathBuf],
    plugin_agent_dirs: &[std::path::PathBuf],
    hook_groups: &[Vec<peri_middlewares::hooks::RegisteredHook>],
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>>,
    plugin_lsp_servers: &[peri_lsp::config::LspServerConfig],
    transport: &Arc<dyn peri_acp::transport::AcpTransport>,
    thread_store: &Arc<dyn peri_agent::thread::ThreadStore>,
    langfuse_session: Option<Arc<LangfuseSession>>,
    pool: Arc<parking_lot::Mutex<peri_acp::session::agent_pool::AgentPool>>,
) -> Result<Value, AcpError> {
    let session_id = params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?
        .to_string();
    let message = params
        .get("message")
        .ok_or_else(|| AcpError::new(-32602, "missing message"))?;
    let content: peri_agent::messages::MessageContent = message
        .get("content")
        .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
        .unwrap_or_default();

    // Create cancel token and register in sessions.
    let cancel = AgentCancellationToken::new();
    {
        let mut sessions = sessions.lock().await;
        let state = sessions
            .get_mut(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        state.cancel_token = Some(cancel.clone());
    }

    // Read session data under lock, then release immediately.
    let (
        cwd,
        history,
        is_empty,
        thread_id,
        frozen_system_prompt,
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
        incoming_recalls,
    ) = {
        let mut sessions = sessions.lock().await;
        let state = sessions
            .get_mut(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        (
            state.cwd.clone(),
            state.history.clone(),
            state.history.is_empty(),
            state.thread_id.clone(),
            state.frozen_system_prompt.clone(),
            state.frozen_claude_md.clone(),
            state.frozen_claude_local_md.clone(),
            state.frozen_skill_summary.clone(),
            state.frozen_date.clone(),
            std::mem::take(&mut state.recall_items),
        )
    };
    let history_len = history.len();

    let broker: Arc<dyn peri_agent::interaction::UserInteractionBroker> = Arc::new(
        AcpTransportBroker::new(Arc::clone(transport), session_id.clone().into()),
    );
    let event_sink = Arc::new(TransportEventSink::new(Arc::clone(transport)));

    let provider_snapshot = provider.read().clone();
    let peri_config_snapshot = Arc::new(peri_config.read().clone());

    let frozen = frozen_system_prompt.map(|sp| executor::FrozenSessionData {
        system_prompt: sp,
        claude_md: frozen_claude_md,
        claude_local_md: frozen_claude_local_md,
        skill_summary: frozen_skill_summary,
        date: frozen_date.unwrap_or_default(),
        is_git_repo: std::path::Path::new(&cwd).join(".git").exists(),
    });

    let result = executor::execute_prompt(
        &provider_snapshot,
        peri_config_snapshot,
        &cwd,
        content,
        frozen,
        history,
        incoming_recalls,
        is_empty,
        permission_mode.clone(),
        event_sink,
        cancel,
        broker,
        plugin_skill_dirs.to_vec(),
        plugin_agent_dirs.to_vec(),
        hook_groups.to_vec(),
        cron_scheduler,
        session_id.clone(),
        mcp_pool,
        tool_search_index,
        shared_tools,
        plugin_lsp_servers.to_vec(),
        langfuse_session,
        pool,
    )
    .await;

    // Persist new messages to ThreadStore and update in-memory state.
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            if result.ok {
                info!(session_id = %session_id, messages = result.messages.len(), "Agent execution completed");
                // Persist only the newly added messages.
                if history_len < result.messages.len() {
                    let new_msgs = &result.messages[history_len..];
                    if let Err(e) = thread_store.append_messages(&thread_id, new_msgs).await {
                        tracing::warn!(error = %e, "Failed to persist messages to ThreadStore");
                    }
                }
            }
            state.history = result.messages;
            state.recall_items = result.recall_items;
            state.cancel_token = None;
        }
    }

    let acp_stop_reason = match result.stop_reason {
        executor::PromptStopReason::Cancelled => StopReason::Cancelled,
        executor::PromptStopReason::MaxTurnRequests => StopReason::MaxTurnRequests,
        executor::PromptStopReason::EndTurn => StopReason::EndTurn,
    };
    let resp = PromptResponse::new(acp_stop_reason);
    serde_json::to_value(resp).map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
