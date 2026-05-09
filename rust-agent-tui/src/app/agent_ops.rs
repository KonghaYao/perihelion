use super::message_pipeline::PipelineAction;
use super::plugin_panel::PluginPanel;
use super::*;
use rust_agent_middlewares::hitl::BatchItem;

/// 从输入文本中提取 `/skill-name` 格式的 token（字母、数字、连字符、下划线）
fn extract_skill_tokens(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter(|token| token.starts_with('/') && token.len() > 1)
        .map(|token| {
            let name = token.trim_start_matches('/');
            name.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

impl App {
    pub fn submit_message(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        // 记录提交前的状态长度，用于中断时回滚 agent_state_messages
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .pre_submit_state_len = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .len();

        self.push_input_history(input.clone());

        // 消费待发送附件
        let attachments = std::mem::take(
            &mut self.session_mgr.sessions[self.session_mgr.active]
                .metadata
                .pending_attachments,
        );

        // 构建用于显示的文字（附件摘要追加在末尾）
        let display = if attachments.is_empty() {
            input.clone()
        } else {
            format!("{} [🖼 {} 张图片]", input, attachments.len())
        };
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .round_start_vm_idx = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .len();
        let user_vm = MessageViewModel::user(display.clone());
        self.apply_pipeline_action(PipelineAction::AddMessage(user_vm));
        self.session_mgr.sessions[self.session_mgr.active]
            .metadata
            .last_human_message = Some(display);
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .last_submitted_text = Some(input.clone());
        self.set_loading(true);
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_offset = u16::MAX;
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .scroll_follow = true;
        self.session_mgr.sessions[self.session_mgr.active]
            .todo_items
            .clear();

        // 开始计时新任务
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .task_start_time = Some(std::time::Instant::now());
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .last_task_duration = None;
        if self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .session_start_time
            .is_none()
        {
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .session_start_time = Some(std::time::Instant::now());
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
                self.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::system(
                    "未配置 API Key，请输入 /login 配置 Provider".to_string(),
                )));
                self.set_loading(false);
                return;
            }
        };

        // 防御性重置：上次 agent 任务若 SubAgentEnd 因通道溢出被丢弃，
        // subagent_depth 会永久 > 0，导致所有后续 TokenUsageUpdate 被过滤（ctx 显示为 0）
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .subagent_depth = 0;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_replied = false;
        // 清理后台任务 continuation 状态（用户主动发消息时覆盖自动 continuation）
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_done_pending_bg = false;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pending_bg_continuation = None;

        let (tx, rx) = mpsc::channel(64);
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_rx = Some(rx);

        // 创建取消令牌（Ctrl+C 触发中断）
        let cancel = AgentCancellationToken::new();
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .cancel_token = Some(cancel.clone());

        // 注意：HITL 审批和 AskUser 问答现在统一通过 TuiInteractionBroker 路由到 tx channel，
        // YOLO 模式由 HumanInTheLoopMiddleware::from_env() 内部处理（自动放行）。

        let cwd = self.services.cwd.clone();

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

        // 解析消息中的 /skill-name（字母、数字、连字符、下划线）
        let preload_skills = extract_skill_tokens(&input);

        // 确保当前 thread 存在
        let thread_id = self.ensure_thread_id();

        // 懒加载 Thread 级 LangfuseSession（首轮创建，后续复用；未配置环境变量时静默跳过）
        if self.session_mgr.sessions[self.session_mgr.active]
            .langfuse
            .langfuse_session
            .is_none()
        {
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
                self.session_mgr.sessions[self.session_mgr.active]
                    .langfuse
                    .langfuse_session = session.map(Arc::new);
            } else {
                tracing::debug!("langfuse: no config found in env, skipping session creation");
            }
        } else {
            tracing::debug!(thread_id = %thread_id, "langfuse: reusing existing session");
        }

        // 构造当前轮次的 Langfuse Tracer（同步，复用共享 Session）
        let langfuse_tracer = self.session_mgr.sessions[self.session_mgr.active]
            .langfuse
            .langfuse_session
            .clone()
            .map(|session| {
                let mut t = crate::langfuse::LangfuseTracer::new(session);
                t.on_trace_start(input.trim());
                Arc::new(parking_lot::Mutex::new(t))
            });
        self.session_mgr.sessions[self.session_mgr.active]
            .langfuse
            .langfuse_tracer = langfuse_tracer.clone();

        let span = tracing::info_span!(
            "thread.run",
            thread.id = %thread_id,
            thread.cwd = %cwd,
        );
        let history = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .clone();
        let agent_id = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_id
            .clone();
        let thread_store = self.services.thread_store.clone();
        let thread_id_for_agent = thread_id.clone();
        let peri_config_for_agent = Arc::new(self.services.peri_config.clone().unwrap_or_default());
        let cron_scheduler = Some(self.services.cron.scheduler.clone());
        let permission_mode = self.services.permission_mode.clone();

        let mcp_pool = self.services.mcp_pool.clone();
        let plugin_skill_dirs = self
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_skill_dirs.clone())
            .unwrap_or_default();
        let plugin_agent_dirs = self
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_agent_dirs.clone())
            .unwrap_or_default();
        let mut plugin_hooks = self
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_hooks.clone())
            .unwrap_or_default();
        let local_hooks =
            rust_agent_middlewares::hooks::loader::load_settings_local_hooks(&self.services.cwd);

        // hook_groups：每组对应一个独立的 HookMiddleware 实例
        // plugin hooks 和 settings.local hooks 分组，便于独立控制
        let mut hook_groups: Vec<Vec<rust_agent_middlewares::hooks::RegisteredHook>> = Vec::new();
        if !plugin_hooks.is_empty() {
            tracing::info!(count = plugin_hooks.len(), "Registering plugin hooks");
            hook_groups.push(std::mem::take(&mut plugin_hooks));
        }
        if !local_hooks.is_empty() {
            tracing::info!(
                count = local_hooks.len(),
                "Registering settings.local hooks"
            );
            hook_groups.push(local_hooks);
        }

        // 扁平化所有 hooks 供 SubAgentTool 和 SessionEnd 使用
        let all_hooks: Vec<rust_agent_middlewares::hooks::RegisteredHook> =
            hook_groups.iter().flatten().cloned().collect();

        tracing::info!(
            groups = hook_groups.len(),
            total = all_hooks.len(),
            "Hook groups assembled for agent"
        );

        let hook_session_start = history.is_empty();

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
                    config: peri_config_for_agent,
                    cron_scheduler,
                    permission_mode,
                    mcp_pool,
                    plugin_skill_dirs,
                    plugin_agent_dirs,
                    plugin_hooks: all_hooks,
                    hook_groups,
                    hook_session_start,
                })
                .await;
            }
            .instrument(span),
        );
    }

    /// 发送缓冲的 cron 消息（每次只发一条，其余留待后续 Done 周期发送）
    /// 多条独立 cron 任务不应合并为一个 LLM 消息，避免语义混淆
    fn flush_pending_messages(&mut self) {
        if let Some(msg) = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pending_messages
            .first()
            .cloned()
        {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pending_messages
                .remove(0);
            self.submit_message(msg);
        }
    }

    /// 将 PipelineAction 映射到 view_messages 更新 + RenderEvent 发送
    fn apply_pipeline_action(&mut self, action: PipelineAction) {
        match action {
            PipelineAction::None => {}
            PipelineAction::AddMessage(vm) => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AddMessage(vm));
            }
            PipelineAction::AppendChunk(chunk) => {
                match self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .last_mut()
                {
                    Some(m) if m.is_assistant() => {
                        m.append_chunk(&chunk);
                    }
                    _ => {
                        // 首个 chunk：创建带内容的 assistant bubble，通过 AddMessage 通知渲染线程
                        let mut vm = MessageViewModel::assistant();
                        vm.append_chunk(&chunk);
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages
                            .push(vm.clone());
                        let _ = self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .render_tx
                            .send(RenderEvent::AddMessage(vm));
                        return;
                    }
                }
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AppendChunk(chunk));
            }
            PipelineAction::UpdateLast(vm) => {
                if let Some(last) = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .last_mut()
                {
                    *last = vm.clone();
                } else {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages
                        .push(vm.clone());
                }
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::UpdateLastMessage(vm));
            }
            PipelineAction::RemoveLast => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .pop();
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::RemoveLastMessage);
            }
            PipelineAction::UpdateToolResult { tool_call_id, vm } => {
                // 按 tool_call_id 精确查找 ToolBlock（并行工具调用时避免 UpdateLast 互相覆盖）
                let idx = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .iter()
                    .position(|m| {
                        if let MessageViewModel::ToolBlock {
                            tool_call_id: tc_id,
                            ..
                        } = m
                        {
                            tc_id == &tool_call_id
                        } else {
                            false
                        }
                    });
                if let Some(idx) = idx {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages[idx] = (*vm).clone();
                    // AskUserQuestion 结束后聚合尾部（多个并行调用合并为一个块）
                    if matches!(&*vm, MessageViewModel::ToolBlock { tool_name, .. } if tool_name == "AskUserQuestion")
                    {
                        let aggregate_from = idx.saturating_sub(1);
                        aggregate_tail_tool_groups(
                            &mut self.session_mgr.sessions[self.session_mgr.active]
                                .messages
                                .view_messages,
                            aggregate_from,
                        );
                    }
                } else {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages
                        .push((*vm).clone());
                }
                // 刷新渲染（用 LoadHistory 保证渲染线程同步）
                let msgs = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .clone();
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::LoadHistory(msgs));
            }
            PipelineAction::RemoveLastN(n) => {
                for _ in 0..n {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages
                        .pop();
                }
                for _ in 0..n {
                    let _ = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .render_tx
                        .send(RenderEvent::RemoveLastMessage);
                }
            }
            PipelineAction::RebuildAll {
                prefix_len,
                tail_vms,
            } => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .truncate(prefix_len);
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .extend(tail_vms.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::LoadHistory(
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages
                            .clone(),
                    ));
            }
        }
    }

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
                    tracer.lock().on_subagent_start(&agent_id, &task_preview);
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
                (true, false, false)
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
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
                    tracer.lock().on_subagent_end(&result, is_error);
                }
                // Pipeline：更新 SubAgentGroup（is_running=false, final_result）
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::SubAgentEnd { result, is_error });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::ContextWarning {
                used_tokens: _,
                total_tokens: _,
                percentage: _,
            } => {
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
            } => {
                // 关闭 MCP 面板，避免与 OAuth 面板渲染冲突
                self.global_panels.close_if(PanelKind::Mcp);
                self.services.oauth_prompt = Some(OAuthPrompt::new(
                    server_name,
                    authorization_url,
                    callback_tx,
                ));
                (true, true, false)
            }
            AgentEvent::OAuthAuthorizationCompleted { server_name } => {
                self.services.oauth_prompt = None;
                // 刷新 MCP 面板的服务器列表以反映新的连接状态
                if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
                    panel.servers = self
                        .services
                        .mcp_pool
                        .as_ref()
                        .map(|p| p.all_server_infos())
                        .unwrap_or_default();
                }
                let vm = MessageViewModel::system(format!("[i] OAuth 授权完成: {}", server_name));
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::OAuthAuthorizationFailed { server_name, error } => {
                self.services.oauth_prompt = None;
                // 刷新 MCP 面板的服务器列表（可能仍是 Failed 状态但信息已更新）
                if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
                    panel.servers = self
                        .services
                        .mcp_pool
                        .as_ref()
                        .map(|p| p.all_server_infos())
                        .unwrap_or_default();
                }
                let vm = MessageViewModel::system(format!(
                    "[i] OAuth 授权失败: {} - {}",
                    server_name, error
                ));
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::McpActionCompleted {
                server_name,
                action,
                success,
            } => {
                if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
                    panel.servers = self
                        .services
                        .mcp_pool
                        .as_ref()
                        .map(|p| p.all_server_infos())
                        .unwrap_or_default();
                }
                let msg = match (action.as_str(), success) {
                    ("clear_auth", true) => {
                        format!("[i] OAuth credentials cleared: {}", server_name)
                    }
                    ("clear_auth", false) => {
                        format!("[i] Failed to clear credentials: {}", server_name)
                    }
                    (_, true) => format!("[i] Action completed: {}", server_name),
                    (_, false) => format!("[i] Action failed: {}", server_name),
                };
                let vm = MessageViewModel::system(msg);
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::PluginActionCompleted {
                plugin_id,
                action,
                success,
                message,
            } => {
                // 从 installing/uninstalling 集合中移除
                if let Some(ref mut panel) = self.global_panels.get_mut::<PluginPanel>() {
                    panel.installing.remove(&plugin_id);
                    panel.uninstalling.remove(&plugin_id);
                    panel.marketplace_updating.remove(&plugin_id);

                    // 更新 discover 列表中的 installed 标记
                    match (action.as_str(), success) {
                        ("install", true) => {
                            for dp in &mut panel.discover_plugins {
                                if dp.plugin_id == plugin_id {
                                    dp.installed = true;
                                }
                            }
                        }
                        ("uninstall", true) => {
                            // 更新 discover 列表
                            for dp in &mut panel.discover_plugins {
                                if dp.plugin_id == plugin_id {
                                    dp.installed = false;
                                }
                            }
                            // 从 Installed 列表移除
                            panel.entries.retain(|e| e.id != plugin_id);
                            // 关闭详情页（如果正在查看被卸载的插件）
                            if panel.detail_index.is_some() || panel.discover_detail_index.is_some()
                            {
                                panel.detail_index = None;
                                panel.discover_detail_index = None;
                                panel.detail_cursor = 0;
                                panel.discover_detail_cursor = 0;
                            }
                            // 调整 cursor 避免越界
                            if panel.cursor >= panel.current_list_len() {
                                panel.cursor = panel.current_list_len().saturating_sub(1);
                            }
                        }
                        ("refresh", true) | ("add", true) => {
                            // Marketplace 刷新/添加成功，重新加载面板数据
                            // 保存当前面板状态
                            let current_view = panel.view;
                            let current_marketplace_cursor = panel.marketplace_cursor;
                            // 重新加载面板数据
                            self.open_plugin_panel();
                            // 恢复面板状态
                            if let Some(ref mut p) = self.global_panels.get_mut::<PluginPanel>() {
                                p.view = current_view;
                                // 确保 cursor 不越界
                                let max = p.marketplace_entries.len();
                                p.marketplace_cursor = if current_marketplace_cursor <= max {
                                    current_marketplace_cursor
                                } else {
                                    max
                                };
                            }
                        }
                        ("install_counts_refresh", _) => {
                            // 安装量数据后台刷新完成，重新加载面板以更新排序
                            let current_view = panel.view;
                            let current_cursor = panel.cursor;
                            let current_discover_cursor = panel.discover_cursor;
                            let current_marketplace_cursor = panel.marketplace_cursor;
                            self.open_plugin_panel();
                            if let Some(ref mut p) = self.global_panels.get_mut::<PluginPanel>() {
                                p.view = current_view;
                                p.cursor =
                                    current_cursor.min(p.current_list_len().saturating_sub(1));
                                p.discover_cursor = current_discover_cursor
                                    .min(p.discover_filtered_plugins().len().saturating_sub(1));
                                let max = p.marketplace_entries.len();
                                p.marketplace_cursor = if current_marketplace_cursor <= max {
                                    current_marketplace_cursor
                                } else {
                                    max
                                };
                            }
                            // 不显示系统消息
                            return (false, false, false);
                        }
                        _ => {}
                    }
                }
                let msg = match (action.as_str(), success) {
                    ("install", true) => {
                        format!("Plugin installed: {}", plugin_id)
                    }
                    ("install", false) => {
                        format!("Plugin install failed: {} ({})", plugin_id, message)
                    }
                    ("uninstall", true) => {
                        format!("Plugin uninstalled: {}", plugin_id)
                    }
                    ("uninstall", false) => {
                        format!("Plugin uninstall failed: {} ({})", plugin_id, message)
                    }
                    (_, true) => format!("Plugin action completed: {}", plugin_id),
                    (_, false) => {
                        format!("Plugin action failed: {} ({})", plugin_id, message)
                    }
                };
                let vm = MessageViewModel::system(msg);
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
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

                // 缓存率检查：低于 80% 时显示黄色提示
                {
                    let cache_read = usage.cache_read_input_tokens.unwrap_or(0) as u64;
                    let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0) as u64;
                    let denominator = cache_read + cache_creation;
                    if denominator > 0 {
                        let rate = cache_read as f64 / denominator as f64;
                        if rate < 0.8 {
                            let percentage = (rate * 100.0) as u32;
                            let msg = format!("Prompt cache 命中率 {}% < 80%", percentage);
                            let vm = MessageViewModel::cache_warning(msg);
                            self.session_mgr.sessions[self.session_mgr.active]
                                .messages
                                .view_messages
                                .push(vm.clone());
                            let _ = self.session_mgr.sessions[self.session_mgr.active]
                                .messages
                                .render_tx
                                .send(RenderEvent::AddMessage(vm));
                        }
                    }
                }

                // 累积到会话追踪器
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .session_token_tracker
                    .accumulate(&usage);
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
                    let budget = rust_create_agent::agent::token::ContextBudget::new(
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
                    .set_mode(perihelion_widgets::SpinnerMode::ToolUse);
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
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
            } => {
                // Pipeline：更新 ToolBlock 结果 / SubAgentGroup 完成
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::ToolEnd {
                        tool_call_id,
                        name,
                        output,
                        is_error,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::AssistantChunk(chunk) => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = None;
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_replied = true;
                // 跨切面：spinner
                self.session_mgr.sessions[self.session_mgr.active]
                    .spinner_state
                    .set_mode(perihelion_widgets::SpinnerMode::Responding);
                // Pipeline：路由到 SubAgentGroup 或父 Agent AssistantBubble
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::AssistantChunk(chunk));
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::Done => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = None;
                // Pipeline：finalize 当前 AI 消息 + reconcile 重建 view_messages
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::Done);
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                // reconcile 尾部重建：保留 SubAgentGroup 富状态，防止显示退化
                let (prefix_len, tail_vms) = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .reconcile_tail_with_subagents(
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .round_start_vm_idx,
                        &self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages,
                    );
                self.apply_pipeline_action(PipelineAction::RebuildAll {
                    prefix_len,
                    tail_vms,
                });
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
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_rx = None;
                }
                // Auto-compact 两级策略
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .needs_auto_compact
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .needs_auto_compact = false;
                    tracing::info!(
                        "auto-compact: context threshold reached, triggering full compact"
                    );
                    self.start_compact("auto".to_string());
                    return (true, false, true);
                } else {
                    let compact_config = self.get_compact_config();
                    let budget = rust_create_agent::agent::token::ContextBudget::new(
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
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages
                            .truncate(round_start);
                        let _ = self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .render_tx
                            .send(RenderEvent::LoadHistory(
                                self.session_mgr.sessions[self.session_mgr.active]
                                    .messages
                                    .view_messages
                                    .clone(),
                            ));
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
                        let vm =
                            MessageViewModel::system("⚠ 已中断（输入已恢复到输入框）".to_string());
                        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    } else {
                        let vm = MessageViewModel::system("⚠ 已中断".to_string());
                        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                    }
                } else {
                    // Agent 已回复部分内容，走正常的 reconcile 尾部重建（保留 SubAgentGroup 富状态）
                    let (prefix_len, tail_vms) = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .pipeline
                        .reconcile_tail_with_subagents(
                            self.session_mgr.sessions[self.session_mgr.active]
                                .messages
                                .round_start_vm_idx,
                            &self.session_mgr.sessions[self.session_mgr.active]
                                .messages
                                .view_messages,
                        );
                    self.apply_pipeline_action(PipelineAction::RebuildAll {
                        prefix_len,
                        tail_vms,
                    });
                    let vm = MessageViewModel::system(
                        "⚠ 已中断（工具调用已以 error 结尾，消息已保存，可继续发送恢复）"
                            .to_string(),
                    );
                    self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                }
                (true, false, false)
            }
            AgentEvent::Error(ref e) => {
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
                use rust_agent_middlewares::ask_user::{
                    AskUserBatchRequest, AskUserOption, AskUserQuestionData,
                };
                use rust_create_agent::interaction::{
                    ApprovalDecision, InteractionContext, InteractionResponse, QuestionAnswer,
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
                                            reason: "用户拒绝".to_string(),
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
                        {
                            let q_lines: Vec<String> = requests
                                .iter()
                                .flat_map(|q| {
                                    let hint = if q.multi_select {
                                        " [多选]"
                                    } else {
                                        " [单选]"
                                    };
                                    vec![
                                        format!("{}{}", q.header, hint),
                                        format!("  > {}", q.question),
                                    ]
                                })
                                .collect();
                            let vm = MessageViewModel::system(q_lines.join("\n"));
                            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
                        }
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
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_state_messages
                    .extend(msgs.clone());
                // Pipeline：更新 completed 状态（用于后续 reconcile）
                let actions = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::StateSnapshot(msgs));
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::CompactDone {
                summary,
                new_thread_id: _,
            } => {
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

                let inject_count = self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_state_messages
                    .len()
                    - 1;
                let compact_label = if inject_count > 0 {
                    format!("上下文已压缩，注入 {} 条上下文", inject_count)
                } else {
                    "上下文已压缩".to_string()
                };
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

                if !self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pending_messages
                    .is_empty()
                {
                    self.flush_pending_messages();
                }

                (true, false, true)
            }
            AgentEvent::CompactError(msg) => {
                let vm = MessageViewModel::system(format!("❌ 压缩失败: {}", msg));
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
            AgentEvent::LlmRetrying {
                attempt,
                max_attempts,
                delay_ms,
                error: _,
            } => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .retry_status = Some(super::agent_comm::RetryStatus {
                    attempt,
                    max_attempts,
                    delay_ms,
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
            } => {
                // 递减后台任务计数
                self.session_mgr.sessions[self.session_mgr.active].background_task_count =
                    self.session_mgr.sessions[self.session_mgr.active]
                        .background_task_count
                        .saturating_sub(1);

                // 用于 LLM 上下文的纯文本通知
                let state_notification = if success {
                    format!(
                        "[后台任务 {} 已完成] Agent: {} | 工具调用: {} | 耗时: {}ms\n结果:\n{}",
                        &task_id[..8.min(task_id.len())],
                        agent_name,
                        tool_calls_count,
                        duration_ms,
                        output,
                    )
                } else {
                    format!(
                        "[后台任务 {} 执行失败] Agent: {}\n错误:\n{}",
                        &task_id[..8.min(task_id.len())],
                        agent_name,
                        output,
                    )
                };

                // 将通知加入 agent_state_messages，使下一轮 agent 执行可见
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_state_messages
                    .push(rust_create_agent::messages::BaseMessage::human(
                        state_notification.as_str(),
                    ));

                // 以 ToolBlock 样式显示（紧凑单行格式，折叠长输出）
                let short_id = &task_id[..8.min(task_id.len())];
                let display_name = format!("bg:{}", agent_name);
                // 输出截断为单行（取第一行，再截取前 80 字符）
                let first_line = output.lines().next().unwrap_or("");
                let one_line = if first_line.chars().count() > 80 {
                    let truncated: String = first_line.chars().take(80).collect();
                    format!("{}...", truncated)
                } else if first_line.is_empty() && !output.is_empty() {
                    String::from("(empty)")
                } else {
                    first_line.to_string()
                };
                let header_info = if success {
                    format!(
                        "{} completed ({} calls, {}ms): {}",
                        short_id, tool_calls_count, duration_ms, one_line
                    )
                } else {
                    format!("{} failed: {}", short_id, one_line)
                };
                let mut vm =
                    MessageViewModel::tool_block(display_name.clone(), header_info, None, !success);
                if let MessageViewModel::ToolBlock { collapsed, .. } = &mut vm {
                    *collapsed = true; // 始终折叠，摘要已在 header 中
                }
                self.apply_pipeline_action(PipelineAction::AddMessage(vm));

                // 如果 agent 已完成（Done）且所有后台任务都已完成，关闭通道并自动提交 continuation
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .agent_done_pending_bg
                    && self.session_mgr.sessions[self.session_mgr.active].background_task_count == 0
                {
                    tracing::info!("all background tasks completed, auto-submitting continuation");
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_done_pending_bg = false;
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_rx = None;
                    // 截断显示文本（完整数据已在 agent_state_messages 中供 LLM 使用）
                    let display_notification = if success {
                        let output_preview: String = output
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(80)
                            .collect();
                        format!(
                            "[后台任务 {} 已完成] Agent: {} | 工具调用: {} | 耗时: {}ms\n{}",
                            &task_id[..8.min(task_id.len())],
                            agent_name,
                            tool_calls_count,
                            duration_ms,
                            if output.chars().count() > 80 || output.lines().count() > 1 {
                                format!("{}...", output_preview)
                            } else {
                                output_preview
                            },
                        )
                    } else {
                        let err_preview: String = output.chars().take(80).collect();
                        format!(
                            "[后台任务 {} 执行失败] Agent: {} | {}",
                            &task_id[..8.min(task_id.len())],
                            agent_name,
                            err_preview,
                        )
                    };
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .pending_bg_continuation = Some(display_notification);
                    return (true, false, true);
                }

                (true, false, false)
            }
        }
    }

    /// 每帧调用：消费 channel 事件，返回是否有 UI 更新
    pub fn poll_agent(&mut self) -> bool {
        // 优先处理延迟的后台任务 continuation（由 BackgroundTaskCompleted 处理器设置）
        if let Some(continuation) = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pending_bg_continuation
            .take()
        {
            if !self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .loading
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
                        Some("Agent 连接异常断开，请重试发送消息".to_string()),
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

    /// 执行 micro-compact：清除旧工具结果，不调用 LLM
    pub fn start_micro_compact(&mut self) {
        use rust_create_agent::agent::compact::micro_compact_enhanced;
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
            let vm =
                MessageViewModel::system(format!("自动清理：释放了 {} 个工具调用结果", cleared));
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preload_skills_extracts_slash_prefix() {
        let result = extract_skill_tokens("请使用 /commit 提交");
        assert_eq!(result, vec!["commit"]);
    }

    #[test]
    fn test_preload_skills_extracts_multiple_skills() {
        let result = extract_skill_tokens("/review /refactor");
        assert_eq!(result, vec!["review", "refactor"]);
    }

    #[test]
    fn test_preload_skills_ignores_hash_prefix() {
        let result = extract_skill_tokens("#old-skill /new-skill");
        assert_eq!(result, vec!["new-skill"], "# 前缀不再匹配");
    }

    #[test]
    fn test_preload_skills_empty_for_no_skills() {
        let result = extract_skill_tokens("普通消息没有 skill 引用");
        assert!(result.is_empty());
    }

    #[test]
    fn test_preload_skills_truncates_on_invalid_char() {
        let result = extract_skill_tokens("/skill-name!suffix");
        assert_eq!(result, vec!["skill-name"], "遇到 ! 截断");
    }

    // ─── reconcile 事件处理测试 ──────────────────────────────────────────────

    /// 场景1: Done 事件触发 reconcile → view_messages 被截断并 extend
    #[tokio::test]
    async fn test_reconcile_event_handling_done() {
        use rust_create_agent::messages::BaseMessage;

        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);

        let mut messages = crate::app::MessageState::new(
            "/tmp".to_string(),
            render_tx,
            render_cache,
            render_notify,
        );

        messages.view_messages = vec![
            crate::ui::message_view::MessageViewModel::user("q1".to_string()),
            crate::ui::message_view::MessageViewModel::from_base_message(
                &BaseMessage::ai("a1".to_string()),
                &[],
            ),
        ];

        messages.round_start_vm_idx = 2;

        messages.pipeline.restore_completed(vec![
            BaseMessage::human("q1"),
            BaseMessage::ai("a1"),
            BaseMessage::human("q2"),
            BaseMessage::ai("a2"),
        ]);

        let (prefix_len, tail_vms) = messages
            .pipeline
            .reconcile_tail(messages.round_start_vm_idx);
        assert_eq!(prefix_len, 2);

        messages.view_messages.truncate(prefix_len);
        messages.view_messages.extend(tail_vms);

        assert_eq!(messages.view_messages.len(), 4);
    }

    /// 场景2: Interrupted 事件触发 reconcile → 与 Done 相同
    #[tokio::test]
    async fn test_reconcile_event_handling_interrupted() {
        use rust_create_agent::messages::BaseMessage;

        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);

        let mut messages = crate::app::MessageState::new(
            "/tmp".to_string(),
            render_tx,
            render_cache,
            render_notify,
        );

        messages.view_messages = vec![
            crate::ui::message_view::MessageViewModel::user("q1".to_string()),
            crate::ui::message_view::MessageViewModel::from_base_message(
                &BaseMessage::ai("a1".to_string()),
                &[],
            ),
        ];
        messages.round_start_vm_idx = 2;

        messages.pipeline.restore_completed(vec![
            BaseMessage::human("q1"),
            BaseMessage::ai("a1"),
            BaseMessage::human("q2"),
        ]);

        let (prefix_len, tail_vms) = messages
            .pipeline
            .reconcile_tail(messages.round_start_vm_idx);
        assert_eq!(prefix_len, 2);

        messages.view_messages.truncate(prefix_len);
        messages.view_messages.extend(tail_vms);

        assert_eq!(messages.view_messages.len(), 3);
    }

    /// 场景3: submit_message 记录 round_start_vm_idx
    #[tokio::test]
    async fn test_submit_message_records_round_start_vm_idx() {
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);

        let mut messages = crate::app::MessageState::new(
            "/tmp".to_string(),
            render_tx,
            render_cache,
            render_notify,
        );

        messages.view_messages = vec![
            crate::ui::message_view::MessageViewModel::user("q1".to_string()),
            crate::ui::message_view::MessageViewModel::from_base_message(
                &rust_create_agent::messages::BaseMessage::ai("a1".to_string()),
                &[],
            ),
            crate::ui::message_view::MessageViewModel::user("q2".to_string()),
        ];

        messages.round_start_vm_idx = messages.view_messages.len();
        assert_eq!(messages.round_start_vm_idx, 3);

        messages
            .view_messages
            .push(crate::ui::message_view::MessageViewModel::user(
                "q3".to_string(),
            ));
        assert_eq!(messages.view_messages.len(), 4);
        assert_eq!(
            messages.round_start_vm_idx, 3,
            "round_start_vm_idx 应保持为 push 前的值"
        );
    }
}
