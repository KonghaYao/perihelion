use super::message_pipeline::PipelineAction;
use super::*;
use peri_middlewares::hitl::BatchItem;

impl App {
    /// 处理单个 AgentEvent，返回 `(updated, should_break, should_return)`
    pub(crate) fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
        match event {
            AgentEvent::SubAgentStart {
                agent_id,
                task_preview,
                is_background,
            } => {
                if is_background {
                    self.session_mgr.sessions[self.session_mgr.active].background_task_count += 1;
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth += 1;
                // 跨切面：Langfuse
                if let Some(ref tracer) = self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_tracer
                {
                    let _lock_start = std::time::Instant::now();
                    tracer.lock().on_subagent_start(&agent_id, &task_preview);
                    let _wait = _lock_start.elapsed();
                    if _wait.as_millis() > 50 {
                        tracing::warn!(
                            "[DEADLOCK] TUI: tracer lock held {:?} for SubAgentStart({})",
                            _wait,
                            agent_id
                        );
                    }
                }
                // Pipeline：创建 SubAgentGroup VM
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::SubAgentStart {
                        agent_id,
                        task_preview,
                        is_background,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::SubagentLifecycle {
                agent_name,
                started,
            } => {
                if started {
                    // SubAgent 实际开始执行：更新 spinner 为工具使用模式
                    let verb = format!("Agent: {}", agent_name);
                    self.session_mgr.sessions[self.session_mgr.active]
                        .spinner_state
                        .set_mode(peri_widgets::SpinnerMode::ToolUse);
                    self.session_mgr.sessions[self.session_mgr.active]
                        .spinner_state
                        .set_verb(Some(&verb));
                } else {
                    // SubAgent 执行结束：恢复 spinner 为响应模式
                    self.session_mgr.sessions[self.session_mgr.active]
                        .spinner_state
                        .set_mode(peri_widgets::SpinnerMode::Responding);
                    self.session_mgr.sessions[self.session_mgr.active]
                        .spinner_state
                        .set_verb(Some("思考中…"));
                }
                // 触发 rebuild 刷新 SubAgentGroup 卡片显示
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::SubAgentEnd {
                result,
                is_error,
                agent_id,
            } => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth
                    .saturating_sub(1);
                // 跨切面：Langfuse
                if let Some(ref tracer) = self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_tracer
                {
                    let _lock_start = std::time::Instant::now();
                    let agent_id_str = agent_id.as_deref().unwrap_or("?");
                    tracer.lock().on_subagent_end(&result, is_error);
                    let _wait = _lock_start.elapsed();
                    if _wait.as_millis() > 50 {
                        tracing::warn!(
                            "[DEADLOCK] TUI: tracer lock held {:?} for SubAgentEnd({})",
                            _wait,
                            agent_id_str
                        );
                    }
                }
                // 如果所有 SubAgent 已完成，恢复 spinner 到思考模式
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth
                    == 0
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .spinner_state
                        .set_mode(peri_widgets::SpinnerMode::Responding);
                    self.session_mgr.sessions[self.session_mgr.active]
                        .spinner_state
                        .set_verb(Some("思考中…"));
                }
                // Pipeline：更新 SubAgentGroup（is_running=false, final_result）
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::SubAgentEnd {
                        result,
                        is_error,
                        agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::ContextWarning {
                used_tokens: _,
                total_tokens,
                percentage: _,
            } => {
                // 子 Agent 的 ContextWarning 不应触发父 Agent 的 auto-compact
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth
                    > 0
                {
                    return (true, false, false);
                }
                // 从核心层同步 context_window（核心层通过 model.context_window() 获取正确值）
                let cw = total_tokens as u32;
                if cw > 0
                    && self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .context_window
                        != cw
                {
                    tracing::debug!(
                        old = self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .context_window,
                        new = cw,
                        "context_window updated from core layer"
                    );
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .context_window = cw;
                }
                // 核心层上下文警告：触发 auto-compact 标记
                if std::env::var("DISABLE_COMPACT").is_ok() {
                    return (true, false, false);
                }
                let compact_config = self.get_compact_config();
                if !compact_config.auto_compact_enabled {
                    return (true, false, false);
                }
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .auto_compact_failures
                    < compact_config.max_consecutive_failures
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .needs_auto_compact = true;
                }
                (true, false, false)
            }
            AgentEvent::OAuthAuthorizationNeeded {
                server_name,
                authorization_url,
                callback_tx,
            } => self.handle_oauth_needed(server_name, authorization_url, callback_tx),
            AgentEvent::OAuthAuthorizationCompleted { server_name } => {
                self.handle_oauth_completed(server_name)
            }
            AgentEvent::OAuthAuthorizationFailed { server_name, error } => {
                self.handle_oauth_failed(server_name, error)
            }
            AgentEvent::McpActionCompleted {
                server_name,
                action,
                success,
            } => self.handle_mcp_action_completed(server_name, action, success),
            AgentEvent::PluginActionCompleted {
                plugin_id,
                action,
                success,
                message,
            } => self.handle_plugin_action_completed(plugin_id, action, success, message),
            AgentEvent::TokenUsageUpdate {
                usage,
                model: _model,
            } => {
                // SubAgent 的 TokenUsageUpdate 不应污染父 agent 的 tracker
                // （SubAgent 上下文远小于父 agent，会覆盖 last_usage 导致 ctx 突降）
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth
                    > 0
                {
                    return (true, false, false);
                }

                // 累积到会话追踪器
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .session_token_tracker
                    .accumulate(&usage);

                // 缓存率检查：当次命中率低于 80% 时显示黄色提示
                let rate = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .session_token_tracker
                    .cache_hit_rate();
                if rate < 0.8 {
                    let tracker = &self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .session_token_tracker;
                    tracing::warn!(
                        input = tracker.total_input_tokens,
                        cache_read = tracker.total_cache_read_tokens,
                        rate_pct = rate * 100.0,
                        "prompt cache hit rate below threshold"
                    );
                    let percentage = (rate * 100.0) as u32;
                    let req_id = tracker.last_request_id.as_deref().unwrap_or("-");
                    let msg = format!(
                        "⚠ {}",
                        self.services.lc.tr_args(
                            "app-prompt-cache-low",
                            &[
                                ("rate".into(), (percentage as i64).into()),
                                ("req".into(), req_id.to_string().into()),
                            ]
                        )
                    );
                    let vm = MessageViewModel::system(msg);
                    self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                }
                // 更新 spinner 的 token 显示（仅当次调用的 token，不累计）
                let current_tokens = usage.input_tokens as usize + usage.output_tokens as usize;
                self.session_mgr.sessions[self.session_mgr.active]
                    .spinner_state
                    .set_token_count(current_tokens);
                // compact 被完全禁用
                if std::env::var("DISABLE_COMPACT").is_ok() {
                    return (true, false, false);
                }
                // 从 settings.json 获取 CompactConfig
                let compact_config = self.get_compact_config();
                // auto-compact 被禁用
                if !compact_config.auto_compact_enabled {
                    return (true, false, false);
                }
                // circuit breaker: 连续失败达到上限后不再自动触发
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .auto_compact_failures
                    < compact_config.max_consecutive_failures
                {
                    let budget = peri_agent::agent::token::ContextBudget::new(
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .context_window,
                    )
                    .with_auto_compact_threshold(compact_config.auto_compact_threshold);
                    if budget.should_auto_compact(
                        &self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .session_token_tracker,
                    ) {
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .needs_auto_compact = true;
                    }
                }
                (true, false, false)
            }
            AgentEvent::ToolStart {
                tool_call_id,
                name,
                display,
                args,
                input,
                source_agent_id,
            } => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_replied = true;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .tool_call_count += 1;
                // 跨切面：spinner
                self.session_mgr.sessions[self.session_mgr.active]
                    .spinner_state
                    .set_mode(peri_widgets::SpinnerMode::ToolUse);
                let verb_text = if !args.is_empty() {
                    let summary: String = args.chars().take(40).collect();
                    format!("{} {}", display, summary)
                } else {
                    format!("{}…", display)
                };
                self.session_mgr.sessions[self.session_mgr.active]
                    .spinner_state
                    .set_verb(Some(&verb_text));
                // Pipeline：创建 ToolBlock / 路由进 SubAgentGroup
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::ToolStart {
                        tool_call_id,
                        name,
                        display,
                        args,
                        input,
                        source_agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id,
            } => {
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::ToolEnd {
                        tool_call_id,
                        name,
                        output,
                        is_error,
                        source_agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::AssistantChunk {
                chunk,
                source_agent_id,
            } => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_replied = true;
                // 跨切面：spinner
                self.session_mgr.sessions[self.session_mgr.active]
                    .spinner_state
                    .set_mode(peri_widgets::SpinnerMode::Responding);
                // Pipeline：路由到 SubAgentGroup 或父 Agent AssistantBubble
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::AssistantChunk {
                        chunk,
                        source_agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::Done => {
                // Child agent Done during tool execution — ignore to prevent
                // TUI from treating it as parent completion (setting agent_rx=None,
                // loading=false, etc.). The parent ReAct loop is still blocked in
                // the tool call and will continue after it returns.
                if self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .in_subagent()
                {
                    return (false, false, false);
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = None;
                // Pipeline：finalize 当前 AI 消息
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::Done);
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                // 跳过已由 Interrupted/Error 处理器完成的 reconcile，
                // 防止 RebuildAll 覆盖它们添加的通知消息
                if !self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .reconcile_already_done
                {
                    self.request_rebuild();
                } else {
                    // Interrupted/Error 已完成 reconcile，清除 streaming 标志并重建
                    if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages
                            .last_mut()
                    {
                        *is_streaming = false;
                    }
                    self.render_rebuild();
                }
                // 跨切面：Langfuse
                let langfuse_tracer = self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_tracer
                    .take();
                if let Some(ref tracer) = langfuse_tracer {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .langfuse
                        .langfuse_flush_handle = Some(tracer.lock().on_trace_end(None));
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_tracer = None;
                self.set_loading(false);
                // 如果仍有后台任务在运行，保持 agent_rx 存活以接收 BackgroundTaskCompleted 事件
                if self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0 {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_done_pending_bg = true;
                    tracing::info!(
                        count = self.session_mgr.sessions[self.session_mgr.active]
                            .background_task_count,
                        "agent done but background tasks still running, keeping channel alive"
                    );
                } else {
                    // 竞态修复：检查是否有暂存的后台任务完成通知
                    // （BackgroundTaskCompleted 在 Done 之前被消费的情况）
                    if !self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .pre_done_bg_completions
                        .is_empty()
                    {
                        let notifications: Vec<String> = self.session_mgr.sessions
                            [self.session_mgr.active]
                            .agent
                            .pre_done_bg_completions
                            .drain(..)
                            .collect();
                        let combined = notifications.join("\n");
                        tracing::info!(
                            count = notifications.len(),
                            "Done: processing pre-done background task completions, setting continuation"
                        );
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .pending_bg_continuation = Some(combined);
                    }
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_rx = None;
                }
                // Auto-compact 两级策略
                // 中断后不触发 compact（Interrupted 已清除 needs_auto_compact），
                // agent_replied 为 false 时也跳过（上下文极小，无需压缩）
                // 后台任务运行时跳过 auto-compact：start_compact 会替换 agent_rx，
                // 导致后台任务的 BackgroundTaskCompleted 事件发送到已关闭的旧通道而丢失。
                // needs_auto_compact 标记保留，待后台任务完成后由 BackgroundTaskCompleted 处理器触发。
                let has_bg_tasks =
                    self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0;
                let should_check_compact = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_replied;
                if should_check_compact
                    && self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .needs_auto_compact
                    && !has_bg_tasks
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .needs_auto_compact = false;
                    tracing::info!(
                        "auto-compact: context threshold reached, triggering full compact"
                    );
                    self.start_compact("auto".to_string());
                    // Done 后 auto-compact 不应 resubmit：任务已结束，重新执行无意义
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .compact_should_resubmit = false;
                    return (true, false, true);
                } else if should_check_compact {
                    let compact_config = self.get_compact_config();
                    let budget = peri_agent::agent::token::ContextBudget::new(
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .context_window,
                    )
                    .with_warning_threshold(compact_config.micro_compact_threshold);
                    if budget.should_warn(
                        &self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .session_token_tracker,
                    ) {
                        self.start_micro_compact();
                    }
                }
                // 清理残留弹窗状态
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .interaction_prompt = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .pending_hitl_items = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .pending_ask_user = None;
                // circuit breaker 渐进恢复：每轮成功对话将 failure 计数减半
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .auto_compact_failures
                    > 0
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .auto_compact_failures /= 2;
                }
                if let Some(start) = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .task_start_time
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pending_messages
                    .is_empty()
                {
                    self.flush_pending_messages();
                }
                (true, false, true)
            }
            AgentEvent::Interrupted => {
                // Child agent interrupted during tool execution — ignore;
                // parent tool call will handle the result when it returns.
                if self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .in_subagent()
                {
                    return (false, false, false);
                }
                // 中断后不应触发 auto-compact（上下文可能还很充裕），
                // 清除标记，防止紧随其后的 Done 事件误触发
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .needs_auto_compact = false;
                // Pipeline：finalize 当前状态
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::Interrupted);
                for action in actions {
                    self.apply_pipeline_action(action);
                }

                let agent_replied = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_replied;
                if !agent_replied {
                    // Agent 尚未回复，恢复用户文本到输入框
                    // 不走 reconcile_tail（它会从 completed 中找到 Human 消息并重新渲染），
                    // 直接截断到 round_start 并清除 pipeline。
                    if let Some(text) = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .last_submitted_text
                        .take()
                    {
                        let round_start = self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .round_start_vm_idx;
                        // 截断 view_messages（移除本轮 Human 消息）
                        self.apply_pipeline_action(PipelineAction::RebuildAll {
                            prefix_len: round_start,
                            tail_vms: vec![],
                        });
                        // 截断 agent_state_messages（回滚 StateSnapshot 扩展的内容）
                        let pre_len = self.session_mgr.sessions[self.session_mgr.active]
                            .metadata
                            .pre_submit_state_len;
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .agent_state_messages
                            .truncate(pre_len);
                        // 恢复文本到输入框
                        let mut ta = crate::app::build_textarea(false);
                        ta.insert_str(text.clone());
                        self.session_mgr.sessions[self.session_mgr.active]
                            .ui
                            .textarea = ta;
                        // 清除 pending 缓冲
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .pending_messages
                            .clear();
                        // 清除 sticky header
                        self.session_mgr.sessions[self.session_mgr.active]
                            .metadata
                            .last_human_message = None;
                        // 清除 pipeline 状态（completed 中含本轮 Human 消息）
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .pipeline
                            .done();
                        let restored = self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .agent_state_messages
                            .clone();
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .pipeline
                            .restore_completed(restored);
                        let vm = MessageViewModel::system(
                            self.services.lc.tr("app-interrupted-resumed"),
                        );
                        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    } else {
                        let vm =
                            MessageViewModel::system(self.services.lc.tr("app-interrupt-done"));
                        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    }
                } else {
                    self.request_rebuild();
                    let vm = MessageViewModel::system(self.services.lc.tr("app-interrupt-done"));
                    self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    // 标记 reconcile 已完成，防止后续 Done 事件重复 RebuildAll 覆盖通知消息
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .reconcile_already_done = true;
                }
                (true, false, false)
            }
            AgentEvent::Error(ref e) => {
                // Child agent error during tool execution — ignore;
                // the error is captured in the tool result string that
                // invoke_fork/invoke_normal returns to the parent.
                if self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .in_subagent()
                {
                    return (false, false, false);
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = None;
                // 清理 pipeline 状态（残留 SubAgent 栈等），防止下一个任务 UI 损坏
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .done();

                let mut vm = MessageViewModel::tool_block(
                    "error".to_string(),
                    "Agent Error".to_string(),
                    None,
                    true,
                );
                // 将完整错误信息放入 content，并默认展开，确保用户能看到
                if let MessageViewModel::ToolBlock {
                    content, collapsed, ..
                } = &mut vm
                {
                    *content = e.clone();
                    *collapsed = false;
                }
                self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                // 标记 reconcile 已完成，防止后续 Done 事件重复 RebuildAll 覆盖错误消息
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .reconcile_already_done = true;
                // Langfuse：错误路径也需结束 Trace
                let langfuse_tracer = self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_tracer
                    .take();
                if let Some(ref tracer) = langfuse_tracer {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .langfuse
                        .langfuse_flush_handle =
                        Some(tracer.lock().on_trace_end(Some(&format!("ERROR: {}", e))));
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_tracer = None;
                self.set_loading(false);
                // Error 路径同样需要保持通道给后台任务
                if self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0 {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_done_pending_bg = true;
                } else {
                    // 竞态修复：检查是否有暂存的后台任务完成通知
                    if !self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .pre_done_bg_completions
                        .is_empty()
                    {
                        let notifications: Vec<String> = self.session_mgr.sessions
                            [self.session_mgr.active]
                            .agent
                            .pre_done_bg_completions
                            .drain(..)
                            .collect();
                        let combined = notifications.join("\n");
                        tracing::info!(
                            count = notifications.len(),
                            "Error: processing pre-done background task completions, setting continuation"
                        );
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .pending_bg_continuation = Some(combined);
                    }
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_rx = None;
                }
                // Agent 出错时清理残留弹窗状态，避免 UI 卡在弹窗
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .interaction_prompt = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .pending_hitl_items = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .pending_ask_user = None;
                if let Some(start) = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .task_start_time
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pending_messages
                    .is_empty()
                {
                    self.flush_pending_messages();
                }
                (true, false, true)
            }
            AgentEvent::InteractionRequest { ctx, response_tx } => {
                use peri_agent::interaction::{
                    ApprovalDecision, InteractionContext, InteractionResponse, QuestionAnswer,
                };
                use peri_middlewares::ask_user::{
                    AskUserBatchRequest, AskUserOption, AskUserQuestionData,
                };
                use tokio::sync::oneshot;

                match ctx {
                    InteractionContext::Approval { items } => {
                        let batch_items: Vec<BatchItem> = items
                            .iter()
                            .map(|i| BatchItem {
                                tool_name: i.tool_name.clone(),
                                input: i.tool_input.clone(),
                            })
                            .collect();
                        let (bridge_tx, bridge_rx) = oneshot::channel::<Vec<HitlDecision>>();
                        tokio::spawn(async move {
                            if let Ok(decisions) = bridge_rx.await {
                                let approval_decisions: Vec<ApprovalDecision> = decisions
                                    .into_iter()
                                    .map(|d| match d {
                                        HitlDecision::Approve => ApprovalDecision::Approve,
                                        HitlDecision::Reject => ApprovalDecision::Reject {
                                            reason: "User rejected".to_string(),
                                        },
                                        HitlDecision::Edit(v) => {
                                            ApprovalDecision::Edit { new_input: v }
                                        }
                                        HitlDecision::Respond(msg) => {
                                            ApprovalDecision::Respond { message: msg }
                                        }
                                    })
                                    .collect();
                                let _ = response_tx
                                    .send(InteractionResponse::Decisions(approval_decisions));
                            }
                        });
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .interaction_prompt = Some(InteractionPrompt::Approval(
                            HitlBatchPrompt::new(batch_items, bridge_tx),
                        ));
                        (true, true, false) // 暂停消费，等待用户确认
                    }
                    InteractionContext::Questions { requests } => {
                        let ask_questions: Vec<AskUserQuestionData> = requests
                            .iter()
                            .map(|q| AskUserQuestionData {
                                tool_call_id: q.id.clone(),
                                question: q.question.clone(),
                                header: q.header.clone(),
                                multi_select: q.multi_select,
                                options: q
                                    .options
                                    .iter()
                                    .map(|o| AskUserOption {
                                        label: o.label.clone(),
                                        description: o.description.clone(),
                                    })
                                    .collect(),
                            })
                            .collect();
                        let (bridge_tx, bridge_rx) = oneshot::channel::<Vec<String>>();
                        let ids: Vec<String> = requests.iter().map(|q| q.id.clone()).collect();
                        tokio::spawn(async move {
                            if let Ok(answers) = bridge_rx.await {
                                let question_answers: Vec<QuestionAnswer> = ids
                                    .into_iter()
                                    .zip(answers)
                                    .map(|(id, answer)| QuestionAnswer {
                                        id,
                                        selected: vec![answer.clone()],
                                        text: Some(answer),
                                    })
                                    .collect();
                                let _ = response_tx
                                    .send(InteractionResponse::Answers(question_answers));
                            }
                        });
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .pending_ask_user = Some(false);
                        let (batch_req, _) = AskUserBatchRequest::new(ask_questions);
                        let batch_req_bridged = AskUserBatchRequest {
                            questions: batch_req.questions,
                            response_tx: bridge_tx,
                        };
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .interaction_prompt = Some(InteractionPrompt::Questions(
                            AskUserBatchPrompt::from_request(batch_req_bridged),
                        ));
                        (true, true, false) // 暂停消费，等待用户输入
                    }
                }
            }
            AgentEvent::TodoUpdate(todos) => {
                self.session_mgr.sessions[self.session_mgr.active].todo_items = todos;
                (true, false, false)
            }
            AgentEvent::StateSnapshot(msgs) => {
                // 子 Agent 的 StateSnapshot 不应污染父 Agent 的 agent_state_messages，
                // 否则子 Agent 的全部内部消息会混入父 Agent 的对话历史和持久化。
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth
                    > 0
                {
                    return (true, false, false);
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_state_messages
                    .extend(msgs.clone());
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::StateSnapshot(msgs));
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::CompactDone {
                summary,
                new_thread_id: _,
            } => self.handle_compact_done(summary),
            AgentEvent::CompactError(msg) => self.handle_compact_error(msg),
            AgentEvent::LlmRetrying {
                attempt,
                max_attempts,
                delay_ms,
                error,
            } => {
                // 子 Agent 的 LlmRetrying 不应覆盖父 Agent 的 retry_status 显示
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .subagent_depth
                    > 0
                {
                    return (true, false, false);
                }
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = Some(super::agent_comm::RetryStatus {
                    attempt,
                    max_attempts,
                    delay_ms,
                    error,
                });
                (true, false, false)
            }
            AgentEvent::AiReasoning(text) => {
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::AiReasoning(text));
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::BackgroundTaskCompleted {
                task_id,
                agent_name,
                success,
                output,
                tool_calls_count,
                duration_ms,
            } => self.handle_background_task_completed(
                task_id,
                agent_name,
                success,
                output,
                tool_calls_count,
                duration_ms,
            ),
            AgentEvent::LspDiagnostics {
                errors,
                warnings,
                files_with_errors,
            } => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .lsp_errors = errors;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .lsp_warnings = warnings;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .lsp_files_with_errors = files_with_errors;
                (true, false, false)
            }
        }
    }

    /// 每帧调用：消费 channel 事件，返回是否有 UI 更新
    pub fn poll_agent(&mut self) -> bool {
        // 优先处理延迟的后台任务 continuation（由 BackgroundTaskCompleted 处理器设置）
        // 只有在 loading=false 时才 take()，避免 loading=true（如 compact 中）时
        // continuation 被消费但未使用而永久丢失
        if !self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .loading
        {
            if let Some(continuation) = self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .pending_bg_continuation
                .take()
            {
                tracing::info!("auto-submitting background task continuation");
                self.submit_message(continuation);
                return true;
            }
        }

        if self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_rx
            .is_none()
        {
            return false;
        }

        let mut updated = false;

        // 节流检查（每帧开始时，确保上一批 chunk 的尾部也被显示）
        {
            let prefix_len = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .round_start_vm_idx;
            if let Some(action) = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .check_throttle(prefix_len)
            {
                self.apply_pipeline_action(action);
                updated = true;
            }
        }

        loop {
            let result = self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_rx
                .as_mut()
                .map(|rx| rx.try_recv());
            match result {
                Some(Ok(event)) => {
                    let (ev_updated, should_break, should_return) = self.handle_agent_event(event);
                    if ev_updated {
                        updated = true;
                    }
                    if should_return {
                        return true;
                    }
                    if should_break {
                        break;
                    }
                }
                Some(Err(mpsc::error::TryRecvError::Empty)) | None => break,
                Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                    // 清理 pipeline 状态（残留 SubAgent 栈等）
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .pipeline
                        .done();
                    // 重置 subagent_depth，防止残留计数过滤后续 TokenUsageUpdate
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .subagent_depth = 0;

                    // 后台任务场景：spawn closure 结束后丢弃最后一个 sender 导致通道关闭。
                    // 如果有后台任务，说明 BackgroundTaskCompleted 已处理或通道竞态关闭，
                    // 不应显示 "连接异常断开" 错误。静默清理并结束 loading 状态。
                    if self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_done_pending_bg
                        || self.session_mgr.sessions[self.session_mgr.active].background_task_count
                            > 0
                    {
                        tracing::info!(
                            agent_done = self.session_mgr.sessions[self.session_mgr.active]
                                .agent
                                .agent_done_pending_bg,
                            bg_count = self.session_mgr.sessions[self.session_mgr.active]
                                .background_task_count,
                            "channel disconnected during background task flow, suppressing error"
                        );
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .agent_done_pending_bg = false;
                        self.session_mgr.sessions[self.session_mgr.active].background_task_count =
                            0;
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .pre_done_bg_completions
                            .clear();
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .agent_rx = None;
                        let langfuse_tracer = self.session_mgr.sessions[self.session_mgr.active]
                            .langfuse
                            .langfuse_tracer
                            .take();
                        if let Some(ref tracer) = langfuse_tracer {
                            self.session_mgr.sessions[self.session_mgr.active]
                                .langfuse
                                .langfuse_flush_handle = Some(tracer.lock().on_trace_end(None));
                        }
                        self.session_mgr.sessions[self.session_mgr.active]
                            .langfuse
                            .langfuse_tracer = None;
                        self.set_loading(false);
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .interaction_prompt = None;
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .pending_hitl_items = None;
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .pending_ask_user = None;
                        if let Some(start) = self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .task_start_time
                        {
                            self.session_mgr.sessions[self.session_mgr.active]
                                .agent
                                .last_task_duration = Some(start.elapsed());
                        }
                        return true;
                    }

                    let vm = MessageViewModel::tool_block(
                        "error".to_string(),
                        "agent-error".to_string(),
                        Some(self.services.lc.tr("app-agent-disconnected")),
                        true,
                    );
                    self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    let langfuse_tracer = self.session_mgr.sessions[self.session_mgr.active]
                        .langfuse
                        .langfuse_tracer
                        .take();
                    if let Some(ref tracer) = langfuse_tracer {
                        self.session_mgr.sessions[self.session_mgr.active]
                            .langfuse
                            .langfuse_flush_handle =
                            Some(tracer.lock().on_trace_end(Some(
                                "ERROR: agent channel disconnected unexpectedly",
                            )));
                    }
                    self.session_mgr.sessions[self.session_mgr.active]
                        .langfuse
                        .langfuse_tracer = None;
                    self.set_loading(false);
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_rx = None;
                    // 清理残留弹窗状态，避免 UI 卡在弹窗
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .interaction_prompt = None;
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .pending_hitl_items = None;
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .pending_ask_user = None;
                    if let Some(start) = self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .task_start_time
                    {
                        self.session_mgr.sessions[self.session_mgr.active]
                            .agent
                            .last_task_duration = Some(start.elapsed());
                    }
                    return true;
                }
            }
        }

        updated
    }

    /// 每帧调用：消费后台事件通道（MCP OAuth 等异步任务发送的事件），返回是否有 UI 更新
    pub fn poll_background_events(&mut self) -> bool {
        let events: Vec<_> = match self.services.bg_event_rx.as_mut() {
            Some(rx) => {
                let mut evts = Vec::new();
                loop {
                    match rx.try_recv() {
                        Ok(event) => evts.push(event),
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            self.services.bg_event_rx = None;
                            break;
                        }
                    }
                }
                evts
            }
            None => return false,
        };
        let mut updated = false;
        for event in events {
            let (ev_updated, _should_break, should_return) = self.handle_agent_event(event);
            if ev_updated {
                updated = true;
            }
            if should_return {
                return true;
            }
        }
        updated
    }

    /// 每帧调用：检查 cron 触发事件，空闲时自动提交 prompt
    pub fn poll_cron_triggers(&mut self) {
        let cron_triggers: Vec<_> = self
            .services
            .cron
            .trigger_rx
            .as_mut()
            .map(|rx| {
                let mut triggers = Vec::new();
                while let Ok(trigger) = rx.try_recv() {
                    triggers.push(trigger);
                }
                triggers
            })
            .unwrap_or_default();
        for trigger in cron_triggers {
            if !self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .loading
            {
                self.submit_message(trigger.prompt);
            } else {
                // Agent 正在执行，缓冲触发事件等待 Done 后自动发送
                const MAX_PENDING: usize = 10;
                if self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pending_messages
                    .len()
                    < MAX_PENDING
                {
                    tracing::debug!(prompt = %trigger.prompt, "cron trigger buffered (agent busy)");
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .pending_messages
                        .push(trigger.prompt);
                } else {
                    tracing::warn!("pending_messages 已达上限 {}，丢弃 cron 触发", MAX_PENDING);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "agent_ops_test.rs"]
mod tests;
