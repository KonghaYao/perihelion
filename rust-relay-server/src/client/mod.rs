use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::protocol::{RelayMessage, WebMessage};

pub type RelayEventRx = mpsc::UnboundedReceiver<WebMessage>;

pub struct RelayClient {
    tx: mpsc::UnboundedSender<String>,
    pub session_id: Arc<tokio::sync::RwLock<Option<String>>>,
    connected: Arc<AtomicBool>,
    /// 后台任务句柄，Drop 时 abort 避免清理输出
    _tasks: Vec<JoinHandle<()>>,
}

impl Drop for RelayClient {
    fn drop(&mut self) {
        // 强制终止后台 WS 任务，防止 tungstenite Drop 时发送 close frame 产生输出
        for handle in &self._tasks {
            handle.abort();
        }
    }
}

impl RelayClient {
    pub async fn connect(
        url: &str,
        token: &str,
        name: Option<&str>,
    ) -> anyhow::Result<(Self, RelayEventRx)> {
        let mut ws_url = format!("{}/agent/ws?token={}", url, token);
        if let Some(n) = name {
            ws_url.push_str(&format!("&name={}", n));
        }

        let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await?;
        let (ws_write, mut ws_read) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<WebMessage>();

        let session_id = Arc::new(tokio::sync::RwLock::new(None::<String>));
        let session_id_clone = session_id.clone();
        let connected = Arc::new(AtomicBool::new(true));

        // Parse first message to get session_id
        if let Some(Ok(msg)) = ws_read.next().await {
            if let Message::Text(text) = msg {
                if let Ok(relay_msg) = serde_json::from_str::<RelayMessage>(&text) {
                    if let RelayMessage::SessionId { session_id: sid } = relay_msg {
                        *session_id_clone.write().await = Some(sid.clone());
                    }
                }
            }
        }

        // Spawn write task
        let ws_write = Arc::new(tokio::sync::Mutex::new(ws_write));
        let ws_write_clone = ws_write.clone();
        let connected_write = connected.clone();
        let write_handle = tokio::spawn(async move {
            while let Some(msg) = write_rx.recv().await {
                let mut w = ws_write_clone.lock().await;
                if w.send(Message::Text(msg.into())).await.is_err() {
                    connected_write.store(false, Ordering::Relaxed);
                    write_rx.close();
                    break;
                }
            }
        });

        // Spawn read task
        let write_tx_pong = write_tx.clone();
        let connected_read = connected.clone();
        let read_handle = tokio::spawn(async move {
            loop {
                match ws_read.next().await {
                    Some(Ok(Message::Text(text))) => {
                        let text_str = text.to_string();
                        if let Ok(relay_msg) = serde_json::from_str::<RelayMessage>(&text_str) {
                            if matches!(relay_msg, RelayMessage::Ping) {
                                let pong = serde_json::to_string(&WebMessage::Pong).unwrap();
                                let _ = write_tx_pong.send(pong);
                                continue;
                            }
                        }
                        if let Ok(web_msg) = serde_json::from_str::<WebMessage>(&text_str) {
                            let _ = event_tx.send(web_msg);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            connected_read.store(false, Ordering::Relaxed);
        });

        Ok((
            RelayClient {
                tx: write_tx,
                session_id,
                connected,
                _tasks: vec![write_handle, read_handle],
            },
            event_rx,
        ))
    }

    /// 发送 agent 事件到 relay，断线后静默跳过
    pub fn send_agent_event(&self, event: &rust_create_agent::agent::AgentEvent) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        let msg = RelayMessage::AgentEvent {
            event: event.clone(),
        };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    /// 发送原始 JSON 到 relay，断线后静默跳过
    pub fn send_raw(&self, msg: &str) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        let _ = self.tx.send(msg.to_string());
    }
}
