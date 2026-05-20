//! Shared Agent builder（ACP 和 TUI 共用）
//!
//! 提供 `AcpAgentConfig` 配置结构和 `build_agent()` 构建函数，
//! 组装完整的中间件链和 ReActAgent 实例。
//!
//! 本模块从 peri-tui/src/app/agent.rs:build_bare_agent() 迁移而来，
//! 删除 TUI 特有依赖（AgentEvent channel、map_executor_event），
//! 改为通过 `child_handler_factory` 参数从外部注入。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use peri_agent::agent::compact::CompactConfig;
use peri_agent::agent::events::{AgentEvent as ExecutorEvent, AgentEventHandler};
use peri_agent::agent::token::ContextBudget;
use peri_agent::llm::BaseModel;

/// 子 Agent 事件 handler 工厂类型
pub type ChildHandlerFactory = Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>;
use peri_agent::agent::state::AgentState;
use peri_agent::agent::{AgentCancellationToken, ReActAgent};
use peri_agent::interaction::UserInteractionBroker;
use peri_agent::llm::BaseModelReactLLM;
use peri_middlewares::compact_middleware::CompactMiddleware;
use peri_middlewares::prelude::*;
use peri_middlewares::tools::{AskUserTool, TodoItem};

use crate::provider::config::PeriConfig;
use crate::provider::LlmProvider;

// ── 共享 Agent 构建（ACP 和 TUI 共用）─────────────────────────────────────────

/// 共享 Agent 构建配置（ACP 和 TUI 共用）
pub struct AcpAgentConfig {
    pub provider: LlmProvider,
    pub cwd: String,
    pub system_prompt: String,
    /// Frozen CLAUDE.md content (None = read from disk each turn, legacy).
    pub frozen_claude_md: Option<String>,
    /// Frozen CLAUDE.local.md content.
    pub frozen_claude_local_md: Option<String>,
    /// Frozen skills summary (None = scan each turn).
    pub frozen_skill_summary: Option<String>,
    /// Frozen session date in YYYY-MM-DD (None = compute fresh each turn).
    pub frozen_date: Option<String>,
    pub event_handler: Arc<dyn AgentEventHandler>,
    pub cancel: AgentCancellationToken,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub peri_config: Arc<PeriConfig>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    pub agent_overrides: Option<peri_middlewares::agent_define::AgentOverrides>,
    pub preload_skills: Vec<String>,
    pub session_id: Option<String>,
    pub broker: Arc<dyn UserInteractionBroker>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub hook_groups: Vec<Vec<RegisteredHook>>,
    pub hook_session_start: bool,
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    pub tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>>,
    /// 子 Agent 专用事件 handler factory（由调用方提供，取代 TUI 的 child_event_tx）
    pub child_handler_factory: Option<ChildHandlerFactory>,
    /// LSP 服务器配置（由调用方从 settings.json + 插件配置组装）
    pub lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    /// Compact 中间件配置（None = 不启用自动 compact）
    pub compact_config: Option<CompactConfig>,
    /// 上下文窗口预算（CompactMiddleware 需要）
    pub compact_budget: Option<ContextBudget>,
    /// LLM 模型（CompactMiddleware 用于 full compact 摘要生成）
    pub compact_model: Option<Arc<dyn BaseModel>>,
    /// 事件通道（CompactMiddleware 发送 compact 事件）
    pub compact_event_tx:
        Option<Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<ExecutorEvent>>>>>,
}

pub struct AcpAgentOutput {
    pub executor: ReActAgent<peri_agent::llm::RetryableLLM<BaseModelReactLLM>, AgentState>,
    pub todo_rx: tokio::sync::mpsc::Receiver<Vec<TodoItem>>,
    #[allow(dead_code)]
    pub context_window: u32,
}

