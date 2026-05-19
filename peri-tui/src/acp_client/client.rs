//! Thin TUI-side wrapper around [`peri_acp::transport::mpsc::MpscClientTransport`].
//!
//! Translates raw [`IncomingMessage`]s into [`AcpNotification`]s for the TUI event
//! loop to consume. The notification pump runs as a background tokio task.

use peri_acp::transport::mpsc::MpscClientTransport;
use peri_acp::transport::types::{AcpError, IncomingMessage, RequestId};
use peri_acp::transport::AcpTransport;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// Notification events dispatched from the background pump to the TUI event loop.
pub enum AcpNotification {
    /// A `notifications/agent_event` notification carrying a peri-agent ExecutorEvent.
    /// The TUI converts this to its own AgentEvent via `map_executor_event`.
    AgentEvent {
        session_id: String,
        event: peri_agent::agent::events::AgentEvent,
    },
    /// A `notifications/session_update` notification from the ACP server.
    SessionUpdate { session_id: String, params: Value },
    /// A `RequestPermission` request requiring HITL interaction.
    RequestPermission { id: RequestId, params: Value },
    /// An `elicitation/create` request requiring AskUser interaction.
    Elicitation { id: RequestId, params: Value },
    /// An unrecognized notification or request.
    Other { msg: String },
    /// Agent execution completed (synthetic notification from ACP server).
    AgentDone { session_id: String },
    /// A `notifications/peri/*` custom notification (SubAgent, Compact, LSP, etc.)
    Peri {
        session_id: String,
        method: String,
        params: Value,
    },
}

/// TUI-side client that owns the ACP transport and routes notifications.
///
/// Uses `Arc<Mutex<Option<String>>>` for `current_session_id` so that
/// clones (e.g., in `interrupt()` and `submit_message()`'s async task)
/// share the same session state.
#[derive(Clone)]
pub struct AcpTuiClient {
    transport: Arc<MpscClientTransport>,
    notification_tx: mpsc::UnboundedSender<AcpNotification>,
    current_session_id: Arc<Mutex<Option<String>>>,
}

impl AcpTuiClient {
    /// Check whether a session has been created.
    pub fn has_session(&self) -> bool {
        self.current_session_id.lock().unwrap().is_some()
    }

    /// Create a new client wrapping an existing `MpscClientTransport`.
    ///
    /// Returns `(Self, notification_receiver)`. The caller must:
    /// 1. Move `notification_receiver` to the TUI event loop (`AgentComm.acp_notification_rx`)
    /// 2. Spawn the pump via [`AcpTuiClient::spawn_pump`]
    pub fn new(transport: MpscClientTransport) -> (Self, mpsc::UnboundedReceiver<AcpNotification>) {
        let (notification_tx, notification_rx) = mpsc::unbounded_channel();
        let client = Self {
            transport: Arc::new(transport),
            notification_tx,
            current_session_id: Arc::new(Mutex::new(None)),
        };
        (client, notification_rx)
    }

    /// Spawn the notification pump as a tokio task. Consumes internal clones of
    /// transport and notification sender.
    pub fn spawn_pump(&self) {
        let transport = self.transport.clone();
        let notification_tx = self.notification_tx.clone();
        tokio::spawn(async move {
            Self::run_pump(transport, notification_tx).await;
        });
    }

    // ── Pump ──

