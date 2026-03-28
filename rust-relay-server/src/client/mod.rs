use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::protocol::{RelayMessage, WebMessage};
use crate::protocol_types::RelayAgentEvent;

pub type RelayEventRx = mpsc::UnboundedReceiver<WebMessage>;

pub struct RelayClient {
    tx: mpsc::UnboundedSender<String>,
    pub session_id: Arc<tokio::sync::RwLock<Option<String>>>,
    connected: Arc<AtomicBool>,
    /// 后台任务句柄，Drop 时 abort 避免清理输出
    _tasks: Vec<JoinHandle<()>>,
    /// 序列号计数器（每次发送事件时递增）
    seq: Arc<AtomicU64>,
    /// 历史缓存（seq, json），最多 1000 条
    history: Arc<Mutex<VecDeque<(u64, String)>>>,
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
        user_id: &str,
    ) -> anyhow::Result<(Self, RelayEventRx)> {
        let mut ws_url = format!("{}/agent/ws?token={}&user_id={}", url, token, user_id);
        if let Some(n) = name {
            ws_url.push_str(&format!("&name={}", n));
        }

        const CONNECT_TIMEOUT_SECS: u64 = 10;
        let (ws_stream, _) = tokio::time::timeout(
            std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
            tokio_tungstenite::connect_async(&ws_url),
        )
        .await
        .map_err(|_| anyhow::anyhow!("WebSocket 连接超时（{}s）：{}", CONNECT_TIMEOUT_SECS, url))??;
        let (ws_write, mut ws_read) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<WebMessage>();

        let session_id = Arc::new(tokio::sync::RwLock::new(None::<String>));
        let session_id_clone = session_id.clone();
        let connected = Arc::new(AtomicBool::new(true));

        // Parse first message to get session_id
        if let Some(Ok(Message::Text(text))) = ws_read.next().await {
            if let Ok(RelayMessage::SessionId { session_id: sid }) =
                serde_json::from_str::<RelayMessage>(&text)
            {
                *session_id_clone.write().await = Some(sid.clone());
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
                                if let Ok(pong) = serde_json::to_string(&WebMessage::Pong) {
                                    let _ = write_tx_pong.send(pong);
                                }
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
                seq: Arc::new(AtomicU64::new(1)),
                history: Arc::new(Mutex::new(VecDeque::new())),
            },
            event_rx,
        ))
    }

    /// 扁平化发送：注入 seq 并缓存，然后通过 WS 发送
    fn send_with_seq(&self, mut val: serde_json::Value) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        if let Some(obj) = val.as_object_mut() {
            obj.insert("seq".to_string(), seq.into());
        }
        let json = match serde_json::to_string(&val) {
            Ok(j) => j,
            Err(_) => return,
        };
        // 缓存（最多 1000 条，单条限 512KB，超大条目跳过缓存但仍发送）
        const MAX_HISTORY_ENTRY_BYTES: usize = 512 * 1024;
        if let Ok(mut hist) = self.history.lock() {
            if json.len() <= MAX_HISTORY_ENTRY_BYTES {
                if hist.len() >= 1000 {
                    hist.pop_front();
                }
                hist.push_back((seq, json.clone()));
            } else {
                tracing::debug!(
                    seq,
                    bytes = json.len(),
                    limit = MAX_HISTORY_ENTRY_BYTES,
                    "relay history: entry exceeds size limit, skipping cache (message still sent)"
                );
            }
        }
        let _ = self.tx.send(json);
    }

    /// 发送 agent 事件到 relay（扁平化 + seq），断线后静默跳过
    pub fn send_agent_event(&self, event: &RelayAgentEvent) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(val) = serde_json::to_value(event) {
            self.send_with_seq(val);
        }
    }

    /// 发送带 seq 的 JSON Value 到 relay，断线后静默跳过
    /// 用于 ApprovalNeeded / AskUserBatch / TodoUpdate 等需要 seq + 缓存的消息
    pub fn send_value(&self, val: serde_json::Value) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        self.send_with_seq(val);
    }

    /// 发送消息 JSON Value 到 relay（+ seq），序列化由调用方负责
    pub fn send_message(&self, val: &serde_json::Value) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        self.send_with_seq(val.clone());
    }

    /// 发送原始 JSON 字符串到 relay（不注入 seq，不缓存），断线后静默跳过
    /// 用于 sync_response 等协议消息
    pub fn send_raw(&self, msg: &str) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        let _ = self.tx.send(msg.to_string());
    }

    /// 清空历史缓存（ClearThread 时调用）
    pub fn clear_history(&self) {
        if let Ok(mut hist) = self.history.lock() {
            hist.clear();
        }
    }

    /// 获取 seq > since_seq 的历史事件 JSON 列表
    pub fn get_history_since(&self, since_seq: u64) -> Vec<String> {
        match self.history.lock() {
            Ok(hist) => hist
                .iter()
                .filter(|(seq, _)| *seq > since_seq)
                .map(|(_, json)| json.clone())
                .collect(),
            Err(_) => vec![],
        }
    }

    /// 发送 ThreadReset 到 Web 前端（携带当前 thread 所有消息 JSON），序列化由调用方负责
    /// 先清空历史缓存，再用 send_with_seq 发送并缓存——保证重连后 SyncRequest 能恢复正确状态
    pub fn send_thread_reset(&self, messages: &[serde_json::Value]) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        // 先清空旧历史，避免重连后回放已作废的消息
        self.clear_history();
        let json = serde_json::json!({ "type": "thread_reset", "messages": messages });
        self.send_with_seq(json);
    }
}
