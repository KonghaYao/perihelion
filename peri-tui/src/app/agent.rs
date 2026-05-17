use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

use super::interaction_broker::TuiInteractionBroker;
pub(crate) use super::provider::LlmProvider;
use super::AgentEvent;
use peri_agent::agent::events::{AgentEvent as ExecutorEvent, FnEventHandler};
use peri_agent::agent::react::AgentInput;
use peri_agent::agent::state::AgentState;
use peri_agent::agent::{AgentCancellationToken, ReActAgent};
use peri_agent::interaction::UserInteractionBroker;
use peri_agent::llm::BaseModelReactLLM;
use peri_middlewares::prelude::*;
use peri_middlewares::tools::{AskUserTool, TodoItem};

// ─── 主入口 ───────────────────────────────────────────────────────────────────

/// run_universal_agent 的参数集合（避免超过 clippy 的参数数量限制）
pub struct AgentRunConfig {
    pub provider: LlmProvider,
    pub input: AgentInput,
    pub cwd: String,
    pub history: Vec<peri_agent::messages::BaseMessage>,
    pub tx: mpsc::Sender<AgentEvent>,
    pub cancel: AgentCancellationToken,
    pub agent_id: Option<String>,
    pub langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
    pub thread_store: Arc<dyn peri_agent::thread::ThreadStore>,
    pub thread_id: peri_agent::thread::ThreadId,
    pub config: Arc<crate::config::PeriConfig>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    /// 插件 skills 搜索路径（追加到 SkillsMiddleware）
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    /// 插件 agent 搜索路径（追加到 scan_agents）
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    /// 插件 hooks（从 PluginLoadResult.all_hooks 传入）
    pub plugin_hooks: Vec<peri_middlewares::hooks::RegisteredHook>,
    /// 插件 LSP 服务器配置（从 PluginLoadResult.all_lsp_servers 传入）
    pub plugin_lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    /// Hook 分组：每组对应一个独立的 HookMiddleware 实例，灵活控制执行顺序和生命周期
    pub hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    /// Whether this is the first message of a new session (triggers SessionStart hook)
    pub hook_session_start: bool,
    /// 会话级 ToolSearch 索引（跨 submit 复用，缓存 deferred tools 提示词）
    pub tool_search_index: std::sync::Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    /// 会话级共享工具注册表（跨 submit 复用）
    pub shared_tools: std::sync::Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, std::sync::Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    /// 需要预加载全文的 skill 名称列表（从用户消息中 /skill-name 模式提取）
    pub preload_skills: Vec<String>,
}

// ── 共享 Agent 构建（ACP 和 TUI 共用）─────────────────────────────────────────

use parking_lot::RwLock;
use std::collections::HashMap;

/// 共享 Agent 构建配置（ACP 和 TUI 共用）
pub(crate) struct BareAgentConfig {
    pub provider: LlmProvider,
    pub cwd: String,
    pub system_prompt: String,
    pub event_handler: Arc<dyn peri_agent::agent::events::AgentEventHandler>,
    pub cancel: AgentCancellationToken,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub peri_config: Arc<crate::config::PeriConfig>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    pub agent_overrides: Option<peri_middlewares::agent_define::AgentOverrides>,
    pub preload_skills: Vec<String>,
    pub session_id: Option<String>,
    pub broker: Arc<dyn UserInteractionBroker>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    pub hook_session_start: bool,
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    pub tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>>,
}

pub(crate) struct BareAgentOutput {
    pub executor: ReActAgent<peri_agent::llm::RetryableLLM<BaseModelReactLLM>, AgentState>,
    pub todo_rx: mpsc::Receiver<Vec<TodoItem>>,
    #[allow(dead_code)]
    pub context_window: u32,
}

