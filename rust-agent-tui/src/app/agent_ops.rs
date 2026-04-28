use super::*;
use rust_agent_middlewares::hitl::BatchItem;
use super::message_pipeline::PipelineAction;

impl App {
    pub fn submit_message(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        self.push_input_history(input.clone());

        // 消费待发送附件
        let attachments = std::mem::take(&mut self.core.pending_attachments);

        // 构建用于显示的文字（附件摘要追加在末尾）
        let display = if attachments.is_empty() {
            input.clone()
        } else {
            format!("{} [🖼 {} 张图片]", input, attachments.len())
        };
        let user_vm = MessageViewModel::user(display.clone());
        self.apply_pipeline_action(PipelineAction::AddMessage(user_vm));
        self.core.last_human_message = Some(display);
        self.set_loading(true);
        self.core.scroll_offset = u16::MAX;
        self.core.scroll_follow = true;
        self.todo_items.clear();

        // 开始计时新任务
        self.agent.task_start_time = Some(std::time::Instant::now());
        self.agent.last_task_duration = None;

        let provider = match self
            .zen_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config)
            .or_else(agent::LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                self.apply_pipeline_action(PipelineAction::AddMessage(
                    MessageViewModel::tool_block(
                        "error".to_string(),
                        "config-error".to_string(),
                        None,
                        true,
                    ),
                ));
                self.set_loading(false);
                return;
            }
        };

        let (tx, rx) = mpsc::channel(32);
        self.agent.agent_rx = Some(rx);

        // 创建取消令牌（Ctrl+C 触发中断）
        let cancel = AgentCancellationToken::new();
        self.agent.cancel_token = Some(cancel.clone());

        // 注意：HITL 审批和 AskUser 问答现在统一通过 TuiInteractionBroker 路由到 tx channel，
        // YOLO 模式由 HumanInTheLoopMiddleware::from_env() 内部处理（自动放行）。

        let cwd = self.cwd.clone();

        // 构建多模态 AgentInput（有附件时包含图片 blocks）
        let agent_input = if attachments.is_empty() {
            AgentInput::text(input.clone())
        } else {
            let mut blocks = vec![ContentBlock::text(input.clone())];
            for att in &attachments {
                blocks.push(ContentBlock::image_base64(
                    &att.media_type,
                    &att.base64_data,
                ));
            }
            AgentInput::blocks(MessageContent::blocks(blocks))
        };

        // 解析消息中的 #skill-name（字母、数字、连字符、下划线）
        let preload_skills: Vec<String> = input
            .split_whitespace()
            .filter(|token| token.starts_with('#') && token.len() > 1)
            .map(|token| {
                let name = token.trim_start_matches('#');
                // 只取合法字符（字母、数字、-、_），遇到非法字符截断
                name.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect();

        // 确保当前 thread 存在
        let thread_id = self.ensure_thread_id();

        // 懒加载 Thread 级 LangfuseSession（首轮创建，后续复用；未配置环境变量时静默跳过）
        if self.langfuse.langfuse_session.is_none() {
            tracing::debug!(thread_id = %thread_id, "langfuse: session is None, attempting to create");
            if let Some(cfg) = crate::langfuse::LangfuseConfig::from_env() {
                tracing::debug!(host = %cfg.host, "langfuse: config found, creating session");
                let session_id = thread_id.clone();
                let session = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(crate::langfuse::LangfuseSession::new(cfg, session_id))
                });
                if session.is_some() {
                    tracing::info!(thread_id = %thread_id, "langfuse: session created successfully");
                } else {
                    tracing::warn!(thread_id = %thread_id, "langfuse: session creation failed (None)");
                }
                self.langfuse.langfuse_session = session.map(Arc::new);
            } else {
                tracing::debug!("langfuse: no config found in env, skipping session creation");
            }
        } else {
            tracing::debug!(thread_id = %thread_id, "langfuse: reusing existing session");
        }

        // 构造当前轮次的 Langfuse Tracer（同步，复用共享 Session）
        let langfuse_tracer = self.langfuse.langfuse_session.clone().map(|session| {
            let mut t = crate::langfuse::LangfuseTracer::new(session);
            t.on_trace_start(input.trim());
            Arc::new(parking_lot::Mutex::new(t))
        });
        self.langfuse.langfuse_tracer = langfuse_tracer.clone();

        let span = tracing::info_span!(
            "thread.run",
            thread.id = %thread_id,
            thread.cwd = %cwd,
        );
        let history = self.agent.agent_state_messages.clone();
        let agent_id = self.agent.agent_id.clone();
        let thread_store = self.thread_store.clone();
        let thread_id_for_agent = thread_id.clone();
        let zen_config_for_agent = Arc::new(self.zen_config.clone().unwrap_or_default());
        let cron_scheduler = Some(self.cron.scheduler.clone());
        let permission_mode = self.permission_mode.clone();
        tokio::spawn(
            async move {
                agent::run_universal_agent(agent::AgentRunConfig {
                    provider,
                    input: agent_input,
                    cwd,
                    history,
                    tx,
                    cancel,
                    agent_id,
                    langfuse_tracer,
                    thread_store,
                    thread_id: thread_id_for_agent,
                    preload_skills,
                    config: zen_config_for_agent,
                    cron_scheduler,
                    permission_mode,
                })
                .await;
            }
            .instrument(span),
        );
    }

    /// 将 PipelineAction 映射到 view_messages 更新 + RenderEvent 发送
    fn apply_pipeline_action(&mut self, action: PipelineAction) {
        match action {
            PipelineAction::None => {}
            PipelineAction::AddMessage(vm) => {
                self.core.view_messages.push(vm.clone());
                let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
            }
            PipelineAction::AppendChunk(chunk) => {
                match self.core.view_messages.last_mut() {
                    Some(m) if m.is_assistant() => {
                        m.append_chunk(&chunk);
                    }
                    _ => {
                        // 首个 chunk：创建带内容的 assistant bubble，通过 AddMessage 通知渲染线程
                        let mut vm = MessageViewModel::assistant();
                        vm.append_chunk(&chunk);
                        self.core.view_messages.push(vm.clone());
                        let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                        return;
                    }
                }
                let _ = self.core.render_tx.send(RenderEvent::AppendChunk(chunk));
            }
            PipelineAction::UpdateLast(vm) => {
                if let Some(last) = self.core.view_messages.last_mut() {
                    *last = vm.clone();
                } else {
                    self.core.view_messages.push(vm.clone());
                }
                let _ = self.core.render_tx.send(RenderEvent::UpdateLastMessage(vm));
            }
            PipelineAction::RemoveLast => {
                self.core.view_messages.pop();
                let _ = self.core.render_tx.send(RenderEvent::RemoveLastMessage);
            }
            PipelineAction::RemoveLastN(n) => {
                for _ in 0..n {
                    self.core.view_messages.pop();
                }
                for _ in 0..n {
                    let _ = self.core.render_tx.send(RenderEvent::RemoveLastMessage);
                }
            }
            PipelineAction::StreamingDone => {
                if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                    self.core.view_messages.last_mut()
                {
                    *is_streaming = false;
                }
                let _ = self.core.render_tx.send(RenderEvent::StreamingDone);
            }
            PipelineAction::RebuildAll(vms) => {
                self.core.view_messages = vms.clone();
                let _ = self.core.render_tx.send(RenderEvent::LoadHistory(vms));
            }
        }
    }

    /// 处理单个 AgentEvent，返回 `(updated, should_break, should_return)`
    pub(crate) fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
        match event {
            AgentEvent::SubAgentStart { agent_id, task_preview } => {
                // 跨切面：Langfuse
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    tracer.lock().on_subagent_start(&agent_id, &task_preview);
                }
                // Pipeline：创建 SubAgentGroup VM
                let actions = self.core.pipeline.handle_event(
                    AgentEvent::SubAgentStart { agent_id, task_preview }
                );
                for action in actions { self.apply_pipeline_action(action); }
                (true, false, false)
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                // 跨切面：Langfuse
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    tracer.lock().on_subagent_end(&result, is_error);
                }
                // Pipeline：更新 SubAgentGroup（is_running=false, final_result）
                let actions = self.core.pipeline.handle_event(
                    AgentEvent::SubAgentEnd { result, is_error }
                );
                for action in actions { self.apply_pipeline_action(action); }
                (true, false, false)
            }
            AgentEvent::TokenUsageUpdate { usage, model: _model } => {
                // 累积到会话追踪器
                self.agent.session_token_tracker.accumulate(&usage);
                // 更新 spinner 的 token 显示
                let total = self.agent.session_token_tracker.total_input_tokens
                    + self.agent.session_token_tracker.total_output_tokens;
                self.spinner_state.set_token_count(total as usize);
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
                if (self.agent.auto_compact_failures as u32) < compact_config.max_consecutive_failures {
                    let budget = rust_create_agent::agent::token::ContextBudget::new(
                        self.agent.context_window,
                    )
                    .with_auto_compact_threshold(compact_config.auto_compact_threshold);
                    if budget.should_auto_compact(&self.agent.session_token_tracker) {
                        self.agent.needs_auto_compact = true;
                    }
                }
                (true, false, false)
            }
            AgentEvent::ToolStart { tool_call_id, name, display, args, input } => {
                self.agent.retry_status = None;
                // 跨切面：spinner
                self.spinner_state.set_mode(perihelion_widgets::SpinnerMode::ToolUse);
                let verb_text = if !args.is_empty() {
                    let summary: String = args.chars().take(40).collect();
                    format!("{} {}", display, summary)
                } else {
                    format!("{}…", display)
                };
                self.spinner_state.set_verb(Some(&verb_text));
                // Pipeline：创建 ToolBlock / 路由进 SubAgentGroup
                let actions = self.core.pipeline.handle_event(
                    AgentEvent::ToolStart { tool_call_id, name, display, args, input }
                );
                for action in actions { self.apply_pipeline_action(action); }
                (true, false, false)
            }
            AgentEvent::ToolEnd { tool_call_id, name, output, is_error } => {
                // Pipeline：更新 ToolBlock 结果 / SubAgentGroup 完成
                let actions = self.core.pipeline.handle_event(
                    AgentEvent::ToolEnd { tool_call_id, name, output, is_error }
                );
                for action in actions { self.apply_pipeline_action(action); }
                (true, false, false)
            }
            AgentEvent::AssistantChunk(chunk) => {
                self.agent.retry_status = None;
                // 跨切面：spinner
                self.spinner_state.set_mode(perihelion_widgets::SpinnerMode::Responding);
                // Pipeline：路由到 SubAgentGroup 或父 Agent AssistantBubble
                let actions = self.core.pipeline.handle_event(
                    AgentEvent::AssistantChunk(chunk)
                );
                for action in actions { self.apply_pipeline_action(action); }
                (true, false, false)
            }
            AgentEvent::Done => {
                self.agent.retry_status = None;
                // Pipeline：finalize 当前 AI 消息 + reconcile 重建 view_messages
                let actions = self.core.pipeline.handle_event(AgentEvent::Done);
                for action in actions { self.apply_pipeline_action(action); }
                // 跨切面：Langfuse
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    self.langfuse.langfuse_flush_handle = Some(tracer.lock().on_trace_end(None));
                }
                self.langfuse.langfuse_tracer = None;
                self.set_loading(false);
                self.agent.agent_rx = None;
                // Auto-compact 两级策略
                if self.agent.needs_auto_compact {
                    self.agent.needs_auto_compact = false;
                    tracing::info!("auto-compact: context threshold reached, triggering full compact");
                    self.start_compact("auto".to_string());
                    return (true, false, true);
                } else {
                    let compact_config = self.get_compact_config();
                    let budget = rust_create_agent::agent::token::ContextBudget::new(
                        self.agent.context_window,
                    )
                    .with_warning_threshold(compact_config.micro_compact_threshold);
                    if budget.should_warn(&self.agent.session_token_tracker) {
                        self.start_micro_compact();
                    }
                }
                // 清理残留弹窗状态
                self.agent.interaction_prompt = None;
                self.agent.pending_hitl_items = None;
                self.agent.pending_ask_user = None;
                // circuit breaker 渐进恢复：每轮成功对话将 failure 计数减半
                if self.agent.auto_compact_failures > 0 {
                    self.agent.auto_compact_failures /= 2;
                }
                if let Some(start) = self.agent.task_start_time {
                    self.agent.last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.core.pending_messages.is_empty() {
                    let combined = self.core.pending_messages.join("\n\n");
                    self.core.pending_messages.clear();
                    self.submit_message(combined);
                }
                (true, false, true)
            }
            AgentEvent::Interrupted => {
                // Pipeline：finalize 当前状态
                let actions = self.core.pipeline.handle_event(AgentEvent::Interrupted);
                for action in actions { self.apply_pipeline_action(action); }
                // 系统消息由 agent_ops 直接显示
                let vm = MessageViewModel::system(
                    "⚠ 已中断（工具调用已以 error 结尾，消息已保存，可继续发送恢复）".to_string(),
                );
                self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::Error(ref e) => {
                let mut vm = MessageViewModel::tool_block(
                    "error".to_string(),
                    "Agent Error".to_string(),
                    None,
                    true,
                );
                // 将完整错误信息放入 content，并默认展开，确保用户能看到
                if let MessageViewModel::ToolBlock { content, collapsed, .. } = &mut vm {
                    *content = e.clone();
                    *collapsed = false;
                }
                self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                // Langfuse：错误路径也需结束 Trace
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    self.langfuse.langfuse_flush_handle = Some(tracer.lock().on_trace_end(Some(&format!("ERROR: {}", e))));
                }
                self.langfuse.langfuse_tracer = None;
                self.set_loading(false);
                self.agent.agent_rx = None;
                // Agent 出错时清理残留弹窗状态，避免 UI 卡在弹窗
                self.agent.interaction_prompt = None;
                self.agent.pending_hitl_items = None;
                self.agent.pending_ask_user = None;
                if let Some(start) = self.agent.task_start_time {
                    self.agent.last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.core.pending_messages.is_empty() {
                    let combined = self.core.pending_messages.join("\n\n");
                    self.core.pending_messages.clear();
                    self.submit_message(combined);
                }
                (true, false, true)
            }
            AgentEvent::InteractionRequest { ctx, response_tx } => {
                use rust_create_agent::interaction::{
                    ApprovalDecision, InteractionContext, InteractionResponse, QuestionAnswer,
                };
                use rust_agent_middlewares::ask_user::{AskUserBatchRequest, AskUserOption, AskUserQuestionData};
                use tokio::sync::oneshot;

                match ctx {
                    InteractionContext::Approval { items } => {
                        let batch_items: Vec<BatchItem> = items
                            .iter()
                            .map(|i| BatchItem { tool_name: i.tool_name.clone(), input: i.tool_input.clone() })
                            .collect();
                        let (bridge_tx, bridge_rx) = oneshot::channel::<Vec<HitlDecision>>();
                        tokio::spawn(async move {
                            if let Ok(decisions) = bridge_rx.await {
                                let approval_decisions: Vec<ApprovalDecision> = decisions
                                    .into_iter()
                                    .map(|d| match d {
                                        HitlDecision::Approve => ApprovalDecision::Approve,
                                        HitlDecision::Reject => ApprovalDecision::Reject { reason: "用户拒绝".to_string() },
                                        HitlDecision::Edit(v) => ApprovalDecision::Edit { new_input: v },
                                        HitlDecision::Respond(msg) => ApprovalDecision::Respond { message: msg },
                                    })
                                    .collect();
                                let _ = response_tx.send(InteractionResponse::Decisions(approval_decisions));
                            }
                        });
                        self.agent.interaction_prompt = Some(InteractionPrompt::Approval(HitlBatchPrompt::new(batch_items, bridge_tx)));
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
                                options: q.options.iter().map(|o| AskUserOption {
                                    label: o.label.clone(),
                                    description: o.description.clone(),
                                }).collect(),
                            })
                            .collect();
                        let (bridge_tx, bridge_rx) = oneshot::channel::<Vec<String>>();
                        let ids: Vec<String> = requests.iter().map(|q| q.id.clone()).collect();
                        tokio::spawn(async move {
                            if let Ok(answers) = bridge_rx.await {
                                let question_answers: Vec<QuestionAnswer> = ids
                                    .into_iter()
                                    .zip(answers.into_iter())
                                    .map(|(id, answer)| QuestionAnswer { id, selected: vec![answer.clone()], text: Some(answer) })
                                    .collect();
                                let _ = response_tx.send(InteractionResponse::Answers(question_answers));
                            }
                        });
                        self.agent.pending_ask_user = Some(false);
                        {
                            let q_lines: Vec<String> = requests.iter().map(|q| {
                                let hint = if q.multi_select { " [多选]" } else { " [单选]" };
                                format!("{}{}: {}", q.header, hint, q.question)
                            }).collect();
                            let vm = MessageViewModel::system(format!("❓ 向你提问:\n{}", q_lines.join("\n")));
                            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                        }
                        let (batch_req, _) = AskUserBatchRequest::new(ask_questions);
                        let batch_req_bridged = AskUserBatchRequest { questions: batch_req.questions, response_tx: bridge_tx };
                        self.agent.interaction_prompt = Some(InteractionPrompt::Questions(AskUserBatchPrompt::from_request(batch_req_bridged)));
                        (true, true, false) // 暂停消费，等待用户输入
                    }
                }
            }
            AgentEvent::TodoUpdate(todos) => {
                self.todo_items = todos;
                (true, false, false)
            }
            AgentEvent::StateSnapshot(msgs) => {
                tracing::debug!(count = msgs.len(), "received StateSnapshot in poll_agent");
                for msg in &msgs {
                    match msg {
                        BaseMessage::Ai { content: _, tool_calls, .. } => {
                            tracing::debug!(
                                has_tc = !tool_calls.is_empty(),
                                tc_len = tool_calls.len(),
                                "ai msg in snapshot"
                            );
                        }
                        BaseMessage::Tool { tool_call_id, .. } => {
                            tracing::debug!(tc_id = %tool_call_id, "tool msg in snapshot");
                        }
                        _ => {}
                    }
                }
                self.agent.agent_state_messages.extend(msgs.clone());
                // Pipeline：更新 completed 状态（用于后续 reconcile）
                let actions = self.core.pipeline.handle_event(
                    AgentEvent::StateSnapshot(msgs)
                );
                for action in actions { self.apply_pipeline_action(action); }
                (true, false, false)
            }
            AgentEvent::CompactDone { summary, new_thread_id: _ } => {
                // 拆分摘要和重新注入内容
                let (summary_text, re_inject_messages) = if let Some(idx) = summary.find("---RE_INJECT_SEPARATOR---\n") {
                    let parts: (&str, &str) = summary.split_at(idx);
                    let re_inject_part = parts.1.strip_prefix("---RE_INJECT_SEPARATOR---\n").unwrap_or("");
                    let re_inject_msgs: Vec<BaseMessage> = re_inject_part
                        .split("\n\n")
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| BaseMessage::system(s.to_string()))
                        .collect();
                    (parts.0.to_string(), re_inject_msgs)
                } else {
                    (summary.clone(), Vec::new())
                };

                let truncated: String = summary_text.chars().take(30).collect();
                let ellipsis = if summary_text.chars().count() > 30 { "…" } else { "" };
                let thread_title = format!("Compact: {}{}", truncated, ellipsis);
                let mut meta = ThreadMeta::new(&self.cwd);
                meta.title = Some(thread_title);
                let store = self.thread_store.clone();
                let new_tid = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(store.create_thread(meta))
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "compact: 创建新 thread 失败，使用临时 ID");
                            uuid::Uuid::now_v7().to_string()
                        })
                });

                let mut new_messages = vec![BaseMessage::system(summary_text.clone())];
                new_messages.extend(re_inject_messages);

                let store = self.thread_store.clone();
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(store.append_messages(&new_tid, &new_messages))
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, thread_id = %new_tid, "compact: 持久化新 thread 消息失败");
                        });
                });

                self.current_thread_id = Some(new_tid.clone());
                self.agent.agent_state_messages = new_messages;

                self.core.pipeline.clear();
                self.core.pipeline.restore_completed(self.agent.agent_state_messages.clone());

                let compact_vm = MessageViewModel::system(
                    "上下文已压缩（从旧对话迁移到新 Thread）".to_string(),
                );
                let summary_vm = MessageViewModel::from_base_message(
                    &BaseMessage::ai(format!("压缩摘要：\n{}", summary_text)),
                    &[],
                );
                let mut view_msgs = vec![compact_vm, summary_vm];

                let inject_count = self.agent.agent_state_messages.len() - 1;
                if inject_count > 0 {
                    let inject_vm = MessageViewModel::system(
                        format!("已重新注入 {} 条上下文（文件/Skills）", inject_count),
                    );
                    view_msgs.push(inject_vm);
                }
                self.apply_pipeline_action(PipelineAction::RebuildAll(view_msgs));

                self.set_loading(false);
                self.agent.agent_rx = None;

                self.langfuse.langfuse_session = None;
                self.agent.auto_compact_failures = 0;

                if !self.core.pending_messages.is_empty() {
                    let combined = self.core.pending_messages.join("\n\n");
                    self.core.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
            AgentEvent::CompactError(msg) => {
                let vm = MessageViewModel::system(format!("❌ 压缩失败: {}", msg));
                self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                self.set_loading(false);
                self.agent.agent_rx = None;
                self.agent.auto_compact_failures += 1;

                if !self.core.pending_messages.is_empty() {
                    let combined = self.core.pending_messages.join("\n\n");
                    self.core.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
            AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error: _ } => {
                self.agent.retry_status = Some(super::agent_comm::RetryStatus { attempt, max_attempts, delay_ms });
                (true, false, false)
            }
        }
    }

    /// 每帧调用：消费 channel 事件，返回是否有 UI 更新
    pub fn poll_agent(&mut self) -> bool {
        if self.agent.agent_rx.is_none() {
            return false;
        }

        let mut updated = false;

        loop {
            let result = self.agent.agent_rx.as_mut().map(|rx| rx.try_recv());
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
                    let vm = MessageViewModel::tool_block(
                        "error".to_string(),
                        "agent-error".to_string(),
                        Some("agent channel disconnected unexpectedly".to_string()),
                        true,
                    );
                    self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                        self.langfuse.langfuse_flush_handle = Some(tracer.lock().on_trace_end(Some("ERROR: agent channel disconnected unexpectedly")));
                    }
                    self.langfuse.langfuse_tracer = None;
                    self.set_loading(false);
                    self.agent.agent_rx = None;
                    // 清理残留弹窗状态，避免 UI 卡在弹窗
                    self.agent.interaction_prompt = None;
                    self.agent.pending_hitl_items = None;
                    self.agent.pending_ask_user = None;
                    if let Some(start) = self.agent.task_start_time {
                        self.agent.last_task_duration = Some(start.elapsed());
                    }
                    return true;
                }
            }
        }

        updated
    }

    /// 每帧调用：检查 cron 触发事件，空闲时自动提交 prompt
    pub fn poll_cron_triggers(&mut self) {
        let cron_triggers: Vec<_> = self.cron.trigger_rx.as_mut()
            .map(|rx| {
                let mut triggers = Vec::new();
                while let Ok(trigger) = rx.try_recv() {
                    triggers.push(trigger);
                }
                triggers
            })
            .unwrap_or_default();
        for trigger in cron_triggers {
            if !self.core.loading {
                self.submit_message(trigger.prompt);
            } else {
                // Agent 正在执行，缓冲触发事件等待 Done 后自动发送
                tracing::debug!(prompt = %trigger.prompt, "cron trigger buffered (agent busy)");
                self.core.pending_messages.push(trigger.prompt);
            }
        }
    }

    /// 执行 micro-compact：清除旧工具结果，不调用 LLM
    pub fn start_micro_compact(&mut self) {
        use rust_create_agent::agent::compact::micro_compact_enhanced;
        let config = self.get_compact_config();
        let cleared = micro_compact_enhanced(&config, &mut self.agent.agent_state_messages);
        if cleared > 0 {
            tracing::info!(cleared, "micro-compact: enhanced compact completed");
            let vm = MessageViewModel::system(
                format!("Micro-compact: 清除了 {} 个旧工具结果", cleared)
            );
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        }
    }
}
