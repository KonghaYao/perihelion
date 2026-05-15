use super::*;
use rust_create_agent::agent::AgentCancellationToken;

impl App {
    /// 获取或新建当前 thread id（同步，block_in_place）
    pub(super) fn ensure_thread_id(&mut self) -> ThreadId {
        if let Some(id) = &self.session_mgr.sessions[self.session_mgr.active].current_thread_id {
            return id.clone();
        }
        let meta = ThreadMeta::new(&self.services.cwd);
        let store = self.services.thread_store.clone();
        let id = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.create_thread(meta))
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "创建 thread 失败，使用临时 ID（消息将无法持久化）");
                    uuid::Uuid::now_v7().to_string()
                })
        });
        self.session_mgr.sessions[self.session_mgr.active].current_thread_id = Some(id.clone());
        id
    }

    pub fn scroll_up(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_offset = self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_offset
            .saturating_sub(3);
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_follow = false;
    }

    pub fn scroll_down(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_offset = self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_offset
            .saturating_add(3);
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_follow = false;
    }

    /// 展开/折叠所有工具调用消息
    pub fn toggle_collapsed_messages(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .show_tool_messages = !self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .show_tool_messages;
        let _ = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .render_tx
            .send(RenderEvent::ToggleToolMessages(
                self.session_mgr.sessions[self.session_mgr.active]
                    .ui
                    .show_tool_messages,
            ));
    }

    /// 添加一个图片附件到待发送列表
    pub fn add_pending_attachment(&mut self, att: PendingAttachment) {
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .pending_attachments
            .push(att);
    }

    /// 删除最后一个图片附件
    pub fn pop_pending_attachment(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .pending_attachments
            .pop();
    }

    // ─── Thread 操作 ──────────────────────────────────────────────────────────

    /// 重置 AgentComm 会话状态（token tracker、重试、subagent 等）
    /// 在 open_thread / new_thread 时调用，确保切换 thread 后上下文干净
    fn reset_agent_session(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .session_token_tracker
            .reset();
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_compact_token_snapshot = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .needs_auto_compact = false;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .auto_compact_failures = 0;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .compact_should_resubmit = false;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .retry_status = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .subagent_depth = 0;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .task_start_time = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .last_task_duration = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_id = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .interaction_prompt = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pending_hitl_items = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pending_ask_user = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .cancel_token = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_rx = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .last_submitted_text = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .spinner_state
            .reset();
    }

    /// 恢复历史 thread：加载消息，关闭 browser
    pub fn open_thread(&mut self, thread_id: ThreadId) {
        let store = self.services.thread_store.clone();
        let tid = thread_id.clone();
        let base_msgs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_messages(&tid))
                .unwrap_or_default()
        });
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .ephemeral_notes
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages = base_msgs.clone();

        // 使用统一管线转换：与流式路径共享同一个 messages_to_view_models()
        let mut view_msgs = message_pipeline::MessagePipeline::messages_to_view_models(
            &base_msgs,
            &self.services.cwd,
        );
        // 历史恢复时聚合连续的已完成 SubAgentGroup 为批次汇总
        message_pipeline::aggregate_batch_groups(&mut view_msgs);
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages = view_msgs;

        // 同步 Pipeline 内部状态，确保后续流式事件能正确续接
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .restore_completed(base_msgs.clone());

        self.session_mgr.sessions[self.session_mgr.active].current_thread_id = Some(thread_id);
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::ThreadBrowser);
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .pending_attachments
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .langfuse
            .langfuse_session = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .todo_items
            .clear();

        self.reset_agent_session();

        // 恢复 sticky header：找到 thread 中最后一条 Human 消息
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .last_human_message = base_msgs
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
            .next_back();

        // 通知渲染线程加载历史消息
        let _ = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .render_tx
            .send(RenderEvent::Rebuild(
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .clone(),
            ));
    }

    pub fn open_thread_with_feedback(&mut self, thread_id: ThreadId) {
        self.open_thread(thread_id);
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        // Fire SessionEnd hooks before clearing session state
        {
            let mut hooks = self
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_hooks.clone())
                .unwrap_or_default();
            hooks.extend(
                rust_agent_middlewares::hooks::loader::load_settings_local_hooks(
                    &self.services.cwd,
                ),
            );
            if !hooks.is_empty() {
                let cwd = self.services.cwd.clone();
                let provider_name = self.services.provider_name.clone();
                tokio::spawn(async move {
                    rust_agent_middlewares::hooks::middleware::fire_standalone_lifecycle_hooks(
                        &hooks,
                        rust_agent_middlewares::hooks::types::HookEvent::SessionEnd,
                        &cwd,
                        "",
                        "",
                        &provider_name,
                        None,
                    )
                    .await;
                });
            }
        }

        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .ephemeral_notes
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .clear();
        self.session_mgr.sessions[self.session_mgr.active].current_thread_id = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .todo_items
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .pending_attachments
            .clear();
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::ThreadBrowser);
        self.session_mgr.sessions[self.session_mgr.active]
            .langfuse
            .langfuse_session = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .last_human_message = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .last_submitted_text = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .pre_submit_state_len = 0;

        self.reset_agent_session();

        let _ = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .render_tx
            .send(RenderEvent::Clear);
    }

    /// 压缩当前对话上下文：调用 LLM 生成摘要，替换 agent_state_messages
    pub fn start_compact(&mut self, instructions: String) {
        if self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .is_empty()
        {
            self.push_system_note(self.services.lc.tr("app-compact-no-context"));
            self.render_rebuild();
            return;
        }

        let provider = match self
            .services
            .peri_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config)
            .or_else(agent::LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                self.push_system_note(self.services.lc.tr("app-compact-no-provider"));
                self.render_rebuild();
                return;
            }
        };

        let messages = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .clone();
        let model = provider.into_model();
        let config = self.get_compact_config();
        let cwd = self.services.cwd.clone();

        let (tx, rx) = mpsc::channel::<AgentEvent>(64);
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_rx = Some(rx);

        // 创建取消令牌，使 Ctrl+C 可以中断 compact 任务
        let cancel = AgentCancellationToken::new();
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .cancel_token = Some(cancel.clone());

        self.set_loading(true);
        self.session_mgr.sessions[self.session_mgr.active]
            .spinner_state
            .set_verb(Some(self.services.lc.tr("app-compact-compressing").leak()));

        self.render_rebuild();

        // 保存用户输入副本：compact 后 resubmit 用，防止 last_user_input 被 pending_messages 覆盖
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_compact_user_input = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .last_user_input
            .clone();

        // 标记 compact 完成后是否应自动 resubmit：
        // 仅 agent 执行中的 auto-compact 应 resubmit，
        // 手动 /compact（instructions != "auto"）和 Done 后 auto-compact 不应 resubmit
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .compact_should_resubmit = instructions == "auto";

        // 保存快照：compact 失败时恢复，防止 tracker 失去对上下文大小的感知
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_compact_token_snapshot = Some(
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .session_token_tracker
                .clone(),
        );
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .session_token_tracker
            .reset();

        // Merge hooks for PreCompact/PostCompact dispatch
        let mut compact_hooks: Vec<rust_agent_middlewares::hooks::types::RegisteredHook> = self
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_hooks.clone())
            .unwrap_or_default();
        compact_hooks.extend(
            rust_agent_middlewares::hooks::loader::load_settings_local_hooks(&self.services.cwd),
        );

        let session_id = self.session_mgr.sessions[self.session_mgr.active]
            .current_thread_id
            .clone()
            .unwrap_or_default();
        let provider_name = self.services.provider_name.clone();
        // transcript_path not available in App; hooks receive empty string
        let transcript_path = String::new();

        tokio::spawn(async move {
            agent::compact_task(
                messages,
                model,
                instructions,
                config,
                cwd,
                tx,
                cancel,
                compact_hooks,
                session_id,
                transcript_path,
                provider_name,
            )
            .await;
        });
    }

    /// 打开 thread 浏览面板（通过命令触发）
    pub fn open_thread_browser(&mut self) {
        let store = self.services.thread_store.clone();
        let cwd = self.services.cwd.clone();
        let threads = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.list_threads())
                .unwrap_or_default()
        });
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();

        // 检测当前 cwd 的 git 分支
        let branch = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.services.cwd)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());

        let browser = ThreadBrowser::new(filtered, self.services.thread_store.clone(), branch);
        self.open_panel(PanelState::ThreadBrowser(browser));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("thread_ops_test.rs");
}