/// 构建可复用的 Agent（ACP 和 TUI 共用核心构建逻辑）
pub(crate) fn build_bare_agent(cfg: BareAgentConfig) -> BareAgentOutput {
    let BareAgentConfig {
        provider,
        cwd,
        system_prompt,
        event_handler,
        cancel,
        permission_mode,
        peri_config,
        cron_scheduler,
        agent_overrides,
        preload_skills,
        session_id,
        broker: permission_broker,
        plugin_skill_dirs,
        plugin_agent_dirs,
        hook_groups,
        hook_session_start,
        mcp_pool,
        tool_search_index,
        shared_tools,
    } = cfg;

    // 应用 agent overrides 到系统提示词
    let system_prompt = agent_overrides.as_ref().map_or_else(
        || system_prompt.clone(),
        |ov| {
            let features = crate::prompt::PromptFeatures::detect();
            crate::prompt::build_system_prompt(Some(ov), &cwd, features, &plugin_agent_dirs)
        },
    );

    let provider_for_factory = provider.clone();
    let model_name = provider.model_name().to_string();
    let provider_name = provider.display_name().to_string();

    // LLM 模型
    let mut base_llm = BaseModelReactLLM::new(provider.into_model());
    if let Some(ref sid) = session_id {
        base_llm = base_llm.with_session_id(sid);
    }
    let model =
        peri_agent::llm::RetryableLLM::new(base_llm, peri_agent::llm::RetryConfig::default())
            .with_event_handler(Arc::clone(&event_handler));

    // Todo channel
    let (todo_tx, todo_rx) = mpsc::channel::<Vec<TodoItem>>(8);

    // HITL middleware
    let auto_classifier: Option<Arc<dyn AutoClassifier>> =
        Some(Arc::new(LlmAutoClassifier::new(Arc::new(
            tokio::sync::Mutex::new(provider_for_factory.clone().into_model()),
        ))));
    let hitl = HumanInTheLoopMiddleware::with_shared_mode(
        permission_broker.clone(),
        default_requires_approval,
        permission_mode,
        auto_classifier,
    );

    // AskUser 工具
    let ask_user_tool = AskUserTool::new(permission_broker);

    // 父工具集（供子 agent 继承）
    let mut parent_tools: Vec<Box<dyn peri_agent::tools::BaseTool>> =
        FilesystemMiddleware::build_tools(&cwd);
    parent_tools.extend(TerminalMiddleware::build_tools(&cwd));
    if let Some(ref pool) = mcp_pool {
        let mcp_tools = peri_middlewares::mcp::build_tool_bridges(pool);
        for tool in mcp_tools {
            parent_tools.push(tool);
        }
        if pool.has_resources() {
            parent_tools.push(Box::new(peri_middlewares::mcp::McpResourceTool::new(
                Arc::clone(pool),
            )));
        }
    }

    // 子 agent LLM 工厂
    let provider_clone = provider_for_factory;
    let config_for_factory = peri_config.clone();
    let session_id_for_factory = session_id.clone();
    #[allow(clippy::type_complexity)]
    let llm_factory: Arc<
        dyn Fn(Option<&str>) -> Box<dyn peri_agent::agent::react::ReactLLM + Send + Sync>
            + Send
            + Sync,
    > = Arc::new(move |model_alias: Option<&str>| {
        let sid = session_id_for_factory.as_deref();
        if let Some(alias) = model_alias {
            if let Some(p) = LlmProvider::from_config_for_alias(&config_for_factory, alias) {
                let mut llm = BaseModelReactLLM::new(p.into_model());
                if let Some(s) = sid {
                    llm = llm.with_session_id(s);
                }
                return Box::new(peri_agent::llm::RetryableLLM::new(
                    llm,
                    peri_agent::llm::RetryConfig::default(),
                ));
            }
        }
        let mut llm = BaseModelReactLLM::new(provider_clone.clone().into_model());
        if let Some(s) = sid {
            llm = llm.with_session_id(s);
        }
        Box::new(peri_agent::llm::RetryableLLM::new(
            llm,
            peri_agent::llm::RetryConfig::default(),
        ))
    });

    // 系统提示构建器
    #[allow(clippy::type_complexity)]
    let system_builder: Arc<
        dyn Fn(Option<&peri_middlewares::agent_define::AgentOverrides>, &str) -> String
            + Send
            + Sync,
    > = Arc::new(|overrides, cwd_dir| {
        let features = crate::prompt::PromptFeatures::detect();
        crate::prompt::build_system_prompt(overrides, cwd_dir, features, &[])
    });

    // Parent message snapshot
    let parent_messages: Arc<RwLock<Vec<peri_agent::messages::BaseMessage>>> =
        Arc::new(RwLock::new(Vec::new()));

    // 后台任务通知通道
    let (bg_notification_tx, bg_notification_rx) = tokio::sync::mpsc::unbounded_channel();
    let background_registry = Arc::new(peri_middlewares::BackgroundTaskRegistry::new(
        bg_notification_tx,
    ));

    let claude_md_excludes = peri_config
        .config
        .claude_md_excludes
        .clone()
        .unwrap_or_default();

    // SubAgent middleware
    let subagent = SubAgentMiddleware::new(
        parent_tools,
        Some(Arc::clone(&event_handler) as Arc<dyn peri_agent::agent::events::AgentEventHandler>),
        llm_factory.clone(),
    )
    .with_system_builder(system_builder)
    .with_cancel(cancel.clone())
    .with_parent_messages(parent_messages)
    .with_background_registry(Arc::clone(&background_registry))
    .with_registered_hooks(vec![]);

    // 上下文预算
    let mut context_window = model.context_window();
    let context_1m = peri_config.config.context_1m.unwrap_or(false);
    if context_1m {
        context_window = 1_000_000;
    }
    let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();
    let context_budget = peri_agent::agent::token::ContextBudget::new(context_window)
        .with_auto_compact_threshold(compact_config.auto_compact_threshold)
        .with_warning_threshold(compact_config.micro_compact_threshold);

    // 构建 ReActAgent
    let executor = ReActAgent::new(model)
        .max_iterations(500)
        .with_context_budget(context_budget)
        .with_compact_config(compact_config)
        .with_notification_rx(bg_notification_rx)
        .with_system_prompt(system_prompt)
        .with_tool_filter(peri_middlewares::tool_search::is_deferred_tool)
        .with_shared_tools(Arc::clone(&shared_tools))
        .add_middleware(Box::new(
            AgentsMdMiddleware::new().with_excludes(claude_md_excludes),
        ))
        .add_middleware(Box::new(AgentDefineMiddleware::new()))
        .add_middleware(Box::new(
            SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs),
        ))
        .add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
        .add_middleware(Box::new(FilesystemMiddleware::new()))
        .add_middleware(Box::new(peri_middlewares::GitAttributionMiddleware::new(
            &model_name,
        )))
        .add_middleware(Box::new(TerminalMiddleware::new()))
        .add_middleware(Box::new(WebMiddleware::new()))
        .add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
        .add_middleware(Box::new(peri_middlewares::cron::CronMiddleware::new(
            cron_scheduler.unwrap_or_else(|| {
                Arc::new(parking_lot::Mutex::new(
                    peri_middlewares::cron::CronScheduler::new(
                        tokio::sync::mpsc::unbounded_channel().0,
                    ),
                ))
            }),
        )));

    // Hook middleware groups
    let mut executor = executor;
    if !hook_groups.is_empty() {
        let hook_llm_factory: Arc<
            dyn Fn() -> Box<dyn peri_agent::agent::react::ReactLLM + Send + Sync> + Send + Sync,
        > = Arc::new({
            let factory = llm_factory.clone();
            move || factory(None)
        });
        for (i, group) in hook_groups.into_iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            let mw = peri_middlewares::hooks::HookMiddleware::with_session_start(
                group,
                hook_llm_factory.clone(),
                &cwd,
                "",
                "",
                "",
                provider_name.clone(),
                hook_session_start && i == 0,
            );
            executor = executor.add_middleware(Box::new(mw));
        }
    }

    let executor = executor.add_middleware(Box::new(hitl));
    let executor = executor.add_middleware(Box::new(subagent));

    // MCP 中间件
    let executor = if let Some(pool) = mcp_pool {
        executor.add_middleware(Box::new(peri_middlewares::mcp::McpMiddleware::new(pool)))
    } else {
        executor
    };

    // ToolSearch 中间件
    let executor = executor.add_middleware(Box::new(peri_middlewares::ToolSearchMiddleware::new(
        Arc::clone(&tool_search_index),
        Arc::clone(&shared_tools),
    )));

    let executor = executor
        .with_event_handler(Arc::clone(&event_handler))
        .register_tool(Box::new(ask_user_tool));

    BareAgentOutput {
        executor,
        todo_rx,
        context_window,
    }
}

