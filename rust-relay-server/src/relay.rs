use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::protocol::{AgentInfo, BroadcastMessage, RelayMessage};

pub struct SessionEntry {
    pub agent_tx: mpsc::UnboundedSender<String>,
    pub web_txs: RwLock<Vec<mpsc::UnboundedSender<String>>>,
    pub name: Option<String>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_active: RwLock<Instant>,
    pub agent_connected: RwLock<bool>,
}

pub struct RelayState {
    pub sessions: DashMap<String, Arc<SessionEntry>>,
    pub broadcast_txs: RwLock<Vec<mpsc::UnboundedSender<String>>>,
    pub token: String,
}

impl RelayState {
    pub fn new(token: String) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            broadcast_txs: RwLock::new(Vec::new()),
            token,
        })
    }

    pub fn agents_list(&self) -> Vec<AgentInfo> {
        self.sessions
            .iter()
            .map(|entry| AgentInfo {
                session_id: entry.key().clone(),
                name: entry.value().name.clone(),
                connected_at: entry.value().connected_at.to_rfc3339(),
            })
            .collect()
    }

    pub async fn broadcast(&self, msg: &BroadcastMessage) {
        let json = match serde_json::to_string(msg) {
            Ok(j) => j,
            Err(_) => return,
        };
        let txs = self.broadcast_txs.read().await;
        for tx in txs.iter() {
            let _ = tx.send(json.clone());
        }
    }

    pub async fn forward_to_web(&self, session_id: &str, msg: &str) {
        if let Some(entry) = self.sessions.get(session_id) {
            let txs = entry.web_txs.read().await;
            for tx in txs.iter() {
                let _ = tx.send(msg.to_string());
            }
        }
    }
}

pub async fn handle_agent_ws(
    ws: WebSocket,
    state: Arc<RelayState>,
    name: Option<String>,
) {
    use futures_util::{SinkExt, StreamExt};

    let (mut ws_tx, mut ws_rx) = ws.split();

    let session_id = uuid::Uuid::new_v4().to_string();
    let (agent_tx, mut agent_rx) = mpsc::unbounded_channel::<String>();

    let entry = Arc::new(SessionEntry {
        agent_tx,
        web_txs: RwLock::new(Vec::new()),
        name: name.clone(),
        connected_at: chrono::Utc::now(),
        last_active: RwLock::new(Instant::now()),
        agent_connected: RwLock::new(true),
    });

    state.sessions.insert(session_id.clone(), entry.clone());

    // Send session_id to agent
    let session_msg = RelayMessage::SessionId {
        session_id: session_id.clone(),
    };
    let _ = ws_tx
        .send(Message::Text(serde_json::to_string(&session_msg).unwrap().into()))
        .await;

    // Broadcast agent_online
    state
        .broadcast(&BroadcastMessage::AgentOnline {
            session_id: session_id.clone(),
            name: name.clone(),
            connected_at: entry.connected_at.to_rfc3339(),
        })
        .await;

    tracing::info!("Agent connected: session={}, name={:?}", session_id, name);

    let sid = session_id.clone();
    let state2 = state.clone();

    // Task: forward from agent_rx to ws_tx
    let send_task = tokio::spawn(async move {
        while let Some(msg) = agent_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read from ws_rx and forward to web_txs
    let state3 = state.clone();
    let sid2 = session_id.clone();
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                // Update last_active
                *entry.last_active.write().await = Instant::now();
                // Forward agent messages to all web clients for this session
                state3.forward_to_web(&sid2, &text).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Agent disconnected
    tracing::info!("Agent disconnected: session={}", sid);
    *entry.agent_connected.write().await = false;
    send_task.abort();

    // Broadcast agent_offline
    state2
        .broadcast(&BroadcastMessage::AgentOffline {
            session_id: sid.clone(),
        })
        .await;

    // Schedule delayed cleanup (30 minutes)
    let state_cleanup = state2.clone();
    let sid_cleanup = sid.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
        if let Some(entry) = state_cleanup.sessions.get(&sid_cleanup) {
            if !*entry.agent_connected.read().await {
                state_cleanup.sessions.remove(&sid_cleanup);
                tracing::info!("Session cleaned up after timeout: {}", sid_cleanup);
            }
        }
    });
}

pub async fn handle_web_management_ws(
    ws: WebSocket,
    state: Arc<RelayState>,
) {
    use futures_util::{SinkExt, StreamExt};

    let (mut ws_tx, mut ws_rx) = ws.split();

    // Send current agents list
    let agents_msg = BroadcastMessage::AgentsList {
        agents: state.agents_list(),
    };
    let _ = ws_tx
        .send(Message::Text(
            serde_json::to_string(&agents_msg).unwrap().into(),
        ))
        .await;

    // Register broadcast channel
    let (broadcast_tx, mut broadcast_rx) = mpsc::unbounded_channel::<String>();
    {
        let mut txs = state.broadcast_txs.write().await;
        txs.push(broadcast_tx.clone());
    }

    // Forward broadcasts to this web client
    let send_task = tokio::spawn(async move {
        while let Some(msg) = broadcast_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read pong / keep alive
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();

    // Remove from broadcast_txs
    let mut txs = state.broadcast_txs.write().await;
    txs.retain(|tx| !tx.is_closed());
}

pub async fn handle_web_session_ws(
    ws: WebSocket,
    state: Arc<RelayState>,
    session_id: String,
) {
    use futures_util::{SinkExt, StreamExt};

    let entry = match state.sessions.get(&session_id) {
        Some(e) => e.clone(),
        None => {
            let (mut ws_tx, _) = ws.split();
            let err = crate::protocol::RelayError::Error {
                code: "session_not_found".into(),
                message: format!("Session {} not found", session_id),
            };
            let _ = ws_tx
                .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                .await;
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = ws.split();

    // Register web_tx for this session
    let (web_tx, mut web_rx) = mpsc::unbounded_channel::<String>();
    {
        let mut txs = entry.web_txs.write().await;
        txs.push(web_tx.clone());
    }

    // Forward agent events to this web client
    let send_task = tokio::spawn(async move {
        while let Some(msg) = web_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read web messages and forward to agent
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                let _ = entry.agent_tx.send(text.to_string());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();

    // Remove from web_txs
    let mut txs = entry.web_txs.write().await;
    txs.retain(|tx| !tx.is_closed());
}

pub fn spawn_session_cleanup(state: Arc<RelayState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5 * 60)).await;
            let mut to_remove = Vec::new();
            for entry in state.sessions.iter() {
                let connected = *entry.value().agent_connected.read().await;
                let last_active = *entry.value().last_active.read().await;
                if !connected && last_active.elapsed() > std::time::Duration::from_secs(30 * 60) {
                    to_remove.push(entry.key().clone());
                }
            }
            for sid in to_remove {
                state.sessions.remove(&sid);
                tracing::info!("Session expired and removed: {}", sid);
            }
        }
    });
}