    /// Background task that polls the transport and dispatches notifications.
    async fn run_pump(
        transport: Arc<MpscClientTransport>,
        notification_tx: mpsc::UnboundedSender<AcpNotification>,
    ) {
        let mut event_count: u64 = 0;
        loop {
            let msg = transport.recv().await;
            match msg {
                Some(IncomingMessage::Notification { method, params }) => {
                    if method == "peri/agent_event" {
                        event_count += 1;
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if let Some(event_value) = params.get("event") {
                            match serde_json::from_value::<peri_agent::agent::events::AgentEvent>(
                                event_value.clone(),
                            ) {
                                Ok(event) => {
                                    debug!(
                                        event_count = event_count,
                                        session_id = %session_id,
                                        "ACP client pump: received agent_event"
                                    );
                                    let _ = notification_tx
                                        .send(AcpNotification::AgentEvent { session_id, event });
                                }
                                Err(e) => {
                                    error!(
                                        event_count = event_count,
                                        error = %e,
                                        event_json = %event_value,
                                        "ACP client pump: failed to parse AgentEvent — event LOST"
                                    );
                                    let _ = notification_tx.send(AcpNotification::Other {
                                        msg: format!("failed to parse AgentEvent: {e}"),
                                    });
                                }
                            }
                        } else {
                            warn!(
                                "ACP client pump: agent_event notification missing 'event' field"
                            );
                        }
                    } else if method == "session/update" {
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let _ = notification_tx
                            .send(AcpNotification::SessionUpdate { session_id, params });
                    } else if method == "peri/agent_event_done" {
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        debug!(
                            session_id = %session_id,
                            total_events = event_count,
                            "ACP client pump: received agent_event_done"
                        );
                        let _ = notification_tx.send(AcpNotification::AgentDone { session_id });
                    } else if method.starts_with("notifications/peri/") {
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let _ = notification_tx.send(AcpNotification::Peri {
                            session_id,
                            method,
                            params,
                        });
                    } else {
                        let _ = notification_tx.send(AcpNotification::Other {
                            msg: format!("notification: {method}"),
                        });
                    }
                }
                Some(IncomingMessage::Request { id, method, params }) => {
                    if method == "session/request_permission" {
                        let _ =
                            notification_tx.send(AcpNotification::RequestPermission { id, params });
                    } else if method == "elicitation/create" {
                        let _ = notification_tx.send(AcpNotification::Elicitation { id, params });
                    } else {
                        let _ = notification_tx.send(AcpNotification::Other {
                            msg: format!("request: {method}"),
                        });
                    }
                }
                Some(IncomingMessage::Response { .. }) => {}
                None => {
                    debug!("ACP client pump: transport closed, exiting");
                    break;
                }
            }
        }
    }

    // ── High-level RPC wrappers ──

    /// Create a new agent session.
    pub async fn new_session(&self, cwd: &str, model: Option<&str>) -> Result<String, String> {
        let params = json!({ "cwd": cwd, "model": model });
        let result = self
            .transport
            .send_request("session/new", params)
            .await
            .map_err(|e| e.to_string())?;
        // ACP protocol uses camelCase: {"sessionId": "..."}
        let session_id = result
            .get("sessionId")
            .or_else(|| result.get("session_id"))
            .and_then(|v| v.as_str())
            .ok_or("no session_id in response")?
            .to_string();
        *self.current_session_id.lock().unwrap() = Some(session_id.clone());
        Ok(session_id)
    }

    /// Submit a user message to the current session.
    /// Note: prompt() is called from the spawned async task that already
    /// has a session via new_session(), so current_session_id is guaranteed Some.
    pub async fn prompt(&self, text: &str) -> Result<(), String> {
        let session_id = self
            .current_session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no active session")?;
        let params = json!({
            "sessionId": session_id,
            "message": { "role": "user", "content": text },
        });
        self.transport
            .send_request("session/prompt", params)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Change the model for the current session.
    pub async fn set_model(&self, alias: &str) -> Result<(), String> {
        let session_id = self
            .current_session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no active session")?;
        let params = json!({ "sessionId": session_id, "modelId": alias });
        let _ = self
            .transport
            .send_request("session/set_model", params)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Change the permission mode for the current session.
    pub async fn set_mode(&self, mode: &str) -> Result<(), String> {
        let session_id = self
            .current_session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no active session")?;
        let params = json!({ "sessionId": session_id, "modeId": mode });
        let _ = self
            .transport
            .send_request("session/set_mode", params)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Change the thinking config (effort + enabled) for the current session.
    pub async fn set_thinking(&self, effort: &str, enabled: bool) -> Result<(), String> {
        let session_id = self
            .current_session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no active session")?;
        let params = json!({ "sessionId": session_id, "effort": effort, "enabled": enabled });
        let _ = self
            .transport
            .send_request("session/set_thinking", params)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Cancel the currently running prompt.
    pub async fn cancel(&self) -> Result<(), String> {
        let session_id = self
            .current_session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no active session")?;
        let params = json!({ "sessionId": session_id });
        self.transport
            .send_notification("$/cancel_request", params)
            .await
            .map_err(|e| e.to_string())
    }

    /// Send a response to a server-initiated request (e.g. HITL approval).
    pub async fn send_response(
        &self,
        id: RequestId,
        result: Result<Value, AcpError>,
    ) -> Result<(), String> {
        self.transport
            .send_response(id, result)
            .await
            .map_err(|e| e.to_string())
    }
}
