//! ACP Notification dispatch — handles incoming notifications and pushes
//! session update notifications. Extracted from original acp_server.rs (2026-05-20 split).

use std::collections::HashMap;

use serde_json::Value;
use tracing::{debug, info};

use agent_client_protocol::schema::{AvailableCommandsUpdate, SessionUpdate};

use super::{AcpServerConfig, SessionState};
use peri_acp::dispatch::config_update;
use peri_middlewares::skills::SkillMetadata;

// ── Notification dispatch ────────────────────────────────────────────────────

pub(crate) fn handle_notification(
    method: &str,
    params: &Value,
    sessions: &HashMap<String, SessionState>,
) {
    if method == "session/cancel" {
        let session_id = extract_session_id(params, "");
        if let Some(state) = sessions.get(session_id) {
            if let Some(ref token) = state.cancel_token {
                token.cancel();
                info!(session_id = %session_id, "Cancel requested");
            }
        }
    } else {
        debug!(method = %method, "Unhandled notification");
    }
}

// ── Notification helpers ───────────────────────────────────────────────────────

/// Extract `sessionId` from JSON-RPC params, returning `default_value` if absent.
pub(crate) fn extract_session_id<'a>(params: &'a Value, default_value: &'a str) -> &'a str {
    params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_value)
}

/// Build the current set of config options and push a `ConfigOptionUpdate` notification.
pub(crate) async fn send_config_option_update(
    transport: &dyn peri_acp::transport::AcpTransport,
    session_id: &str,
    cfg: &AcpServerConfig,
) {
    if session_id.is_empty() {
        return;
    }
    let update = {
        let c = cfg.peri_config.read();
        let p = cfg.provider.read();
        SessionUpdate::ConfigOptionUpdate(config_update::make_config_option_update(
            &c,
            &p,
            cfg.permission_mode.load(),
        ))
    };
    let update_value = match serde_json::to_value(&update) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize ConfigOptionUpdate");
            return;
        }
    };
    let payload = serde_json::json!({
        "sessionId": session_id,
        "update": update_value,
    });
    let _ = transport.send_notification("session/update", payload).await;
}

/// Push an `AvailableCommandsUpdate` notification for the given session.
pub(crate) async fn send_available_commands_update(
    transport: &dyn peri_acp::transport::AcpTransport,
    session_id: &str,
    skills: &[SkillMetadata],
) {
    if session_id.is_empty() {
        return;
    }
    let commands = peri_acp::dispatch::build_available_commands(skills);
    let update = SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands));
    let update_value = match serde_json::to_value(&update) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize AvailableCommandsUpdate");
            return;
        }
    };
    // Use {"update": ..., "sessionId": ...} format — same as TransportEventSink —
    // so that handle_session_update_peri on the TUI side can parse via params.get("update").
    let payload = serde_json::json!({
        "sessionId": session_id,
        "update": update_value,
    });
    let _ = transport.send_notification("session/update", payload).await;
}

/// Push a `SessionInfoUpdate` notification after prompt/compact completes.
pub(crate) async fn send_session_info_update(
    transport: &dyn peri_acp::transport::AcpTransport,
    session_id: &str,
) {
    use agent_client_protocol::schema::SessionInfoUpdate;
    let info = SessionInfoUpdate::new().updated_at(chrono::Utc::now().to_rfc3339());
    let update = SessionUpdate::SessionInfoUpdate(info);
    let update_value = match serde_json::to_value(&update) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize SessionInfoUpdate");
            return;
        }
    };
    let payload = serde_json::json!({
        "sessionId": session_id,
        "update": update_value,
    });
    let _ = transport.send_notification("session/update", payload).await;
}