pub async fn run_universal_agent(cfg: AgentRunConfig) {
    let AgentRunConfig {
        provider,
        input,
        cwd,
        history,
        tx,
        cancel,
        agent_id,
        langfuse_tracer,
        thread_store,
        thread_id,
        config: peri_config,
        cron_scheduler,
        permission_mode,
        mcp_pool,
        plugin_skill_dirs,
        plugin_agent_dirs,
        plugin_hooks: _,
        plugin_lsp_servers,
        hook_groups,
        hook_session_start,
        tool_search_index,
        shared_tools,
        preload_skills,
    } = cfg;
    // 如果设置了 agent_id，提前解析 agent.md 获取可覆盖部分（persona / tone / proactiveness），
    // 替换 system prompt 中对应占位符；安全策略、代码规范等硬约束始终保留。
    // 使用 spawn_blocking 避免同步 I/O 阻塞 tokio 运行时。
    let overrides = if let Some(id) = agent_id.as_deref() {
        let cwd_clone = cwd.clone();
        let id_owned = id.to_string();
        tokio::task::spawn_blocking(move || {
            peri_middlewares::AgentDefineMiddleware::load_overrides(&cwd_clone, &id_owned)
        })
        .await
        .unwrap_or(None)
    } else {
        None
    };
    let features = crate::prompt::PromptFeatures::detect();
    let system_prompt =
        crate::prompt::build_system_prompt(overrides.as_ref(), &cwd, features, &plugin_agent_dirs);
    let provider_name = provider.display_name().to_string();

    // 事件回调 → TUI AgentEvent channel（在 model 之前创建，供 RetryableLLM 使用）
    let tx_event = tx.clone();
    let cwd_for_handler = cwd.clone();
    let langfuse_for_handler = langfuse_tracer.clone();
    let provider_name_for_handler = provider_name.clone();
    let handler: Arc<dyn peri_agent::agent::events::AgentEventHandler> = Arc::new(FnEventHandler(
        move |event: ExecutorEvent| {
            // Langfuse hook（在 TUI 事件映射前执行，使用原始 ExecutorEvent）
            if let Some(ref tracer) = langfuse_for_handler {
                let _lock_start = std::time::Instant::now();
                let mut t = tracer.lock();
                let _wait = _lock_start.elapsed();
                if _wait.as_millis() > 50 {
                    tracing::warn!(
                        "[DEADLOCK] handler: tracer lock acquired AFTER {:?} wait for event {:?}",
                        _wait,
                        std::mem::discriminant(&event)
                    );
                }
                match &event {
                    ExecutorEvent::LlmCallStart {
                        step,
                        messages,
                        tools,
                    } => t.on_llm_start(*step, messages, tools),
                    ExecutorEvent::LlmCallEnd {
                        step,
                        model,
                        output,
                        usage,
                    } => t.on_llm_end(
                        *step,
                        model,
                        &provider_name_for_handler,
                        output,
                        usage.as_ref(),
                    ),
                    ExecutorEvent::ToolStart {
                        tool_call_id,
                        name,
                        input,
                        ..
                    } => t.on_tool_start(tool_call_id, name, input),
                    ExecutorEvent::ToolEnd {
                        tool_call_id,
                        is_error,
                        output,
                        ..
                    } => t.on_tool_end(tool_call_id, output, *is_error),
                    // 累积最终回答（避免从 UI 截断视图提取）
                    ExecutorEvent::TextChunk { chunk: text, .. } => t.on_text_chunk(text),
                    _ => {}
                }
                drop(t);
                tracing::debug!("[DEADLOCK] handler: tracer lock released");
            }

            // 映射为 TUI AgentEvent
            if let Some(msg) = map_executor_event(event, &cwd_for_handler) {
                if let Err(e) = tx_event.try_send(msg) {
                    match e {
                        tokio::sync::mpsc::error::TrySendError::Full(msg) => {
                            tracing::warn!(
                                event_type = ?std::mem::discriminant(&msg),
                                "AgentEvent channel full (4096), dropping event"
                            );
                        }
                        tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                            tracing::warn!(
                                "AgentEvent channel closed, dropping event (receiver already dropped)"
                            );
                        }
                    }
                }
            }
        },
    ));

    // 使用共享 Agent 构建逻辑
    let broker: Arc<dyn UserInteractionBroker> = TuiInteractionBroker::new(tx.clone());
    let shared_cfg = BareAgentConfig {
        provider,
        cwd: cwd.clone(),
        system_prompt,
        event_handler: Arc::clone(&handler),
        cancel: cancel.clone(),
        permission_mode,
        peri_config,
        cron_scheduler,
        agent_overrides: overrides,
        preload_skills,
        session_id: Some(thread_id.to_string()),
        broker: broker as Arc<dyn UserInteractionBroker>,
        plugin_skill_dirs,
        plugin_agent_dirs,
        hook_groups,
        hook_session_start,
        mcp_pool,
        tool_search_index: Arc::clone(&tool_search_index),
        shared_tools: Arc::clone(&shared_tools),
    };
    let BareAgentOutput {
        mut executor,
        todo_rx,
        ..
    } = build_bare_agent(shared_cfg);

    // Todo 转发到 TUI
    let tx_todo = tx.clone();
    tokio::spawn(async move {
        let mut rx = todo_rx;
        while let Some(todos) = rx.recv().await {
            if tx_todo.send(AgentEvent::TodoUpdate(todos)).await.is_err() {
                tracing::warn!("todo forwarding: TUI channel closed, stopping");
                break;
            }
        }
    });

    // LSP 中间件（TUI only）
    let lsp_settings_path = dirs_next::home_dir().map(|h| h.join(".peri").join("settings.json"));
    let mut lsp_config = if let Some(ref settings_path) = lsp_settings_path {
        peri_lsp::config::load_global_lsp_config(settings_path)
    } else {
        peri_lsp::config::LspConfigFile::default()
    };
    for server in plugin_lsp_servers {
        lsp_config
            .lsp_servers
            .entry(server.name.clone())
            .or_insert(server);
    }
    if !lsp_config.lsp_servers.is_empty() {
        tracing::info!(
            target: "lsp",
            servers = lsp_config.lsp_servers.len(),
            "加载 LSP 配置"
        );
        executor = executor.add_middleware(Box::new(peri_middlewares::LspMiddleware::new(
            cwd.to_string(),
            lsp_config,
        )));
    }

    let mut state =
        AgentState::with_messages(cwd, history).with_persistence(thread_store, thread_id);
    if let Some(id) = agent_id {
        state = state.with_context("agent_id", id);
    }
    let agent_input = input;

    let result = executor
        .execute(agent_input, &mut state, Some(cancel))
        .await;

    // executor 内部通过增量 StateSnapshot 完整覆盖所有新增消息（包括
    // drain_notifications 和 run_after_agent 产生的内容），
    // 无需在此处再发最终快照——重复快照会导致 agent_state_messages 消息重复。
    drop(state);

    match result {
        Ok(_) => {
            if tx.send(AgentEvent::Done).await.is_err() {
                warn!("agent: failed to send Done (channel closed)");
            }
        }
        Err(peri_agent::error::AgentError::Interrupted) => {
            if tx.send(AgentEvent::Interrupted).await.is_err() {
                warn!("agent: failed to send Interrupted (channel closed)");
            }
            if tx.send(AgentEvent::Done).await.is_err() {
                warn!("agent: failed to send Done after Interrupted (channel closed)");
            }
        }
        Err(e) => {
            if tx.send(AgentEvent::Error(e.to_string())).await.is_err() {
                warn!("agent: failed to send Error (channel closed)");
            }
            if tx.send(AgentEvent::Done).await.is_err() {
                warn!("agent: failed to send Done after Error (channel closed)");
            }
        }
    }
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

