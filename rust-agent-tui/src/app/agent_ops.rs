use super::*;
use rust_agent_middlewares::hitl::BatchItem;

impl App {
    pub fn submit_message(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        // 消费待发送附件
        let attachments = std::mem::take(&mut self.pending_attachments);

        // 构建用于显示的文字（附件摘要追加在末尾）
        let display = if attachments.is_empty() {
            input.clone()
        } else {
            format!("{} [🖼 {} 张图片]", input, attachments.len())
        };
        let user_vm = MessageViewModel::user(display);
        self.view_messages.push(user_vm.clone());
        let _ = self.render_tx.send(RenderEvent::AddMessage(user_vm));
        self.set_loading(true);
        self.scroll_offset = u16::MAX;
        self.scroll_follow = true;
        self.todo_items.clear();

        // 开始计时新任务
        self.task_start_time = Some(std::time::Instant::now());
        self.last_task_duration = None;

        let provider = match self
            .zen_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config)
            .or_else(agent::LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                self.view_messages.push(MessageViewModel::tool_block(
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
        self.agent_rx = Some(rx);

        // 创建取消令牌（Ctrl+C 触发中断）
        let cancel = AgentCancellationToken::new();
        self.cancel_token = Some(cancel.clone());

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
        if self.langfuse_session.is_none() {
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
                self.langfuse_session = session.map(Arc::new);
            } else {
                tracing::debug!("langfuse: no config found in env, skipping session creation");
            }
        } else {
            tracing::debug!(thread_id = %thread_id, "langfuse: reusing existing session");
        }

        // 构造当前轮次的 Langfuse Tracer（同步，复用共享 Session）
        let langfuse_tracer = self.langfuse_session.clone().map(|session| {
            let mut t = crate::langfuse::LangfuseTracer::new(session);
            t.on_trace_start(input.trim());
            Arc::new(parking_lot::Mutex::new(t))
        });
        self.langfuse_tracer = langfuse_tracer.clone();

        let span = tracing::info_span!(
            "thread.run",
            thread.id = %thread_id,
            thread.cwd = %cwd,
        );
        let history = self.agent_state_messages.clone();
        let agent_id = self.agent_id.clone();
        let relay_client = self.relay_client.clone();
        let thread_store = self.thread_store.clone();
        let thread_id_for_agent = thread_id.clone();
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
                    relay_client,
                    langfuse_tracer,
                    thread_store,
                    thread_id: thread_id_for_agent,
                    preload_skills,
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
                let vm = MessageViewModel::subagent_group(agent_id, task_preview);
                self.view_messages.push(vm.clone());
                self.subagent_group_idx = Some(self.view_messages.len() - 1);
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                if let Some(idx) = self.subagent_group_idx {
                    if let Some(MessageViewModel::SubAgentGroup {
                        is_running,
                        final_result,
                        ..
                    }) = self.view_messages.get_mut(idx)
                    {
                        *is_running = false;
                        *final_result = Some(result);
                        let _ = is_error; // 错误时颜色由渲染层通过 is_error 体现
                    }
                    // 发送更新事件重绘
                    if let Some(vm) = self.view_messages.get(idx).cloned() {
                        let _ = self.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                    }
                    self.subagent_group_idx = None;
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
                if let Some(idx) = self.subagent_group_idx {
                    // 路由进 SubAgentGroup.recent_messages（滑动窗口 max 4）
                    if let Some(MessageViewModel::SubAgentGroup {
                        total_steps,
                        recent_messages,
                        ..
                    }) = self.view_messages.get_mut(idx)
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
                    if let Some(vm) = self.view_messages.get(idx).cloned() {
                        let _ = self.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                    }
                } else {
                    // 父 Agent 层：正常创建 ToolBlock
                    let vm = MessageViewModel::tool_block(name, display, args, is_error);
                    self.view_messages.push(vm.clone());
                    let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                }
                (true, false, false)
            }
            AgentEvent::MessageAdded(msg) => {
                // SubAgent 执行期间忽略 MessageAdded（不影响父 Agent 消息历史）
                if self.subagent_group_idx.is_some() {
                    return (false, false, false);
                }
                // AI 消息文本由紧随其后的 AiReasoning→AssistantChunk 事件处理，此处不处理
                let _ = msg;
                (true, false, false)
            }
            AgentEvent::AssistantChunk(chunk) => {
                if let Some(idx) = self.subagent_group_idx {
                    // 路由进 SubAgentGroup.recent_messages 的最后一个 AssistantBubble
                    if let Some(MessageViewModel::SubAgentGroup {
                        recent_messages,
                        total_steps,
                        ..
                    }) = self.view_messages.get_mut(idx)
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
                    if let Some(vm) = self.view_messages.get(idx).cloned() {
                        let _ = self.render_tx.send(RenderEvent::UpdateLastMessage(vm));
                    }
                } else {
                    match self.view_messages.last_mut() {
                        Some(m) if m.is_assistant() => m.append_chunk(&chunk),
                        _ => {
                            let vm = MessageViewModel::assistant();
                            self.view_messages.push(vm.clone());
                            let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                        }
                    }
                    let _ = self.render_tx.send(RenderEvent::AppendChunk(chunk));
                }
                (true, false, false)
            }
            AgentEvent::Done => {
                // 将最后一个 AssistantBubble 的 is_streaming 设为 false
                if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                    self.view_messages.last_mut()
                {
                    *is_streaming = false;
                }
                // 通知渲染线程清除流式指示器
                let _ = self.render_tx.send(RenderEvent::StreamingDone);
                // Langfuse：结束 Trace，上报最终答案（通过 TextChunk 事件累积，避免 UI 截断）
                if let Some(ref tracer) = self.langfuse_tracer {
                    self.langfuse_flush_handle = Some(tracer.lock().on_trace_end(None));
                }
                self.langfuse_tracer = None;
                self.set_loading(false);
                self.agent_rx = None;
                // 异常退出时兜底清空 subagent 状态
                self.subagent_group_idx = None;
                // Agent 异常退出时清理残留弹窗状态，避免 UI 卡在弹窗
                self.interaction_prompt = None;
                self.pending_hitl_items = None;
                self.pending_ask_user = None;
                if let Some(start) = self.task_start_time {
                    self.last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }
                (true, false, true)
            }
            AgentEvent::Interrupted => {
                // 中断：工具已以 error 结尾，持久化中间状态，下次发消息可断点续跑
                let vm = MessageViewModel::system(
                    "⚠ 已中断（工具调用已以 error 结尾，消息已保存，可继续发送恢复）".to_string(),
                );
                self.view_messages.push(vm.clone());
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                // Done 事件会紧随而来，由 Done 分支完成 set_loading + persist
                (true, false, false)
            }
            AgentEvent::Error(ref _e) => {
                let vm = MessageViewModel::tool_block(
                    "error".to_string(),
                    "agent-error".to_string(),
                    None,
                    true,
                );
                self.view_messages.push(vm.clone());
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                // Langfuse：错误路径也需结束 Trace，避免 Trace 在 Langfuse 侧永远显示为运行中
                if let Some(ref tracer) = self.langfuse_tracer {
                    self.langfuse_flush_handle = Some(tracer.lock().on_trace_end(Some(&format!("ERROR: {}", _e))));
                }
                self.langfuse_tracer = None;
                self.set_loading(false);
                self.agent_rx = None;
                // 兜底清空 subagent 状态
                self.subagent_group_idx = None;
                // Agent 出错时清理残留弹窗状态，避免 UI 卡在弹窗
                self.interaction_prompt = None;
                self.pending_hitl_items = None;
                self.pending_ask_user = None;
                if let Some(start) = self.task_start_time {
                    self.last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
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
                        // 转发 HITL 审批请求到 Relay（统一 interaction_request 消息）
                        if let Some(ref relay) = self.relay_client {
                            let relay_items: Vec<serde_json::Value> = batch_items
                                .iter()
                                .map(|item| serde_json::json!({ "tool_name": item.tool_name, "input": item.input }))
                                .collect();
                            relay.send_value(serde_json::json!({
                                "type": "interaction_request",
                                "ctx_type": "approval",
                                "items": relay_items
                            }));
                        }
                        self.interaction_prompt = Some(InteractionPrompt::Approval(HitlBatchPrompt::new(batch_items, bridge_tx)));
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
                        // 转发 AskUser 请求到 Relay（统一 interaction_request 消息）
                        self.pending_ask_user = Some(false);
                        if let Some(ref relay) = self.relay_client {
                            let questions_json: Vec<serde_json::Value> = ask_questions.iter().map(|q| {
                                serde_json::json!({
                                    "tool_call_id": q.tool_call_id,
                                    "question": q.question,
                                    "header": q.header,
                                    "multi_select": q.multi_select,
                                    "options": q.options.iter().map(|o| serde_json::json!({"label": o.label, "description": o.description})).collect::<Vec<_>>(),
                                })
                            }).collect();
                            relay.send_value(serde_json::json!({
                                "type": "interaction_request",
                                "ctx_type": "questions",
                                "questions": questions_json
                            }));
                        }
                        let (batch_req, _) = AskUserBatchRequest::new(ask_questions);
                        // 用桥接的 sender 覆盖 batch_req 的 response_tx
                        let batch_req_bridged = AskUserBatchRequest { questions: batch_req.questions, response_tx: bridge_tx };
                        self.interaction_prompt = Some(InteractionPrompt::Questions(AskUserBatchPrompt::from_request(batch_req_bridged)));
                        (true, true, false) // 暂停消费，等待用户输入
                    }
                }
            }
            AgentEvent::TodoUpdate(todos) => {
                // 转发 TODO 更新到 Relay
                if let Some(ref relay) = self.relay_client {
                    let items: Vec<serde_json::Value> = todos
                        .iter()
                        .map(|t| {
                            serde_json::json!({
                                "content": t.content,
                                "status": format!("{:?}", t.status),
                            })
                        })
                        .collect();
                    relay.send_value(serde_json::json!({
                        "type": "todo_update",
                        "items": items,
                    }));
                }
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
                self.agent_state_messages.extend(msgs);
                // 持久化已由 AgentState::add_message 自动完成（fire-and-forget）
                (true, false, false)
            }
            AgentEvent::CompactDone(summary) => {
                // 替换 LLM 历史为摘要（以 AI Message 形式写入，保留 system prompt 由 agent 注入）
                self.agent_state_messages = vec![BaseMessage::ai(summary.clone())];

                // 保留最近 10 条显示消息
                let keep_count = 10usize;
                if self.view_messages.len() > keep_count {
                    let tail = self
                        .view_messages
                        .split_off(self.view_messages.len() - keep_count);
                    self.view_messages = tail;
                }

                // 头部插入压缩提示
                let compact_vm = MessageViewModel::system(format!(
                    "📦 上下文已压缩（保留最近 {} 条显示消息，LLM 历史已替换为摘要）",
                    keep_count
                ));
                self.view_messages.insert(0, compact_vm);

                // 尾部追加摘要内容（可见）
                let summary_vm = MessageViewModel::system(format!("📋 压缩摘要：\n{}", summary));
                self.view_messages.push(summary_vm);

                // 通知渲染线程重建显示
                let _ = self
                    .render_tx
                    .send(crate::ui::render_thread::RenderEvent::Clear);
                for vm in &self.view_messages {
                    let _ = self
                        .render_tx
                        .send(crate::ui::render_thread::RenderEvent::AddMessage(
                            vm.clone(),
                        ));
                }

                self.set_loading(false);
                self.agent_rx = None;

                // 通知 Relay Web 前端：compact 完成，推送压缩后的 LLM 上下文
                if let Some(ref relay) = self.relay_client {
                    let msg_vals: Vec<serde_json::Value> = self.agent_state_messages
                        .iter()
                        .filter_map(|m| serde_json::to_value(m).ok())
                        .collect();
                    relay.send_thread_reset(&msg_vals);
                }

                // 刷新 compact 期间缓冲的消息（与 Done 分支行为一致）
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
            AgentEvent::CompactError(msg) => {
                let vm = MessageViewModel::system(format!("❌ 压缩失败: {}", msg));
                self.view_messages.push(vm.clone());
                let _ = self
                    .render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                self.set_loading(false);
                self.agent_rx = None;

                // 刷新 compact 期间缓冲的消息
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
        }
    }

    /// 每帧调用：消费 channel 事件，返回是否有 UI 更新
    pub fn poll_agent(&mut self) -> bool {
        if self.agent_rx.is_none() {
            return false;
        }

        let mut updated = false;

        loop {
            // 先 try_recv 拿到事件（短暂借用 rx），立即释放借用
            let result = self.agent_rx.as_mut().map(|rx| rx.try_recv());
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
                        None,
                        true,
                    );
                    self.view_messages.push(vm.clone());
                    let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                    // Langfuse：channel 意外断开也需结束 Trace，与 Error 路径保持一致
                    if let Some(ref tracer) = self.langfuse_tracer {
                        self.langfuse_flush_handle = Some(tracer.lock().on_trace_end(Some("ERROR: agent channel disconnected unexpectedly")));
                    }
                    self.langfuse_tracer = None;
                    self.set_loading(false);
                    self.agent_rx = None;
                    // 兜底清空 subagent 状态
                    self.subagent_group_idx = None;
                    // 清理残留弹窗状态，避免 UI 卡在弹窗
                    self.interaction_prompt = None;
                    self.pending_hitl_items = None;
                    self.pending_ask_user = None;
                    if let Some(start) = self.task_start_time {
                        self.last_task_duration = Some(start.elapsed());
                    }
                    return true;
                }
            }
        }

        updated
    }
}
