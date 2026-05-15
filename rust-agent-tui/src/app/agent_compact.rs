use super::message_pipeline::PipelineAction;
use super::*;
use crate::thread::ThreadMeta;
use rust_create_agent::messages::BaseMessage;

impl App {
    pub(crate) fn handle_compact_done(&mut self, summary: String) -> (bool, bool, bool) {
        // 拆分摘要和重新注入内容
        let (summary_text, re_inject_messages) =
            if let Some(idx) = summary.find("---RE_INJECT_SEPARATOR---\n") {
                let parts: (&str, &str) = summary.split_at(idx);
                let re_inject_part = parts
                    .1
                    .strip_prefix("---RE_INJECT_SEPARATOR---\n")
                    .unwrap_or("");
                // 使用唯一消息分隔符拆分，保留文件内容中的空行
                let re_inject_msgs: Vec<BaseMessage> = re_inject_part
                    .split("\n---RE_INJECT_MSG_BREAK---\n")
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| BaseMessage::system(s.to_string()))
                    .collect();
                (parts.0.trim_end().to_string(), re_inject_msgs)
            } else {
                (summary.clone(), Vec::new())
            };

        let truncated: String = summary_text.chars().take(30).collect();
        let ellipsis = if summary_text.chars().count() > 30 {
            "…"
        } else {
            ""
        };
        let thread_title = format!("Compact: {}{}", truncated, ellipsis);
        let mut meta = ThreadMeta::new(&self.services.cwd);
        meta.title = Some(thread_title);
        let store = self.services.thread_store.clone();
        let new_tid = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.create_thread(meta))
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "compact: 创建新 thread 失败，使用临时 ID");
                    uuid::Uuid::now_v7().to_string()
                })
        });

        // 从 re-inject 消息中提取文件和 skill 信息，生成 condensed summary
        let mut file_entries = Vec::new();
        let mut skill_names = Vec::new();
        for msg in &re_inject_messages {
            let content = msg.content();
            if let Some(rest) = content.strip_prefix("[最近读取的文件: ") {
                let path = rest.lines().next().unwrap_or("");
                let line_count = rest.lines().count().saturating_sub(1);
                file_entries.push(format!("  ⎿  Read {} ({} lines)", path, line_count));
            } else if let Some(rest) = content.strip_prefix("[激活的 Skill 指令: ") {
                let name = rest.lines().next().unwrap_or("");
                skill_names.push(name.to_string());
            }
        }

        let mut new_messages = vec![BaseMessage::system(summary_text.clone())];
        new_messages.extend(re_inject_messages);

        let store = self.services.thread_store.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.append_messages(&new_tid, &new_messages))
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, thread_id = %new_tid, "compact: 持久化新 thread 消息失败");
                });
        });

        self.session_mgr.sessions[self.session_mgr.active].current_thread_id =
            Some(new_tid.clone());
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages = new_messages;

        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .clear();
        let state_msgs = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .clone();
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .restore_completed(state_msgs);

        let mut label_lines = vec![format!("✻ {}", self.services.lc.tr("app-compact-done"))];
        label_lines.extend(file_entries);
        if !skill_names.is_empty() {
            label_lines.push(format!("  ⎿  Skill: {}", skill_names.join(", ")));
        }
        let compact_label = label_lines.join("\n");
        // 清除 ephemeral_notes，防止 compact 前的系统通知（如 CacheWarning、Error）被 saved_notes 机制保留
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .ephemeral_notes
            .clear();
        let view_msgs = vec![MessageViewModel::system(compact_label)];
        self.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: view_msgs,
        });

        self.set_loading(false);
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_rx = None;

        self.session_mgr.sessions[self.session_mgr.active]
            .langfuse
            .langfuse_session = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .auto_compact_failures = 0;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_compact_token_snapshot = None;
        // 清理后台任务残留状态（防御性：auto-compact 现已跳过后台任务运行期，
        // 但手动 /compact 或竞态仍可能在此处遇到非零计数）
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_done_pending_bg = false;
        self.session_mgr.sessions[self.session_mgr.active].background_task_count = 0;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_done_bg_completions
            .clear();

        // Auto-continue: compact 完成后自动用原始输入重新启动 agent
        // 仅在 agent 执行中 auto-compact 时 resubmit（compact_should_resubmit == true），
        // 手动 /compact 和 Done 后 auto-compact 不 resubmit
        // 先读取再清除，防止 flag 泄漏到下次 compact
        let should_resubmit = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .compact_should_resubmit;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .compact_should_resubmit = false;
        // 优先使用 pre_compact_user_input（在 start_compact 时保存，防止被 pending_messages 覆盖）
        let resubmit_input = if should_resubmit {
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .pre_compact_user_input
                .take()
                .or_else(|| {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .last_user_input
                        .clone()
                })
        } else {
            None
        };

        const MAX_AUTO_COMPACT_RESUBMITS: u32 = 3;
        if let Some(original_input) = resubmit_input {
            if self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .auto_compact_resubmit_count
                < MAX_AUTO_COMPACT_RESUBMITS
            {
                let new_count = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .auto_compact_resubmit_count
                    + 1;
                tracing::info!(
                    count = new_count,
                    "auto-compact: re-submitting original user input to continue agent"
                );
                self.submit_message(original_input);
                // submit_message 会重置计数器为 0，恢复为递增后的值
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .auto_compact_resubmit_count = new_count;
            } else {
                tracing::warn!(
                    "auto-compact: reached max re-submit count ({}), stopping",
                    MAX_AUTO_COMPACT_RESUBMITS
                );
                let vm = MessageViewModel::system(self.services.lc.tr("app-compact-limit-reached"));
                self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                // 未 resubmit 时处理待发消息
                if !self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pending_messages
                    .is_empty()
                {
                    self.flush_pending_messages();
                }
            }
        } else {
            tracing::debug!(
                "compact: skipping auto-resubmit (should_resubmit=false or no user input)"
            );
            // 未 resubmit 时处理待发消息
            if !self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pending_messages
                .is_empty()
            {
                self.flush_pending_messages();
            }
        }

        (true, false, true)
    }

    /// CompactError 事件处理器：显示错误、恢复 token snapshot
    pub(crate) fn handle_compact_error(&mut self, msg: String) -> (bool, bool, bool) {
        let vm = MessageViewModel::system(
            self.services
                .lc
                .tr_args("app-compact-failed", &[("error".into(), msg.into())]),
        );
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        self.set_loading(false);
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_rx = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .auto_compact_failures += 1;

        // 恢复 compact 前的 token tracker 快照，使 auto-compact 仍能感知上下文大小
        if let Some(snapshot) = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_compact_token_snapshot
            .take()
        {
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .session_token_tracker = snapshot;
        }

        if !self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pending_messages
            .is_empty()
        {
            self.flush_pending_messages();
        }

        (true, false, true)
    }

    /// 执行 micro-compact：清除旧工具结果，不调用 LLM
    pub fn start_micro_compact(&mut self) {
        use rust_create_agent::agent::micro_compact_enhanced;
        let config = self.get_compact_config();
        let cleared = micro_compact_enhanced(
            &config,
            &mut self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_state_messages,
        );
        if cleared > 0 {
            tracing::info!(cleared, "micro-compact: enhanced compact completed");
            // 同步 pipeline.completed 与 agent_state_messages
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .clear();
            let state_msgs = self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_state_messages
                .clone();
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .restore_completed(state_msgs);
            let vm = MessageViewModel::system(self.services.lc.tr_args(
                "app-compact-auto-cleared",
                &[("count".into(), (cleared as i64).into())],
            ));
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        }
    }
}
