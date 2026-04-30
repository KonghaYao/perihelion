use super::*;
use rust_create_agent::agent::AgentCancellationToken;

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
        let _ = self.core.render_tx.send(RenderEvent::ToggleToolMessages(
            self.core.show_tool_messages,
        ));
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

    /// 重置 AgentComm 会话状态（token tracker、重试、subagent 等）
    /// 在 open_thread / new_thread 时调用，确保切换 thread 后上下文干净
    fn reset_agent_session(&mut self) {
        self.agent.session_token_tracker.reset();
        self.agent.pre_compact_token_snapshot = None;
        self.agent.needs_auto_compact = false;
        self.agent.auto_compact_failures = 0;
        self.agent.retry_status = None;
        self.agent.subagent_depth = 0;
        self.agent.task_start_time = None;
        self.agent.last_task_duration = None;
        self.agent.agent_id = None;
        self.agent.interaction_prompt = None;
        self.agent.pending_hitl_items = None;
        self.agent.pending_ask_user = None;
        self.agent.cancel_token = None;
        self.agent.agent_rx = None;
    }

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

        // 使用统一管线转换：与流式路径共享同一个 messages_to_view_models()
        self.core.view_messages =
            message_pipeline::MessagePipeline::messages_to_view_models(&base_msgs, &self.cwd);

        // 同步 Pipeline 内部状态，确保后续流式事件能正确续接
        self.core.pipeline.clear();
        self.core.pipeline.restore_completed(base_msgs.clone());

        self.current_thread_id = Some(thread_id);
        self.core.thread_browser = None;
        self.core.pending_attachments.clear();
        self.langfuse.langfuse_session = None;
        self.todo_items.clear();

        self.reset_agent_session();

        // 恢复 sticky header：找到 thread 中最后一条 Human 消息
        self.core.last_human_message = base_msgs
            .iter()
            .filter_map(|m| {
                if let BaseMessage::Human { content, .. } = m {
                    let text = content.text_content();
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                } else {
                    None
                }
            })
            .last();

        // 通知渲染线程加载历史消息
        let _ = self
            .core
            .render_tx
            .send(RenderEvent::LoadHistory(self.core.view_messages.clone()));
    }

    pub fn open_thread_with_feedback(&mut self, thread_id: ThreadId) {
        self.open_thread(thread_id);
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        self.core.view_messages.clear();
        self.agent.agent_state_messages.clear();
        self.core.pipeline.clear();
        self.current_thread_id = None;
        self.todo_items.clear();
        self.core.pending_attachments.clear();
        self.core.thread_browser = None;
        self.langfuse.langfuse_session = None;
        self.core.last_human_message = None;

        self.reset_agent_session();

        let _ = self.core.render_tx.send(RenderEvent::Clear);
    }

    /// 压缩当前对话上下文：调用 LLM 生成摘要，替换 agent_state_messages
    pub fn start_compact(&mut self, instructions: String) {
        if self.agent.agent_state_messages.is_empty() {
            let vm = MessageViewModel::system("无可压缩的上下文（历史消息为空）".to_string());
            self.core.view_messages.push(vm.clone());
            let _ = self
                .core
                .render_tx
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
                    .core
                    .render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                return;
            }
        };

        let messages = self.agent.agent_state_messages.clone();
        let model = provider.into_model();
        let config = self.get_compact_config();
        let cwd = self.cwd.clone();

        let (tx, rx) = mpsc::channel::<AgentEvent>(8);
        self.agent.agent_rx = Some(rx);

        // 创建取消令牌，使 Ctrl+C 可以中断 compact 任务
        let cancel = AgentCancellationToken::new();
        self.agent.cancel_token = Some(cancel.clone());

        self.set_loading(true);
        self.spinner_state.set_verb(Some("压缩上下文"));

        let vm = MessageViewModel::system("正在压缩上下文…".to_string());
        self.core.view_messages.push(vm.clone());
        let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));

        // 保存快照：compact 失败时恢复，防止 tracker 失去对上下文大小的感知
        self.agent.pre_compact_token_snapshot = Some(self.agent.session_token_tracker.clone());
        self.agent.session_token_tracker.reset();

        tokio::spawn(async move {
            agent::compact_task(messages, model, instructions, config, cwd, tx, cancel).await;
        });
    }

    /// 打开 thread 浏览面板（通过命令触发）
    pub fn open_thread_browser(&mut self) {
        let store = self.thread_store.clone();
        let cwd = self.cwd.clone();
        let threads = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.list_threads())
                .unwrap_or_default()
        });
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        self.core.thread_browser = Some(ThreadBrowser::new(filtered, self.thread_store.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_thread(cwd: &str) -> ThreadMeta {
        ThreadMeta::new(cwd)
    }

    #[test]
    fn filter_keeps_matching_cwd() {
        let cwd = "/Users/alice/project";
        let threads = vec![
            make_thread(cwd),
            make_thread("/Users/alice/other"),
            make_thread(cwd),
        ];
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_returns_empty_when_no_match() {
        let cwd = "/Users/alice/project";
        let threads = vec![
            make_thread("/Users/alice/other"),
            make_thread("/Users/bob/project"),
        ];
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_keeps_all_when_all_match() {
        let cwd = "/Users/alice/project";
        let threads = vec![make_thread(cwd), make_thread(cwd), make_thread(cwd)];
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        assert_eq!(filtered.len(), 3);
    }
}