use super::tool_display::{format_tool_args, format_tool_name, truncate};

/// 将 ExecutorEvent 映射为 TUI AgentEvent；不需转发的内部事件返回 None
fn map_executor_event(event: ExecutorEvent, cwd: &str) -> Option<AgentEvent> {
    Some(match event {
        ExecutorEvent::AiReasoning(text) => AgentEvent::AiReasoning(text),
        ExecutorEvent::TextChunk {
            chunk: text,
            source_agent_id,
            ..
        } => AgentEvent::AssistantChunk {
            chunk: text,
            source_agent_id,
        },
        // Agent ToolStart → SubAgentStart（在通用 ToolStart 分支之前）
        ExecutorEvent::ToolStart { name, input, .. } if name == "Agent" => {
            let agent_id = input
                .get("subagent_type")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("fork")
                .to_string();
            let task_preview = input["prompt"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(40)
                .collect();
            let is_background = input["run_in_background"].as_bool().unwrap_or(false);
            AgentEvent::SubAgentStart {
                agent_id,
                task_preview,
                is_background,
            }
        }
        ExecutorEvent::ToolStart {
            tool_call_id,
            name,
            input,
            source_agent_id,
            ..
        } => AgentEvent::ToolStart {
            tool_call_id,
            name: name.clone(),
            display: format_tool_name(&name),
            args: format_tool_args(&name, &input, Some(cwd)).unwrap_or_default(),
            input: input.clone(),
            source_agent_id,
        },
        // ask_user 成功：显示用户的回答
        ExecutorEvent::ToolEnd {
            tool_call_id,
            name,
            output,
            is_error: false,
            source_agent_id,
            ..
        } if name == "AskUserQuestion" => AgentEvent::ToolEnd {
            tool_call_id,
            name,
            output: format!("? → {}", truncate(&output, 60)),
            is_error: false,
            source_agent_id,
        },
        // 工具执行出错
        ExecutorEvent::ToolEnd {
            tool_call_id,
            name,
            output,
            is_error: true,
            source_agent_id,
            ..
        } => AgentEvent::ToolEnd {
            tool_call_id,
            name,
            output: format!("✗ {}", truncate(&output, 60)),
            is_error: true,
            source_agent_id,
        },
        // 无需转发的内部事件（ToolEnd 成功事件需要转发以更新 ToolBlock 内容）
        ExecutorEvent::StateSnapshot(msgs) => AgentEvent::StateSnapshot(msgs),
        ExecutorEvent::StepDone { .. }
        | ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. } => return None,
        // 成功的 ToolEnd（非 Agent / AskUserQuestion / error）
        ExecutorEvent::ToolEnd {
            tool_call_id,
            name,
            output,
            source_agent_id,
            ..
        } => AgentEvent::ToolEnd {
            tool_call_id,
            name,
            output: truncate(&output, 200),
            is_error: false,
            source_agent_id,
        },
        // 上下文使用警告：映射为 TUI 层事件，由 handle_agent_event 触发 auto-compact
        ExecutorEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        } => AgentEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        },
        ExecutorEvent::LlmCallEnd {
            usage: Some(usage),
            model,
            ..
        } => AgentEvent::TokenUsageUpdate { usage, model },
        ExecutorEvent::LlmCallEnd { usage: None, .. } => return None,
        ExecutorEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            error,
        } => AgentEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            error,
        },
        ExecutorEvent::BackgroundTaskCompleted(result) => AgentEvent::BackgroundTaskCompleted {
            task_id: result.task_id,
            agent_name: result.agent_name,
            success: result.success,
            output: result.output,
            tool_calls_count: result.tool_calls_count,
            duration_ms: result.duration_ms,
        },
        ExecutorEvent::LspDiagnostics {
            errors,
            warnings,
            files_with_errors,
        } => AgentEvent::LspDiagnostics {
            errors,
            warnings,
            files_with_errors,
        },
        // SubAgent 生命周期事件 → 触发 spinner 更新 + 刷新显示
        ExecutorEvent::SubagentStarted { agent_name } => AgentEvent::SubagentLifecycle {
            agent_name,
            started: true,
        },
        ExecutorEvent::SubagentStopped {
            agent_name,
            result,
            is_error,
        } => AgentEvent::SubAgentEnd {
            agent_id: Some(agent_name),
            result,
            is_error,
        },
        // Other lifecycle events — not yet handled in TUI, ignore
        ExecutorEvent::SessionEnded
        | ExecutorEvent::CompactStarted
        | ExecutorEvent::CompactCompleted => return None,
    })
}

