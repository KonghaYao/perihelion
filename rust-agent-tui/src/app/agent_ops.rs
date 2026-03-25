use super::*;

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

        // YOLO_MODE 时跳过 HITL channel，直接给 agent 一个永远不会被消费的 sender
        let yolo = rust_agent_middlewares::is_yolo_mode();

        let (approval_tx, approval_rx) = mpsc::channel::<ApprovalEvent>(4);
        {
            let tx_hitl = tx.clone();
            tokio::spawn(async move {
                let mut approval_rx = approval_rx;
                while let Some(ev) = approval_rx.recv().await {
                    match ev {
                        ApprovalEvent::Batch(req) => {
                            if yolo {
                                // YOLO 模式：跳过弹窗，直接全部批准
                                let decisions = vec![HitlDecision::Approve; req.items.len()];
                                let _ = req.response_tx.send(decisions);
                            } else {
                                let _ = tx_hitl.send(AgentEvent::ApprovalNeeded(req)).await;
                            }
                        }
                        ApprovalEvent::AskUserBatch(req) => {
                            let _ = tx_hitl.send(AgentEvent::AskUserBatch(req)).await;
                        }
                    }
                }
            });
        }

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

        // 确保当前 thread 存在
        let thread_id = self.ensure_thread_id();
        // 用户消息将由 agent 执行结束时的 StateSnapshot 统一持久化，
        // 避免与 StateSnapshot 竞争写 DB seq 导致序号错乱/重复写入
        // 用户消息已追加到 self.view_messages，更新已持久化计数
        self.persisted_count = self.view_messages.len();

        // 懒加载 Thread 级 LangfuseSession（首轮创建，后续复用；未配置环境变量时静默跳过）
        if self.langfuse_session.is_none() {
            tracing::info!(thread_id = %thread_id, "langfuse: session is None, attempting to create");
            if let Some(cfg) = crate::langfuse::LangfuseConfig::from_env() {
                tracing::info!(host = %cfg.host, "langfuse: config found, creating session");
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
                tracing::info!("langfuse: no config found in env, skipping session creation");
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
        tokio::spawn(
            async move {
                agent::run_universal_agent(agent::AgentRunConfig {
                    provider,
                    input: agent_input,
                    cwd,
                    history,
                    approval_tx,
                    tx,
                    cancel,
                    agent_id,
                    relay_client,
                    langfuse_tracer,
                })
                .await;
            }
            .instrument(span),
        );
    }

    /// 处理单个 AgentEvent，返回 `(updated, should_break, should_return)`
    pub(crate) fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
        match event {
            AgentEvent::ToolCall {
                tool_call_id: _tool_call_id,
                name,
                display,
                args,
                is_error,
            } => {
                let vm = MessageViewModel::tool_block(name, display, args, is_error);
                self.view_messages.push(vm.clone());
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::MessageAdded(msg) => {
                // 只处理工具调用消息的渲染；纯文本 AI 消息由 AssistantChunk 处理
                if let rust_create_agent::messages::BaseMessage::Ai {
                        content,
                        tool_calls,
                        ..
                    } = msg {
                    // 工具调用消息需要同步到 UI（折叠状态、工具调用列表）
                    if !tool_calls.is_empty() {
                        let text = match &content {
                            rust_create_agent::messages::MessageContent::Text(t) => t.clone(),
                            rust_create_agent::messages::MessageContent::Blocks(blocks) => blocks
                                .iter()
                                .filter_map(|b| match b {
                                    rust_create_agent::messages::ContentBlock::Text { text } => {
                                        Some(text.clone())
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join(""),
                            _ => String::new(),
                        };

                        match self.view_messages.last_mut() {
                            Some(m) if m.is_assistant() => {
                                // 追加文本到现有的 assistant 消息
                                if !text.is_empty() {
                                    if let MessageViewModel::AssistantBubble {
                                        blocks, ..
                                    } = m
                                    {
                                        blocks.push(ContentBlockView::Text {
                                            raw: text.clone(),
                                            rendered: parse_markdown(&text),
                                            dirty: false,
                                        });
                                    }
                                }
                            }
                            _ => {
                                // 创建新的 assistant 消息（折叠状态）
                                let mut vm = MessageViewModel::assistant();
                                if let MessageViewModel::AssistantBubble {
                                    collapsed,
                                    blocks,
                                    ..
                                } = &mut vm
                                {
                                    *collapsed = true;
                                    if !text.is_empty() {
                                        blocks.push(ContentBlockView::Text {
                                            raw: text.clone(),
                                            rendered: parse_markdown(&text),
                                            dirty: false,
                                        });
                                    }
                                }
                                self.view_messages.push(vm.clone());
                                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                            }
                        }
                    }
                    // 纯文本 AI 消息由 AssistantChunk 事件处理，此处不重复渲染
                }
                (true, false, false)
            }
            AgentEvent::AssistantChunk(chunk) => {
                match self.view_messages.last_mut() {
                    Some(m) if m.is_assistant() => m.append_chunk(&chunk),
                    _ => {
                        let vm = MessageViewModel::assistant();
                        self.view_messages.push(vm.clone());
                        let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                    }
                }
                let _ = self.render_tx.send(RenderEvent::AppendChunk(chunk));
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
                    tracer.lock().on_trace_end(None);
                }
                self.langfuse_tracer = None;
                self.set_loading(false);
                self.agent_rx = None;
                // Agent 异常退出时清理残留弹窗状态，避免 UI 卡在弹窗
                self.hitl_prompt = None;
                self.ask_user_prompt = None;
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
                    tracer.lock().on_trace_end(Some(&format!("ERROR: {}", _e)));
                }
                self.langfuse_tracer = None;
                self.set_loading(false);
                self.agent_rx = None;
                // Agent 出错时清理残留弹窗状态，避免 UI 卡在弹窗
                self.hitl_prompt = None;
                self.ask_user_prompt = None;
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
            AgentEvent::ApprovalNeeded(req) => {
                // 转发 HITL 审批请求到 Relay
                if let Some(ref relay) = self.relay_client {
                    let items: Vec<serde_json::Value> = req
                        .items
                        .iter()
                        .map(|item| {
                            serde_json::json!({
                                "tool_name": item.tool_name,
                                "input": item.input,
                            })
                        })
                        .collect();
                    relay.send_value(serde_json::json!({
                        "type": "approval_needed",
                        "items": items,
                    }));
                }
                self.hitl_prompt = Some(HitlBatchPrompt::new(req.items, req.response_tx));
                (true, true, false) // 暂停消费，等待用户确认
            }
            AgentEvent::AskUserBatch(req) => {
                // 转发 AskUser 请求到 Relay
                self.pending_ask_user = Some(false);
                if let Some(ref relay) = self.relay_client {
                    let questions: Vec<serde_json::Value> = req.questions.iter().map(|q| {
                        serde_json::json!({
                            "question": q.description,
                            "options": q.options.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
                            "multi_select": q.multi_select,
                        })
                    }).collect();
                    relay.send_value(serde_json::json!({
                        "type": "ask_user_batch",
                        "questions": questions,
                    }));
                }
                self.ask_user_prompt = Some(AskUserBatchPrompt::from_request(req));
                (true, true, false) // 暂停消费，等待用户输入
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
                let start = self.agent_state_messages.len();
                self.agent_state_messages.extend(msgs);

                // 增量持久化到 thread（从上次持久化位置之后的所有消息）
                if let Some(id) = self.current_thread_id.clone() {
                    let new_msgs: Vec<_> = self.agent_state_messages[start..]
                        .iter()
                        .filter(|m| !matches!(m, BaseMessage::System { .. }))
                        .cloned()
                        .collect();
                    if !new_msgs.is_empty() {
                        let store = self.thread_store.clone();
                        let tid = id.clone();
                        tokio::spawn(async move {
                            if let Err(e) = store.append_messages(&tid, &new_msgs).await {
                                tracing::warn!(
                                    thread_id = %tid,
                                    msg_count = new_msgs.len(),
                                    error = %e,
                                    "StateSnapshot 持久化写入失败"
                                );
                            }
                        });
                    }
                }
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

                // 重置持久化计数（view_messages 已重建）
                self.persisted_count = 0;

                self.set_loading(false);
                self.agent_rx = None;

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
                    self.set_loading(false);
                    self.agent_rx = None;
                    return true;
                }
            }
        }

        updated
    }
}
