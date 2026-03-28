use super::*;

impl App {
    /// 检查是否需要重连 Relay，如果计时器到期则尝试重连
    pub async fn check_relay_reconnect(&mut self) {
        let due = self
            .relay_reconnect_at
            .map(|t| t <= std::time::Instant::now())
            .unwrap_or(false);
        if !due {
            return;
        }
        // 取消计时器，避免重入
        self.relay_reconnect_at = None;
        // 已连接时不重连
        if self.relay_client.is_some() {
            return;
        }
        let Some((url, token, name)) = self.relay_params.clone() else {
            return;
        };
        use rust_create_agent::messages::BaseMessage;
        use crate::ui::render_thread::RenderEvent;
        use crate::app::MessageViewModel;
        match rust_relay_server::client::RelayClient::connect(&url, &token, name.as_deref()).await {
            Ok((client, event_rx)) => {
                let sid = client.session_id.read().await.clone().unwrap_or_default();
                let status_msg = format!("Relay reconnected (session: {})", &sid[..8.min(sid.len())]);
                let vm = MessageViewModel::from_base_message(&BaseMessage::system(status_msg), &[]);
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                self.relay_client = Some(Arc::new(client));
                self.relay_event_rx = Some(event_rx);
            }
            Err(_) => {
                // 重连失败，3 秒后再试（静默，不重复打印错误）
                self.relay_reconnect_at = Some(
                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                );
            }
        }
    }

    /// 每帧调用：消费 Relay 事件（Web 端发来的控制消息）
    pub fn poll_relay(&mut self) -> bool {
        use rust_relay_server::protocol::WebMessage;

        // 先收集所有待处理事件（避免借用冲突）
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = self.relay_event_rx.as_mut() {
            loop {
                match rx.try_recv() {
                    Ok(msg) => events.push(msg),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        } else {
            return false;
        }

        if disconnected {
            // 不用 tracing，通过 TUI 消息显示
            self.relay_event_rx = None;
            self.relay_client = None;
            let vm = MessageViewModel::from_base_message(
                &BaseMessage::system("Relay disconnected, reconnecting in 3s..."),
                &[],
            );
            let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
            // 有缓存参数时，3 秒后自动重连
            if self.relay_params.is_some() {
                self.relay_reconnect_at = Some(
                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                );
            }
        }

        if events.is_empty() {
            return false;
        }

        for web_msg in events {
            match web_msg {
                WebMessage::UserInput { text } => {
                    // 不在此处发送 user 消息到 relay，由 executor 中的 MessageAdded 事件统一发送
                    if self.loading {
                        self.pending_messages.push(text);
                    } else {
                        self.submit_message(text);
                    }
                }
                WebMessage::HitlDecision { decisions } => {
                    if let Some(InteractionPrompt::Approval(prompt)) = self.interaction_prompt.take() {
                        // 远程控制支持全部 4 种 HITL 决策：Approve / Edit / Reject / Respond
                        let hitl_decisions: Vec<HitlDecision> = decisions
                            .iter()
                            .map(|d| match d.decision.as_str() {
                                "Approve" => HitlDecision::Approve,
                                "Edit" => {
                                    let new_input = d
                                        .input
                                        .as_deref()
                                        .and_then(|s| serde_json::from_str(s).ok())
                                        .unwrap_or(serde_json::json!({}));
                                    HitlDecision::Edit(new_input)
                                }
                                "Respond" => {
                                    HitlDecision::Respond(d.input.clone().unwrap_or_default())
                                }
                                _ => HitlDecision::Reject,
                            })
                            .collect();
                        let _ = prompt.response_tx.send(hitl_decisions);
                    }
                }
                WebMessage::AskUserResponse { answers } => {
                    if let Some(InteractionPrompt::Questions(prompt)) = self.interaction_prompt.as_mut() {
                        for (q_text, answer) in &answers {
                            if let Some(q) = prompt
                                .questions
                                .iter_mut()
                                .find(|q| q.data.tool_call_id == *q_text)
                            {
                                let answer_str = match answer {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Array(arr) => arr
                                        .iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    other => other.to_string(),
                                };
                                q.custom_input = answer_str;
                                q.in_custom_input = true;
                            }
                        }
                        for c in prompt.confirmed.iter_mut() {
                            *c = true;
                        }
                    }
                    self.ask_user_confirm();
                }
                WebMessage::CancelAgent => {
                    self.interrupt();
                    self.interaction_prompt = None;
                }
                WebMessage::ClearThread => {
                    if let Some(ref relay) = self.relay_client {
                        relay.send_thread_reset(&[]);
                    }
                    self.new_thread();
                }
                WebMessage::CompactThread => {
                    self.start_compact(String::new());
                }
                WebMessage::Pong => {}
                WebMessage::SyncRequest { since_seq } => {
                    if let Some(ref relay) = self.relay_client {
                        let events = relay.get_history_since(since_seq);
                        let response = serde_json::json!({
                            "type": "sync_response",
                            "events": events.iter()
                                .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap_or_default())
                                .collect::<Vec<_>>()
                        });
                        if let Ok(json) = serde_json::to_string(&response) {
                            relay.send_raw(&json);
                        }
                    }
                }
            }
        }
        true
    }
}