// ─── 上下文压缩任务 ────────────────────────────────────────────────────────────

/// 独立的上下文压缩异步任务：调用核心层 full_compact + re_inject 三阶段流程
#[allow(clippy::too_many_arguments)]
pub async fn compact_task(
    messages: Vec<peri_agent::messages::BaseMessage>,
    model: Box<dyn peri_agent::llm::BaseModel>,
    instructions: String,
    config: peri_agent::agent::CompactConfig,
    cwd: String,
    tx: mpsc::Sender<super::AgentEvent>,
    cancel: AgentCancellationToken,
    registered_hooks: Vec<peri_middlewares::hooks::types::RegisteredHook>,
    session_id: String,
    transcript_path: String,
    provider_name: String,
) {
    use peri_agent::agent::{full_compact, re_inject};
    use peri_middlewares::hooks::middleware::fire_standalone_lifecycle_hooks;
    use peri_middlewares::hooks::types::HookEvent;

    let msg_count = messages.len();

    tracing::info!(msg_count = msg_count, "compact_task: 开始 Full Compact");

    // Fire PreCompact hooks
    fire_standalone_lifecycle_hooks(
        &registered_hooks,
        HookEvent::PreCompact,
        &cwd,
        &session_id,
        &transcript_path,
        &provider_name,
        Some(msg_count),
    )
    .await;

    // full_compact 调用 LLM，支持取消
    let compact_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            tracing::info!("compact_task: 被用户取消");
            if tx.send(super::AgentEvent::CompactError("已取消".to_string())).await.is_err() {
                warn!("compact_task: failed to send CompactError (channel closed)");
            }
            // Fire PostCompact even on cancel
            fire_standalone_lifecycle_hooks(
                &registered_hooks,
                HookEvent::PostCompact,
                &cwd,
                &session_id,
                &transcript_path,
                &provider_name,
                Some(msg_count),
            )
            .await;
            return;
        }
        result = full_compact(&messages, model.as_ref(), &config, &instructions) => {
            match result {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "compact_task: Full Compact 失败");
                    if tx.send(super::AgentEvent::CompactError(e.to_string())).await.is_err() {
                        warn!("compact_task: failed to send CompactError (channel closed)");
                    }
                    // Fire PostCompact even on failure
                    fire_standalone_lifecycle_hooks(
                        &registered_hooks,
                        HookEvent::PostCompact,
                        &cwd,
                        &session_id,
                        &transcript_path,
                        &provider_name,
                        Some(msg_count),
                    )
                    .await;
                    return;
                }
            }
        }
    };

    // 取消检查：re_inject 之前
    if cancel.is_cancelled() {
        tracing::info!("compact_task: re_inject 前被取消");
        if tx
            .send(super::AgentEvent::CompactError("已取消".to_string()))
            .await
            .is_err()
        {
            warn!("compact_task: failed to send CompactError on re_inject cancel (channel closed)");
        }
        fire_standalone_lifecycle_hooks(
            &registered_hooks,
            HookEvent::PostCompact,
            &cwd,
            &session_id,
            &transcript_path,
            &provider_name,
            Some(msg_count),
        )
        .await;
        return;
    }

    tracing::info!(
        summary_len = compact_result.summary.len(),
        messages_used = compact_result.messages_used,
        "compact_task: Full Compact 完成"
    );

    let re_inject_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            tracing::info!("compact_task: re_inject 阶段被取消");
            if tx.send(super::AgentEvent::CompactError("已取消".to_string())).await.is_err() {
                warn!("compact_task: failed to send CompactError (channel closed)");
            }
            fire_standalone_lifecycle_hooks(
                &registered_hooks,
                HookEvent::PostCompact,
                &cwd,
                &session_id,
                &transcript_path,
                &provider_name,
                Some(msg_count),
            )
            .await;
            return;
        }
        result = re_inject(&messages, &config, &cwd) => result,
    };

    tracing::info!(
        files_injected = re_inject_result.files_injected,
        skills_injected = re_inject_result.skills_injected,
        "compact_task: 重新注入完成"
    );

    // compact_result.summary 已包含 postprocess_summary 添加的前缀，无需重复添加
    let summary_text = compact_result.summary;

    let re_inject_content = if re_inject_result.messages.is_empty() {
        String::new()
    } else {
        let mut parts = Vec::new();
        for msg in &re_inject_result.messages {
            parts.push(msg.content());
        }
        // 使用唯一分隔符避免文件内容中的空行被错误分割
        format!(
            "\n\n---RE_INJECT_SEPARATOR---\n{}",
            parts.join("\n---RE_INJECT_MSG_BREAK---\n")
        )
    };

    let combined_summary = format!("{}{}", summary_text, re_inject_content);

    // Fire PostCompact hooks on success
    fire_standalone_lifecycle_hooks(
        &registered_hooks,
        HookEvent::PostCompact,
        &cwd,
        &session_id,
        &transcript_path,
        &provider_name,
        Some(msg_count),
    )
    .await;

    if tx
        .send(super::AgentEvent::CompactDone {
            summary: combined_summary,
            new_thread_id: String::new(),
        })
        .await
        .is_err()
    {
        warn!("compact_task: failed to send CompactDone (channel closed)");
    }
}
