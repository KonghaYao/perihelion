use super::*;

impl App {
    /// 获取或新建当前 thread id（同步，block_in_place）
    pub(super) fn ensure_thread_id(&mut self) -> ThreadId {
        if let Some(id) = &self.current_thread_id {
            return id.clone();
        }
        let meta = ThreadMeta::new(&self.cwd);
        let store = self.thread_store.clone();
        let id = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.create_thread(meta))
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "创建 thread 失败，使用临时 ID（消息将无法持久化）");
                    uuid::Uuid::now_v7().to_string()
                })
        });
        self.current_thread_id = Some(id.clone());
        id
    }

    pub fn scroll_up(&mut self) {
        self.core.scroll_offset = self.core.scroll_offset.saturating_sub(3);
        self.core.scroll_follow = false;
    }

    pub fn scroll_down(&mut self) {
        self.core.scroll_offset = self.core.scroll_offset.saturating_add(3);
        self.core.scroll_follow = false;
    }

    /// 展开/折叠所有工具调用消息
    pub fn toggle_collapsed_messages(&mut self) {
        self.core.show_tool_messages = !self.core.show_tool_messages;
        let _ = self
            .core.render_tx
            .send(RenderEvent::ToggleToolMessages(self.core.show_tool_messages));
    }

    /// 添加一个图片附件到待发送列表
    pub fn add_pending_attachment(&mut self, att: PendingAttachment) {
        self.core.pending_attachments.push(att);
    }

    /// 删除最后一个图片附件
    pub fn pop_pending_attachment(&mut self) {
        self.core.pending_attachments.pop();
    }

    // ─── Thread 操作 ──────────────────────────────────────────────────────────

    /// 恢复历史 thread：加载消息，关闭 browser
    pub fn open_thread(&mut self, thread_id: ThreadId) {
        let store = self.thread_store.clone();
        let tid = thread_id.clone();
        let base_msgs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_messages(&tid))
                .unwrap_or_default()
        });
        self.core.view_messages.clear();
        self.agent.agent_state_messages = base_msgs.clone();
        // 维护前一条 Ai 消息的 tool_calls，用于 Tool 消息获取工具名和参数
        let mut prev_ai_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
        for msg in &base_msgs {
            // 先收集 Ai 消息的 tool_calls
            if let BaseMessage::Ai { tool_calls, .. } = msg {
                prev_ai_tool_calls = tool_calls
                    .iter()
                    .map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                    .collect();
            }
            let vm = MessageViewModel::from_base_message(msg, &prev_ai_tool_calls);
            // 跳过没有可见文本内容的 AssistantBubble（纯 ToolUse 或空文本 + ToolUse）
            if let MessageViewModel::AssistantBubble { blocks, .. } = &vm {
                let has_visible = blocks.iter().any(|b| match b {
                    ContentBlockView::Text { raw, .. } => !raw.trim().is_empty(),
                    ContentBlockView::Reasoning { char_count } => *char_count > 0,
                    ContentBlockView::ToolUse { .. } => false,
                });
                if !has_visible {
                    continue;
                }
            }
            self.core.view_messages.push(vm);
        }
        self.current_thread_id = Some(thread_id);
        self.core.thread_browser = None;
        self.langfuse.langfuse_session = None;

        // 通知 Relay Web 前端：thread 已切换，推送完整历史消息
        if let Some(ref relay) = self.relay.relay_client {
            let msg_vals: Vec<serde_json::Value> = base_msgs
                .iter()
                .filter_map(|m| serde_json::to_value(m).ok())
                .collect();
            relay.send_thread_reset(&msg_vals);
        }

        // 通知渲染线程加载历史消息
        let _ = self
            .core.render_tx
            .send(RenderEvent::LoadHistory(self.core.view_messages.clone()));
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        self.core.view_messages.clear();
        self.agent.agent_state_messages.clear();
        self.current_thread_id = None;
        self.todo_items.clear();
        self.core.pending_attachments.clear();
        self.core.thread_browser = None;
        self.langfuse.langfuse_session = None;
        let _ = self.core.render_tx.send(RenderEvent::Clear);
        if let Some(ref relay) = self.relay.relay_client {
            relay.send_thread_reset(&[]);
        }
    }

    /// 压缩当前对话上下文：调用 LLM 生成摘要，替换 agent_state_messages
    pub fn start_compact(&mut self, instructions: String) {
        if self.agent.agent_state_messages.is_empty() {
            let vm = MessageViewModel::system("无可压缩的上下文（历史消息为空）".to_string());
            self.core.view_messages.push(vm.clone());
            let _ = self
                .core.render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
            return;
        }

        let provider = match self
            .zen_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config)
            .or_else(agent::LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                let vm = MessageViewModel::system(
                    "❌ 压缩失败: 未配置 LLM Provider（请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY）".to_string(),
                );
                self.core.view_messages.push(vm.clone());
                let _ = self
                    .core.render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                return;
            }
        };

        let messages = self.agent.agent_state_messages.clone();
        let model = provider.into_model();

        let (tx, rx) = mpsc::channel::<AgentEvent>(8);
        self.agent.agent_rx = Some(rx);
        self.set_loading(true);

        tokio::spawn(async move {
            agent::compact_task(messages, model, instructions, tx).await;
        });
    }

    /// 打开 thread 浏览面板（通过命令触发）
    pub fn open_thread_browser(&mut self) {
        let store = self.thread_store.clone();
        let threads = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.list_threads())
                .unwrap_or_default()
        });
        self.core.thread_browser = Some(ThreadBrowser::new(threads, self.thread_store.clone()));
    }
}
