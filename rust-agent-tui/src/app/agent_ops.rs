use super::*;
use rust_agent_middlewares::hitl::BatchItem;
use crate::ui::message_view::{ToolCategory, ToolEntry};

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
        self.core.view_messages.push(user_vm.clone());
        self.core.last_human_message = Some(display);
        let _ = self.core.render_tx.send(RenderEvent::AddMessage(user_vm));
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
                self.core.view_messages.push(MessageViewModel::tool_block(
                    "error".to_string(),
                    "config-error".to_string(),
                    None,
                    true,
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

    /// 处理单个 AgentEvent，返回 `(updated, should_break, should_return)`
    pub(crate) fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
        match event {
            AgentEvent::SubAgentStart { agent_id, task_preview } => {
                // Langfuse：创建 SubAgent Observation（与主 agent 共享 trace_id）
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    tracer.lock().on_subagent_start(&agent_id, &task_preview);
                }
                let vm = MessageViewModel::subagent_group(agent_id, task_preview);
                self.core.view_messages.push(vm.clone());
                self.core.subagent_group_idx = Some(self.core.view_messages.len() - 1);
                let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                // Langfuse：结束 SubAgent Observation
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    tracer.lock().on_subagent_end(&result, is_error);
                }
                if let Some(idx) = self.core.subagent_group_idx {
                    if let Some(MessageViewModel::SubAgentGroup {
                        is_running,
                        final_result,
                        ..
                    }) = self.core.view_messages.get_mut(idx)
                    {
                        *is_running = false;
                        *final_result = Some(result);
                        let _ = is_error; // 错误时颜色由渲染层通过 is_error 体现
                    }
                    // 发送更新事件重绘
                    if let Some(vm) = self.core.view_messages.get(idx).cloned() {
                        let _ = self.core.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                    }
                    self.core.subagent_group_idx = None;
                }
                (true, false, false)
            }
            AgentEvent::TokenUsageUpdate { usage, model: _model } => {
                // 累积到会话追踪器
                self.agent.session_token_tracker.accumulate(&usage);
                // 更新 spinner 的 token 显示
                let total = self.agent.session_token_tracker.total_input_tokens
                    + self.agent.session_token_tracker.total_output_tokens;
                self.spinner_state.set_token_count(total as usize);
                // circuit breaker: 连续 3 次失败后不再自动触发
                if self.agent.auto_compact_failures < 3 {
                    let budget = rust_create_agent::agent::token::ContextBudget::new(self.agent.context_window);
                    if budget.should_auto_compact(&self.agent.session_token_tracker) {
                        self.agent.needs_auto_compact = true;
                    }
                }
                (true, false, false)
            }
            AgentEvent::ToolCall {
                tool_call_id: _tool_call_id,
                name,
                display,
                args,
                is_error,
            } => {
                self.agent.retry_status = None;
                // 切换 spinner 到 ToolUse 模式，动词显示工具名+参数摘要
                self.spinner_state.set_mode(perihelion_widgets::SpinnerMode::ToolUse);
                let verb_text = match args.as_deref() {
                    Some(a) if !a.is_empty() => {
                        let summary: String = a.chars().take(40).collect();
                        format!("{} {}", display, summary)
                    }
                    _ => format!("{}…", display),
                };
                self.spinner_state.set_verb(Some(&verb_text));
                if let Some(idx) = self.core.subagent_group_idx {
                    // 路由进 SubAgentGroup.recent_messages（滑动窗口 max 4）
                    if let Some(MessageViewModel::SubAgentGroup {
                        total_steps,
                        recent_messages,
                        ..
                    }) = self.core.view_messages.get_mut(idx)
                    {
                        *total_steps += 1;
                        if recent_messages.len() >= 4 {
                            recent_messages.remove(0);
                        }
                        recent_messages.push(MessageViewModel::tool_block(
                            name, display, args, is_error,
                        ));
                    }
                    // 发送更新事件重绘最后一条消息（SubAgentGroup）
                    if let Some(vm) = self.core.view_messages.get(idx).cloned() {
                        let _ = self.core.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                    }
                } else {
                    // 父 Agent 层：检查是否可聚合到现有 ToolCallGroup
                    if let Some(cat) = ToolCategory::from_tool_name(&name) {
                        // 只读工具：尝试聚合到附近的 ToolCallGroup（跳过中间的空 thinking bubble，允许跨类别合并）
                        let mut thinking_count = 0;
                        let mut found_group = false;

                        for vm in self.core.view_messages.iter().rev() {
                            if vm.is_reasoning_only() {
                                thinking_count += 1;
                            } else if matches!(vm, MessageViewModel::ToolCallGroup { .. }) {
                                found_group = true;
                                break;
                            } else {
                                break;
                            }
                        }

                        if found_group {
                            // 移除中间的空 thinking bubbles（它们在 UI 上不可见，只阻隔了分组合并）
                            for _ in 0..thinking_count {
                                self.core.view_messages.pop();
                                let _ = self.core.render_tx.send(RenderEvent::RemoveLastMessage);
                            }

                            // 现在 last 是 ToolCallGroup，添加新工具（保持原有 category）
                            if let Some(MessageViewModel::ToolCallGroup { tools, .. }) =
                                self.core.view_messages.last_mut()
                            {
                                tools.push(ToolEntry {
                                    tool_name: name.clone(),
                                    display_name: display.clone(),
                                    args_display: args.clone(),
                                    content: String::new(),
                                    is_error,
                                });
                            }
                            if let Some(vm) = self.core.view_messages.last().cloned() {
                                let _ = self.core.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                            }
                        } else {
                            // 创建新的 ToolCallGroup
                            let vm = MessageViewModel::ToolCallGroup {
                                category: cat,
                                tools: vec![ToolEntry {
                                    tool_name: name.clone(),
                                    display_name: display.clone(),
                                    args_display: args.clone(),
                                    content: String::new(),
                                    is_error,
                                }],
                                collapsed: true,
                            };
                            self.core.view_messages.push(vm.clone());
                            let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                        }
                    } else {
                        // 非只读工具：保持原逻辑
                        let vm = MessageViewModel::tool_block(name, display, args, is_error);
                        self.core.view_messages.push(vm.clone());
                        let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                    }
                }
                (true, false, false)
            }
            AgentEvent::AssistantChunk(chunk) => {
                self.agent.retry_status = None;
                // 切换 spinner 到 Responding 模式
                self.spinner_state.set_mode(perihelion_widgets::SpinnerMode::Responding);
                if let Some(idx) = self.core.subagent_group_idx {
                    // 路由进 SubAgentGroup.recent_messages 的最后一个 AssistantBubble
                    if let Some(MessageViewModel::SubAgentGroup {
                        recent_messages,
                        total_steps,
                        ..
                    }) = self.core.view_messages.get_mut(idx)
                    {
                        match recent_messages.last_mut() {
                            Some(m) if m.is_assistant() => m.append_chunk(&chunk),
                            _ => {
                                // 新建 AssistantBubble，先维护滑动窗口
                                if recent_messages.len() >= 4 {
                                    recent_messages.remove(0);
                                } else {
                                    *total_steps += 1;
                                }
                                let mut bubble = MessageViewModel::assistant();
                                bubble.append_chunk(&chunk);
                                recent_messages.push(bubble);
                            }
                        }
                    }
                    // 发送更新事件重绘
                    if let Some(vm) = self.core.view_messages.get(idx).cloned() {
                        let _ = self.core.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                    }
                } else {
                    // 如果 chunk 为空且没有现有的 assistant bubble，跳过创建空的 bubble
                    // 避免 AI 只发起工具调用时显示空白消息
                    if chunk.is_empty() {
                        match self.core.view_messages.last_mut() {
                            Some(m) if m.is_assistant() => m.append_chunk(&chunk),
                            _ => {
                                // 没有现有的 assistant bubble，chunk 为空，不创建新的空 bubble
                            }
                        }
                    } else {
                        match self.core.view_messages.last_mut() {
                            Some(m) if m.is_assistant() => m.append_chunk(&chunk),
                            _ => {
                                let vm = MessageViewModel::assistant();
                                self.core.view_messages.push(vm.clone());
                                let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                            }
                        }
                        let _ = self.core.render_tx.send(RenderEvent::AppendChunk(chunk));
                    }
                }
                (true, false, false)
            }
            AgentEvent::Done => {
                self.agent.retry_status = None;
                // 将最后一个 AssistantBubble 的 is_streaming 设为 false
                if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                    self.core.view_messages.last_mut()
                {
                    *is_streaming = false;
                }
                // 通知渲染线程清除流式指示器
                let _ = self.core.render_tx.send(RenderEvent::StreamingDone);
                // Langfuse：结束 Trace，上报最终答案（通过 TextChunk 事件累积，避免 UI 截断）
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
                    // 70%-85% 区间: micro-compact
                    let budget = rust_create_agent::agent::token::ContextBudget::new(self.agent.context_window);
                    if budget.should_warn(&self.agent.session_token_tracker) {
                        self.start_micro_compact();
                    }
                }
                // 异常退出时兜底清空 subagent 状态
                self.core.subagent_group_idx = None;
                // Agent 异常退出时清理残留弹窗状态，避免 UI 卡在弹窗
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
            AgentEvent::Interrupted => {
                // 中断：工具已以 error 结尾，持久化中间状态，下次发消息可断点续跑
                let vm = MessageViewModel::system(
                    "⚠ 已中断（工具调用已以 error 结尾，消息已保存，可继续发送恢复）".to_string(),
                );
                self.core.view_messages.push(vm.clone());
                let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                // Done 事件会紧随而来，由 Done 分支完成 set_loading + persist
                (true, false, false)
            }
            AgentEvent::Error(ref e) => {
                let vm = MessageViewModel::tool_block(
                    "error".to_string(),
                    "agent-error".to_string(),
                    Some(e.clone()),
                    true,
                );
                self.core.view_messages.push(vm.clone());
                let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                // Langfuse：错误路径也需结束 Trace，避免 Trace 在 Langfuse 侧永远显示为运行中
                if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                    self.langfuse.langfuse_flush_handle = Some(tracer.lock().on_trace_end(Some(&format!("ERROR: {}", e))));
                }
                self.langfuse.langfuse_tracer = None;
                self.set_loading(false);
                self.agent.agent_rx = None;
                // 兜底清空 subagent 状态
                self.core.subagent_group_idx = None;
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
                        // 桥接：将 ApprovalItem 列表转为旧式 BatchItem + 转换响应类型
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
                        // 桥接：将 QuestionItem 列表转为 AskUserQuestionData + 转换响应类型
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
                        // 构建 AskUser 批量请求
                        self.agent.pending_ask_user = Some(false);
                        // 在消息流中显示 AI 向用户提出了什么问题
                        {
                            let q_lines: Vec<String> = requests.iter().map(|q| {
                                let hint = if q.multi_select { " [多选]" } else { " [单选]" };
                                format!("{}{}: {}", q.header, hint, q.question)
                            }).collect();
                            let vm = MessageViewModel::system(format!("❓ 向你提问:\n{}", q_lines.join("\n")));
                            self.core.view_messages.push(vm.clone());
                            let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                        }
                        let (batch_req, _) = AskUserBatchRequest::new(ask_questions);
                        // 用桥接的 sender 覆盖 batch_req 的 response_tx
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
                        BaseMessage::Ai {
                            content: _,
                            tool_calls,
                            ..
                        } => {
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
                // 增量追加到 agent_state_messages（去重，避免重复消息）
                self.agent.agent_state_messages.extend(msgs);
                // 持久化已由 AgentState::add_message 自动完成（fire-and-forget）
                (true, false, false)
            }
            AgentEvent::CompactDone { summary, new_thread_id: _ } => {
                // 创建新 Thread，带摘要截断名称
                let truncated: String = summary.chars().take(30).collect();
                let ellipsis = if summary.chars().count() > 30 { "…" } else { "" };
                let thread_title = format!("📦 Compact: {}{}", truncated, ellipsis);
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

                // 构造新 Thread 的消息：Ai(摘要) — 以 AI 消息形式展示摘要
                let new_messages = vec![BaseMessage::ai(summary.clone())];

                // 持久化新 Thread 消息
                let store = self.thread_store.clone();
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(store.append_messages(&new_tid, &new_messages))
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, thread_id = %new_tid, "compact: 持久化新 thread 消息失败");
                        });
                });

                // 切换到新 Thread
                self.current_thread_id = Some(new_tid.clone());
                self.agent.agent_state_messages = new_messages;

                // 清空显示消息，插入压缩提示 + 摘要（AI 消息形式）
                self.core.view_messages.clear();
                let compact_vm = MessageViewModel::system(
                    "📦 上下文已压缩（从旧对话迁移到新 Thread）".to_string(),
                );
                self.core.view_messages.push(compact_vm);
                let summary_vm = MessageViewModel::from_base_message(
                    &BaseMessage::ai(format!("📋 压缩摘要：\n{}", summary)),
                    &[],
                );
                self.core.view_messages.push(summary_vm);

                // 通知渲染线程重建显示
                let _ = self
                    .core.render_tx
                    .send(crate::ui::render_thread::RenderEvent::Clear);
                for vm in &self.core.view_messages {
                    let _ = self
                        .core.render_tx
                        .send(crate::ui::render_thread::RenderEvent::AddMessage(
                            vm.clone(),
                        ));
                }

                self.set_loading(false);
                self.agent.agent_rx = None;

                // 重置 Langfuse session（新 Thread 需要独立 session）
                self.langfuse.langfuse_session = None;
                self.agent.auto_compact_failures = 0;

                // 刷新 compact 期间缓冲的消息（与 Done 分支行为一致）
                if !self.core.pending_messages.is_empty() {
                    let combined = self.core.pending_messages.join("\n\n");
                    self.core.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
            AgentEvent::CompactError(msg) => {
                let vm = MessageViewModel::system(format!("❌ 压缩失败: {}", msg));
                self.core.view_messages.push(vm.clone());
                let _ = self
                    .core.render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                self.set_loading(false);
                self.agent.agent_rx = None;
                self.agent.auto_compact_failures += 1;

                // 刷新 compact 期间缓冲的消息
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
            // 先 try_recv 拿到事件（短暂借用 rx），立即释放借用
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
                    self.core.view_messages.push(vm.clone());
                    let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
                    // Langfuse：channel 意外断开也需结束 Trace，与 Error 路径保持一致
                    if let Some(ref tracer) = self.langfuse.langfuse_tracer {
                        self.langfuse.langfuse_flush_handle = Some(tracer.lock().on_trace_end(Some("ERROR: agent channel disconnected unexpectedly")));
                    }
                    self.langfuse.langfuse_tracer = None;
                    self.set_loading(false);
                    self.agent.agent_rx = None;
                    // 兜底清空 subagent 状态
                    self.core.subagent_group_idx = None;
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
            }
        }
    }

    /// 执行 micro-compact：清除旧工具结果，不调用 LLM
    pub fn start_micro_compact(&mut self) {
        use rust_create_agent::agent::token::micro_compact;
        let cleared = micro_compact(&mut self.agent.agent_state_messages, 10);
        if cleared > 0 {
            tracing::info!(cleared, "micro-compact: cleared old tool results");
            let vm = MessageViewModel::system(
                format!("📦 Micro-compact: 清除了 {} 个旧工具结果", cleared)
            );
            self.core.view_messages.push(vm.clone());
            let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
        }
    }
}
