//! Event sink abstraction for ACP session event routing.
//!
//! Different frontends (TUI via MpscTransport, IDE via stdio SDK) route agent
//! execution events differently. [`EventSink`] abstracts this so the core
//! prompt execution logic can live in `peri-acp`.

use async_trait::async_trait;
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use serde_json::json;
use tracing::{debug, error};

use crate::event::{map_executor_to_peri_notifications, map_executor_to_updates};
use crate::transport::AcpTransport;

// Re-export SDK types used by StdioEventSink.
pub use agent_client_protocol::schema::{
    SessionId as SdkSessionId, SessionNotification, SessionUpdate,
};
pub use agent_client_protocol::{Client, ConnectionTo};

/// Receives [`ExecutorEvent`]s produced during agent execution and routes them
/// to the appropriate transport.
#[async_trait]
pub trait EventSink: Send + Sync {
    /// Push a single executor event. Called from the background pump task.
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32);

    /// Signal that the agent execution stream has ended (no more events).
    async fn push_done(&self, session_id: &str);
}

// ── TUI transport-backed EventSink ──────────────────────────────────────────

/// [`EventSink`] backed by an [`AcpTransport`]. Sends three notification types:
/// - `peri/agent_event` — raw serialized ExecutorEvent (consumed by TUI pump)
/// - `peri/*` — custom notifications (compact, session lifecycle)
/// - `session/update` — standard ACP SessionUpdate notifications
pub struct TransportEventSink {
    transport: std::sync::Arc<dyn AcpTransport>,
}

impl TransportEventSink {
    pub fn new(transport: std::sync::Arc<dyn AcpTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl EventSink for TransportEventSink {
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32) {
        // 1. peri/agent_event — serialize ExecutorEvent to JSON string once
        let event_json = match serde_json::to_string(event) {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "EventSink: serialize ExecutorEvent failed");
                return;
            }
        };
        let agent_event_params = json!({
            "sessionId": session_id,
            "event_json": event_json,
        });
        if let Err(e) = self
            .transport
            .send_notification("peri/agent_event", agent_event_params)
            .await
        {
            error!(error = %e, "EventSink: send peri/agent_event failed");
            return;
        }

        // 2. peri/* custom notifications (compact, session lifecycle)
        let peri_notifs = map_executor_to_peri_notifications(event);
        for (method, mut payload) in peri_notifs {
            if let serde_json::Value::Object(ref mut map) = payload {
                map.insert("sessionId".to_string(), json!(session_id));
            }
            let _ = self.transport.send_notification(method, payload).await;
        }

        // 3. session/update — standard ACP SessionUpdate
        let updates = map_executor_to_updates(event, context_window);
        for update in updates {
            let mut payload = match serde_json::to_value(&update) {
                Ok(p) => p,
                Err(e) => {
                    error!(error = %e, "EventSink: serialize SessionUpdate failed");
                    continue;
                }
            };
            if let serde_json::Value::Object(ref mut map) = payload {
                map.insert("sessionId".to_string(), json!(session_id));
            }
            let _ = self
                .transport
                .send_notification("session/update", payload)
                .await;
        }
    }

    async fn push_done(&self, session_id: &str) {
        debug!(session_id = %session_id, "EventSink: sending agent_event_done");
        if let Err(e) = self
            .transport
            .send_notification("peri/agent_event_done", json!({ "sessionId": session_id }))
            .await
        {
            error!(session_id = %session_id, error = %e, "EventSink: agent_event_done send failed")
        }
    }
}

// ── SDK-backed EventSink for stdio path ─────────────────────────────────────

/// [`EventSink`] backed by the SDK's [`ConnectionTo<Client>`].
///
/// Sends standard ACP `session/update` notifications only (no `peri/*` custom
/// notifications — those are TUI-specific). Used by the stdio `peri acp` mode
/// which communicates with external IDE clients via the agent-client-protocol SDK.
pub struct StdioEventSink {
    cx: ConnectionTo<Client>,
    session_id: SdkSessionId,
}

impl StdioEventSink {
    pub fn new(cx: ConnectionTo<Client>, session_id: SdkSessionId) -> Self {
        Self { cx, session_id }
    }

    /// Send an arbitrary `SessionUpdate` notification through the SDK connection.
    pub fn send_update(&self, update: SessionUpdate) {
        let notif = SessionNotification::new(self.session_id.clone(), update);
        if let Err(e) = self.cx.send_notification(notif) {
            error!(error = %e, "StdioEventSink: failed to send SessionUpdate");
        }
    }
}

#[async_trait]
impl EventSink for StdioEventSink {
    async fn push_event(&self, _session_id: &str, event: &ExecutorEvent, context_window: u32) {
        let updates = map_executor_to_updates(event, context_window);
        for update in updates {
            let notif = SessionNotification::new(self.session_id.clone(), update);
            if let Err(e) = self.cx.send_notification(notif) {
                error!(error = %e, "StdioEventSink: failed to send SessionNotification");
                break;
            }
        }
    }

    async fn push_done(&self, _session_id: &str) {
        // No explicit done signal in standard ACP protocol.
    }
}