/// 构建可复用的 Agent（ACP 和 TUI 共用核心构建逻辑）
///
/// 迁移自 peri-tui/src/app/agent.rs:build_bare_agent()。
/// 中间件链和 builder 配置与原函数完全一致。
pub fn build_agent(cfg: AcpAgentConfig) -> AcpAgentOutput {
    let AcpAgentConfig {
        provider,
        cwd,
        system_prompt,
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
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
        child_handler_factory,
        lsp_servers,
        compact_config: mw_compact_config,
        compact_budget: mw_compact_budget,
        compact_model: mw_compact_model,
        compact_event_tx: mw_compact_event_tx,
    } = cfg;

    // 应用 agent overrides 到系统提示词
    let system_prompt = agent_overrides.as_ref().map_or_else(
        || system_prompt.clone(),
        |ov| {
            let features = crate::prompt::PromptFeatures::detect();
            crate::prompt::build_system_prompt(Some(ov), &cwd, features, &plugin_agent_dirs, None)
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
    let (todo_tx, todo_rx) = tokio::sync::mpsc::channel::<Vec<TodoItem>>(8);

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
    let frozen_date_for_sub = frozen_date.clone();
    let system_builder: Arc<
        dyn Fn(Option<&peri_middlewares::agent_define::AgentOverrides>, &str) -> String
            + Send
            + Sync,
    > = Arc::new(move |overrides, cwd_dir| {
        let features = crate::prompt::PromptFeatures::detect();
        crate::prompt::build_system_prompt(
            overrides,
            cwd_dir,
            features,
            &[],
            frozen_date_for_sub.as_deref(),
        )
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
    let mut subagent = SubAgentMiddleware::new(
        parent_tools,
        Some(Arc::clone(&event_handler) as Arc<dyn AgentEventHandler>),
        llm_factory.clone(),
    )
    .with_system_builder(system_builder)
    .with_cancel(cancel.clone())
    .with_parent_messages(parent_messages)
    .with_background_registry(Arc::clone(&background_registry))
    .with_registered_hooks(vec![]);
    if let Some(factory) = child_handler_factory {
        subagent = subagent.with_child_handler_factory(factory);
    }

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

    // 将 Git Attribution 追加到系统提示词末尾（动态区域，不影响缓存前缀）
    let attribution = peri_middlewares::GitAttributionMiddleware::attribution_text(&model_name);
    let system_prompt = format!(
        "{}\n\n## Git Attribution\n\nWhen creating git commits, append the following line to the commit message:\n\n```\n{}\n```\n\nThis tracks AI contributions and should always be included in commit messages for code you authored.",
        system_prompt, attribution
    );

    // 构建 ReActAgent
    let executor = ReActAgent::new(model)
        .max_iterations(500)
        .with_context_budget(context_budget)
        .with_compact_config(compact_config)
        .with_notification_rx(bg_notification_rx)
        .with_system_prompt(system_prompt)
        .with_tool_filter(peri_middlewares::tool_search::is_deferred_tool)
        .with_shared_tools(Arc::clone(&shared_tools))
        .add_middleware(Box::new({
            let mut mw = AgentsMdMiddleware::new().with_excludes(claude_md_excludes);
            if let Some(main) = frozen_claude_md {
                mw = mw.with_frozen_content(main, frozen_claude_local_md);
            }
            mw
        }))
        .add_middleware(Box::new(AgentDefineMiddleware::new()))
        .add_middleware(Box::new({
            let mut mw = SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs);
            if let Some(summary) = frozen_skill_summary {
                mw = mw.with_frozen_summary(summary);
            }
            mw
        }))
        .add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
        .add_middleware(Box::new(FilesystemMiddleware::new()))
        .add_middleware(Box::new(peri_middlewares::GitAttributionMiddleware::new(
            &model_name,
        )))
        .add_middleware(Box::new(TerminalMiddleware::new()))
        .add_middleware(Box::new(WebMiddleware::new()))
        .add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
        .add_middleware(Box::new(CronMiddleware::new(
            cron_scheduler.unwrap_or_else(|| {
                Arc::new(parking_lot::Mutex::new(CronScheduler::new(
                    tokio::sync::mpsc::unbounded_channel().0,
                )))
            }),
        )));

    // Hook middleware groups
    // 收集所有 hooks（在 hook_groups 被 move 之前，供 CompactMiddleware 和 HookMiddleware 共用）
    let all_hooks: Vec<RegisteredHook> = hook_groups.iter().flatten().cloned().collect();
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

    // LSP 中间件（条件注册，当有 LSP 服务器配置时）
    let executor = if !lsp_servers.is_empty() {
        let lsp_config = peri_lsp::config::LspConfigFile {
            lsp_servers: lsp_servers
                .into_iter()
                .map(|s| (s.name.clone(), s))
                .collect(),
        };
        tracing::info!(
            target: "lsp",
            servers = lsp_config.lsp_servers.len(),
            "LSP 中间件已注册"
        );
        executor.add_middleware(Box::new(peri_middlewares::LspMiddleware::new(
            cwd.clone(),
            lsp_config,
        )))
    } else {
        executor
    };

    // CompactMiddleware（条件注册，当 compact 配置+模型+事件通道均可用时）
    let executor = if let (Some(config), Some(budget), Some(model), Some(event_tx)) = (
        mw_compact_config,
        mw_compact_budget,
        mw_compact_model,
        mw_compact_event_tx,
    ) {
        let compact_mw = CompactMiddleware::new(
            Some(model),
            config,
            budget,
            cwd.clone(),
            event_tx,
            cancel.clone(),
            all_hooks,
            session_id.unwrap_or_default(),
            provider_name.clone(),
        );
        executor.add_middleware(Box::new(compact_mw))
    } else {
        executor
    };

    AcpAgentOutput {
        executor,
        todo_rx,
        context_window,
    }
}
