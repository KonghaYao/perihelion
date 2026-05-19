use anyhow::Result;
use clap::{Parser, Subcommand};
use ratatui::{
    crossterm::{
        event::{
            DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
            EnableFocusChange, EnableMouseCapture,
        },
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    prelude::*,
};
use std::io;

use peri_acp::transport::mpsc::mpsc_transport_pair;
use peri_tui::acp_client::AcpTuiClient;
use peri_tui::acp_server::{self, AcpServerConfig};
use peri_tui::app::App;
use peri_tui::event;
use peri_tui::ui;
use std::sync::Arc;

// ─── CLI 定义 ──────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "peri", version, about = "Peri AI Agent")]
struct Cli {
    /// 向后兼容，无操作（YOLO 已是默认行为）
    #[arg(short = 'y', long = "yolo")]
    yolo: bool,
    /// 启用 HITL 审批模式
    #[arg(short = 'a', long = "approve")]
    approve: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 以 ACP Agent 模式运行（stdin/stdout JSON-RPC）
    Acp {
        /// 工作目录
        #[arg(long, default_value = ".")]
        cwd: String,
        /// 模型名称/别名
        #[arg(long)]
        model: Option<String>,
        /// Agent 类型（从 .claude/agents/ 中选择，如 code-reviewer、explorer）
        #[arg(short = 'g', long)]
        agent: Option<String>,
    },
    /// 更新：从 GitHub 下载并安装最新版本
    Update,
    /// 配置同步：在设备间同步 settings/skills/mcp/plugins
    Sync {
        #[command(subcommand)]
        action: SyncAction,
        /// Relay server URL
        #[arg(long, default_value = "wss://peri-sync.claude-code-best.win")]
        server: String,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// 发送本地配置到远端设备
    Sender,
    /// 从远端设备接收配置
    Receiver,
}

// ─── 环境变量注入 ──────────────────────────────────────────────────────────

/// 从 settings.json 读取 env 字段并注入进程环境变量
/// 仅在进程环境变量不存在时设置（进程环境优先）
fn inject_env_from_settings() {
    let path = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".peri")
        .join("settings.json");

    if !path.exists() {
        return;
    }

    // 读取并解析 JSON
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    // 提取 config.env 字段
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return;
    };

    let Some(env_obj) = json.get("config").and_then(|c| c.get("env")) else {
        return;
    };

    let Some(env_map) = env_obj.as_object() else {
        return;
    };

    // 遍历键值对，仅在进程环境变量不存在时设置
    for (key, value) in env_map {
        if let Some(value_str) = value.as_str() {
            if std::env::var(key).is_err() {
                std::env::set_var(key, value_str);
            }
        }
    }
}

// ─── 入口 ──────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    // 最先注入环境变量（进程环境变量优先）
    inject_env_from_settings();

    let cli = Cli::parse();

    match cli.command {
        None => run_tui(cli.approve),
        Some(Commands::Acp {
            cwd,
            model: _,
            agent: _,
        }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_acp_stdio(cwd))
        }
        Some(Commands::Update) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                match peri_tui::update::run_update().await {
                    Ok(tag) => println!("Updated to {tag}"),
                    Err(e) => {
                        eprintln!("Update failed: {e:#}");
                        std::process::exit(1);
                    }
                }
                Ok(())
            })
        }
        Some(Commands::Sync { action, server }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                match action {
                    SyncAction::Sender => peri_tui::sync::run_sync_sender(&server).await,
                    SyncAction::Receiver => peri_tui::sync::run_sync_receiver(&server).await,
                }
            })
            .map(|_| println!("Sync complete"))
            .map_err(|e| {
                eprintln!("Sync failed: {e:#}");
                std::process::exit(1);
            })
        }
    }
}

// ─── ACP Stdio 模式（标准 agent-client-protocol SDK）─────────────────────

struct SessionInfo {
    #[allow(dead_code)]
    session_id: String,
    cwd: String,
    history: Vec<peri_agent::messages::BaseMessage>,
    cancel_token: Option<peri_agent::agent::AgentCancellationToken>,
}

struct StdioContext {
    provider: parking_lot::RwLock<peri_tui::app::agent::LlmProvider>,
    peri_config: parking_lot::RwLock<peri_tui::config::PeriConfig>,
    permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    cron_scheduler: Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>,
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    plugin_skill_dirs: Vec<std::path::PathBuf>,
    plugin_agent_dirs: Vec<std::path::PathBuf>,
    hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    plugin_lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    sessions: parking_lot::RwLock<std::collections::HashMap<String, SessionInfo>>,
    session_counter: std::sync::atomic::AtomicU64,
}

/// Stdio 模式下的简化 Broker：直接 approve 所有权限请求，questions 返回空答案。
struct StdioBroker;

impl StdioBroker {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl peri_agent::interaction::UserInteractionBroker for StdioBroker {
    async fn request(
        &self,
        context: peri_agent::interaction::InteractionContext,
    ) -> peri_agent::interaction::InteractionResponse {
        match context {
            peri_agent::interaction::InteractionContext::Approval { items } => {
                peri_agent::interaction::InteractionResponse::Decisions(
                    items
                        .into_iter()
                        .map(|_| peri_agent::interaction::ApprovalDecision::Approve)
                        .collect(),
                )
            }
            peri_agent::interaction::InteractionContext::Questions { requests } => {
                peri_agent::interaction::InteractionResponse::Answers(
                    requests
                        .into_iter()
                        .map(|q| peri_agent::interaction::QuestionAnswer {
                            id: q.id,
                            selected: vec![],
                            text: Some(String::new()),
                        })
                        .collect(),
                )
            }
        }
    }
}

async fn run_acp_stdio(cwd: String) -> Result<()> {
    let _telemetry = peri_agent::telemetry::init_tracing("peri-acp");

    // 解析工作目录
    let cwd = std::path::Path::new(&cwd)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&cwd))
        .to_string_lossy()
        .to_string();

    // 加载配置
    let peri_config = peri_tui::config::load().unwrap_or_default();
    let provider = peri_tui::app::agent::LlmProvider::from_config(&peri_config)
        .or_else(peri_tui::app::agent::LlmProvider::from_env)
        .ok_or_else(|| anyhow::anyhow!("No LLM provider configured. Set ANTHROPIC_API_KEY or OPENAI_API_KEY, or configure ~/.peri/settings.json"))?;

    tracing::info!(
        provider = %provider.display_name(),
        model = %provider.model_name(),
        cwd = %cwd,
        "ACP stdio mode starting"
    );

    // 初始化 cron scheduler
    let cron_scheduler = {
        let scheduler =
            peri_middlewares::cron::CronScheduler::new(tokio::sync::mpsc::unbounded_channel().0);
        Arc::new(parking_lot::Mutex::new(scheduler))
    };

    // 初始化 MCP 连接池（后台）
    let mcp_pool = {
        use peri_middlewares::mcp::{McpClientPool, McpInitStatus};
        let pool = Arc::new(McpClientPool::new_pending());
        let pool_clone = pool.clone();
        let (init_tx, _init_rx) = tokio::sync::watch::channel(McpInitStatus::Pending);
        let cwd_clone = cwd.clone();
        let claude_home = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        tokio::spawn(async move {
            McpClientPool::run_initialize(
                pool_clone,
                std::path::Path::new(&cwd_clone),
                &claude_home,
                init_tx,
                None,
            )
            .await;
        });
        Some(pool)
    };

    // 加载插件数据
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");
    let plugin_data = peri_middlewares::plugin::load_enabled_plugins_aggregated(&claude_dir);

    let plugin_skill_dirs = plugin_data.all_skill_dirs.clone();
    let plugin_agent_dirs = plugin_data.all_agent_dirs.clone();
    let plugin_lsp_servers = plugin_data.all_lsp_servers.clone();
    let plugin_hooks = plugin_data.all_hooks.clone();

    // 组装 hook groups
    let mut hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>> = Vec::new();
    if !plugin_hooks.is_empty() {
        hook_groups.push(plugin_hooks);
    }
    let local_hooks = peri_middlewares::hooks::loader::load_settings_local_hooks(&cwd);
    if !local_hooks.is_empty() {
        hook_groups.push(local_hooks);
    }

    let permission_mode = peri_middlewares::prelude::SharedPermissionMode::new(
        peri_middlewares::prelude::PermissionMode::AutoMode,
    );
    let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new());
    let shared_tools = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

    // 构建共享的 ServerContext，所有请求处理器通过 Arc 共享
    let ctx = Arc::new(StdioContext {
        provider: parking_lot::RwLock::new(provider),
        peri_config: parking_lot::RwLock::new(peri_config),
        permission_mode,
        cron_scheduler,
        mcp_pool,
        plugin_skill_dirs,
        plugin_agent_dirs,
        hook_groups,
        plugin_lsp_servers,
        tool_search_index,
        shared_tools,
        sessions: parking_lot::RwLock::new(std::collections::HashMap::new()),
        session_counter: std::sync::atomic::AtomicU64::new(0),
    });

    use agent_client_protocol::schema::{
        AgentCapabilities, CancelNotification, InitializeRequest, InitializeResponse,
        NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, ProtocolVersion,
        SessionId, SessionNotification, SetSessionConfigOptionRequest,
        SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
        SetSessionModelRequest, SetSessionModelResponse, StopReason,
    };
    use agent_client_protocol::{Agent, Client, ConnectionTo};
    use agent_client_protocol_tokio::Stdio;
    use peri_acp::event::map_executor_to_updates;
    use peri_acp::prompt::{build_system_prompt, PromptFeatures};
    use peri_agent::agent::events::{AgentEventHandler, FnEventHandler};
    use peri_agent::agent::react::AgentInput;
    use peri_agent::agent::state::AgentState;
    use peri_agent::agent::AgentCancellationToken;

    let ctx_clone = ctx.clone();

    Agent
        .builder()
        .name("peri-acp")
        // ── initialize ──
        .on_receive_request(
            async move |_req: InitializeRequest, responder, _cx| {
                tracing::info!("ACP initialize");
                responder.respond(
                    InitializeResponse::new(ProtocolVersion::V1)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/new ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: NewSessionRequest, responder, _cx| {
                    let cwd_str = req.cwd.to_string_lossy().to_string();
                    let mut sessions = ctx.sessions.write();
                    let counter = ctx.session_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let sid = format!("session-{}", counter);
                    sessions.insert(
                        sid.clone(),
                        SessionInfo {
                            session_id: sid.clone(),
                            cwd: cwd_str,
                            history: Vec::new(),
                            cancel_token: None,
                        },
                    );
                    tracing::info!(session_id = %sid, "ACP session created");
                    drop(sessions);
                    let modes = acp_server::build_mode_state(&ctx.permission_mode);
                    let models = {
                        let p = ctx.provider.read();
                        let c = ctx.peri_config.read();
                        acp_server::build_model_state(&p, &c)
                    };
                    let config_options = {
                        let c = ctx.peri_config.read();
                        acp_server::build_config_options(&c)
                    };
                    responder.respond(
                        NewSessionResponse::new(SessionId::new(&*sid))
                            .modes(modes)
                            .models(models)
                            .config_options(config_options),
                    )
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/prompt ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                    let sid = req.session_id.0.to_string();
                    let content: String = req.prompt.iter().filter_map(|b| {
                        if let agent_client_protocol::schema::ContentBlock::Text(t) = b {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    }).collect::<Vec<&str>>().join("");

                    let (agent_cwd, is_empty_history) = {
                        let sessions = ctx.sessions.read();
                        match sessions.get(&sid) {
                            Some(s) => (s.cwd.clone(), s.history.is_empty()),
                            None => {
                                let _ = responder.respond(PromptResponse::new(StopReason::EndTurn));
                                return Ok(());
                            }
                        }
                    };

                    let agent_input = AgentInput::text(content);
                    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<peri_agent::agent::events::AgentEvent>();
                    let cancel = AgentCancellationToken::new();
                    {
                        let mut sessions = ctx.sessions.write();
                        if let Some(s) = sessions.get_mut(&sid) {
                            s.cancel_token = Some(cancel.clone());
                        }
                    }

                    let event_handler: Arc<dyn AgentEventHandler> =
                        Arc::new(FnEventHandler(move |event| {
                            let _ = event_tx.send(event);
                        }));

                    let features = PromptFeatures::detect();
                    let system_prompt = build_system_prompt(None, &agent_cwd, features, &ctx.plugin_agent_dirs);
                    let context_window = ctx.provider.read().context_window();

                    let broker: Arc<dyn peri_agent::interaction::UserInteractionBroker> =
                        Arc::new(StdioBroker::new());

                    let provider_snapshot = ctx.provider.read().clone();
                    let peri_config_snapshot = ctx.peri_config.read().clone();
                    let agent_output = acp_server::build_agent_bridge(
                        &provider_snapshot,
                        &agent_cwd,
                        system_prompt,
                        event_handler,
                        cancel.clone(),
                        ctx.permission_mode.clone(),
                        Arc::new(peri_config_snapshot),
                        Some(ctx.cron_scheduler.clone()),
                        sid.clone(),
                        broker,
                        ctx.plugin_skill_dirs.clone(),
                        ctx.plugin_agent_dirs.clone(),
                        ctx.hook_groups.clone(),
                        is_empty_history,
                        ctx.mcp_pool.clone(),
                        ctx.tool_search_index.clone(),
                        ctx.shared_tools.clone(),
                        ctx.plugin_lsp_servers.clone(),
                    );

                    // Pump events → SessionNotification
                    let sid_for_pump = SessionId::new(&*sid);
                    let cx_clone = cx.clone();
                    let (pump_done_tx, pump_done_rx) = tokio::sync::oneshot::channel();
                    tokio::spawn(async move {
                        while let Some(exec_event) = event_rx.recv().await {
                            let updates = map_executor_to_updates(&exec_event, context_window);
                            for update in updates {
                                let notif = SessionNotification::new(sid_for_pump.clone(), update);
                                if let Err(e) = cx_clone.send_notification(notif) {
                                    tracing::error!(error = %e, "Failed to send SessionNotification");
                                    break;
                                }
                            }
                        }
                        let _ = pump_done_tx.send(());
                    });

                    let mut agent_state = AgentState::with_messages(agent_cwd, {
                        let sessions = ctx.sessions.read();
                        sessions.get(&sid).map(|s| s.history.clone()).unwrap_or_default()
                    });
                    let result = agent_output.executor.execute(agent_input, &mut agent_state, Some(cancel)).await;
                    drop(agent_output);

                    let _ = pump_done_rx.await;

                    if result.is_ok() {
                        let mut sessions = ctx.sessions.write();
                        if let Some(s) = sessions.get_mut(&sid) {
                            s.history = agent_state.into_messages();
                        }
                    }

                    let _ = responder.respond(PromptResponse::new(StopReason::EndTurn));
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/set_mode ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: SetSessionModeRequest, responder, _cx| {
                    let mode_id = req.mode_id.0.as_ref();
                    let mode = acp_server::parse_permission_mode(mode_id);
                    ctx.permission_mode.store(mode);
                    tracing::info!(mode_id = %mode_id, "Permission mode changed");
                    responder.respond(SetSessionModeResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/set_model ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: SetSessionModelRequest, responder, _cx| {
                    let model_id = req.model_id.0.to_string();
                    let new_provider = {
                        let cfg = ctx.peri_config.read();
                        peri_tui::app::agent::LlmProvider::from_config_for_alias(&cfg, &model_id)
                    };
                    if let Some(new_provider) = new_provider {
                        tracing::info!(model_id = %model_id, model = %new_provider.model_name(), "Model changed");
                        *ctx.provider.write() = new_provider;
                    }
                    responder.respond(SetSessionModelResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/set_config_option ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: SetSessionConfigOptionRequest, responder, _cx| {
                    let config_id = req.config_id.0.as_ref();
                    match &req.value {
                        agent_client_protocol_schema::SessionConfigOptionValue::ValueId { value } => {
                            let effort = value.0.as_ref();
                            if config_id == "thinking_effort" {
                                acp_server::apply_thinking_effort(&ctx.peri_config, effort);
                                tracing::info!(effort = %effort, "Thinking effort changed via configOption");
                            }
                        }
                        agent_client_protocol_schema::SessionConfigOptionValue::Boolean { value: _ } => {
                            tracing::debug!(config_id = %config_id, "Boolean config option not handled");
                        }
                        _ => {
                            tracing::debug!(config_id = %config_id, "Unknown config option value type");
                        }
                    }
                    let config_options = {
                        let cfg = ctx.peri_config.read();
                        acp_server::build_config_options(&cfg)
                    };
                    responder.respond(SetSessionConfigOptionResponse::new(config_options))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/cancel ──
        .on_receive_notification(
            {
                let ctx = ctx_clone.clone();
                async move |_notif: CancelNotification, _cx| {
                    let sid: &str = &_notif.session_id.0;
                    let sessions = ctx.sessions.read();
                    if let Some(s) = sessions.get(sid) {
                        if let Some(ref token) = s.cancel_token {
                            token.cancel();
                            tracing::info!(session_id = %sid, "Cancel requested");
                        }
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await
        .map_err(|e| anyhow::anyhow!("ACP error: {e}"))
}

// ─── TUI 模式 ──────────────────────────────────────────────────────────────

fn run_tui(approve: bool) -> Result<()> {
    if approve {
        std::env::set_var("YOLO_MODE", "false");
    }

    // 在创建 tokio runtime 之前初始化 tracing，确保 reqwest::blocking::Client
    // 的内部 runtime 与应用 runtime 完全隔离，避免嵌套 runtime drop panic。
    let _telemetry = peri_agent::telemetry::init_tracing("agent-tui");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let result = rt.block_on(async {
        // 初始化终端
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // 运行应用
        let result = run_app(&mut terminal).await;

        // 恢复终端
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste,
            DisableFocusChange
        )?;
        terminal.show_cursor()?;

        result
    });

    // 先 drop rt（关闭所有 tokio 任务），再 drop _telemetry
    drop(rt);
    drop(_telemetry);

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new().await;

    // 根据环境变量/CLI 参数设置初始权限模式
    {
        use peri_middlewares::prelude::PermissionMode;
        let initial_mode = if std::env::var("YOLO_MODE")
            .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
            .unwrap_or(true)
        {
            PermissionMode::Bypass
        } else {
            PermissionMode::Default
        };
        app.services.permission_mode.store(initial_mode);
    }

    // 检测是否需要 Setup 向导
    if let Some(ref cfg) = app.services.peri_config {
        if peri_tui::app::setup_wizard::needs_setup(&cfg.config) {
            app.global_ui.setup_wizard = Some(peri_tui::app::SetupWizardPanel::new());
        }
    } else {
        // 无配置文件 → 必然需要 setup
        app.global_ui.setup_wizard = Some(peri_tui::app::SetupWizardPanel::new());
    }

    // 后台初始化 MCP 连接池（不阻塞 UI）
    app.spawn_mcp_init();

    // 加载已启用插件数据
    {
        let claude_dir = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        app.services.plugin_data = Some(peri_middlewares::plugin::load_enabled_plugins_aggregated(
            &claude_dir,
        ));
        // 将插件命令注册到所有 session 的 CommandRegistry
        let plugin_commands = app
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_commands.clone())
            .unwrap_or_default();
        // 将插件 skills 追加到所有 session 的 skill 列表
        let plugin_skill_dirs = app
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_skill_dirs.clone())
            .unwrap_or_default();
        let plugin_skills = peri_middlewares::skills::list_skills(&plugin_skill_dirs);
        for session in &mut app.session_mgr.sessions {
            session
                .commands
                .command_registry
                .register_plugin_commands(plugin_commands.clone());
        }
        for session in &mut app.session_mgr.sessions {
            let existing_names: std::collections::HashSet<String> = session
                .commands
                .skills
                .iter()
                .map(|s| s.name.clone())
                .collect();
            for skill in &plugin_skills {
                if !existing_names.contains(&skill.name) {
                    session.commands.skills.push(skill.clone());
                }
            }
        }
    }

    // ── Step 6-a: Setup ACP Server + Client ──────────────────────────────
    {
        let provider = app
            .services
            .peri_config
            .as_ref()
            .and_then(|cfg| peri_tui::app::LlmProvider::from_config(cfg))
            .or_else(peri_tui::app::LlmProvider::from_env);

        if let Some(provider) = provider {
            // Gather plugin configs
            let plugin_skill_dirs = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_skill_dirs.clone())
                .unwrap_or_default();
            let plugin_agent_dirs = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_agent_dirs.clone())
                .unwrap_or_default();
            let plugin_lsp_servers = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_lsp_servers.clone())
                .unwrap_or_default();
            let plugin_hooks = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_hooks.clone())
                .unwrap_or_default();

            // Build hook groups from plugin hooks + local hooks
            let mut hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>> = Vec::new();
            if !plugin_hooks.is_empty() {
                hook_groups.push(plugin_hooks);
            }
            let local_hooks =
                peri_middlewares::hooks::loader::load_settings_local_hooks(&app.services.cwd);
            if !local_hooks.is_empty() {
                hook_groups.push(local_hooks);
            }

            let flat_hooks: Vec<peri_middlewares::hooks::RegisteredHook> =
                hook_groups.iter().flatten().cloned().collect();

            // Create session-level tool_search_index and shared_tools
            let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new());
            let shared_tools = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

            let server_config = AcpServerConfig {
                provider: Arc::new(parking_lot::RwLock::new(provider.clone())),
                peri_config: Arc::new(parking_lot::RwLock::new(
                    app.services.peri_config.clone().unwrap_or_default(),
                )),
                permission_mode: app.services.permission_mode.clone(),
                cron_scheduler: Some(app.services.cron.scheduler.clone()),
                mcp_pool: app.services.mcp_pool.clone(),
                plugin_skill_dirs,
                plugin_agent_dirs,
                plugin_hooks: flat_hooks,
                hook_groups,
                plugin_lsp_servers,
                tool_search_index: tool_search_index.clone(),
                shared_tools: shared_tools.clone(),
                thread_store: app.services.thread_store.clone(),
            };

            let (client_transport, server_transport) = mpsc_transport_pair();
            tokio::spawn(async move {
                acp_server::run_acp_server(Arc::new(server_transport), server_config).await;
            });

            let (acp_client, notification_rx) = AcpTuiClient::new(client_transport);
            // Spawn notification pump
            acp_client.spawn_pump();
            // Wire notification receiver to active session's AgentComm
            app.session_mgr.sessions[app.session_mgr.active]
                .agent
                .acp_notification_rx = Some(notification_rx);
            app.acp_client = Some(acp_client);
        }
    }

    // Spinner tick 驱动：每次渲染前推进一帧
    app.session_mgr.sessions[app.session_mgr.active]
        .spinner_state
        .advance_tick();

    // 初始全量绘制一次
    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;

    'event_loop: loop {
        // 推进所有 session 的 Spinner 动画帧
        for i in 0..app.session_mgr.sessions.len() {
            app.session_mgr.sessions[i].spinner_state.advance_tick();
        }
        // 轮询所有 session 的 agent 结果
        let mut agent_updated = false;
        for i in 0..app.session_mgr.sessions.len() {
            let prev_active = app.session_mgr.active;
            app.session_mgr.active = i;
            agent_updated |= app.poll_agent();
            app.session_mgr.active = prev_active;
        }
        // 轮询后台事件（MCP OAuth 等）
        let bg_updated = app.poll_background_events();
        // 检查 cron 定时触发
        app.poll_cron_triggers();

        match event::next_event(&mut app).await? {
            Some(action) => match action {
                event::Action::Quit => break 'event_loop,
                event::Action::Submit(input) => {
                    app.submit_message(input);
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
                event::Action::Redraw => {
                    // 有用户交互（键盘/鼠标/resize）→ 始终重绘
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
            },
            None => {
                // 无用户事件（poll 超时）：在阻塞结束后重新读取缓存版本
                // 这样能捕获渲染线程在等待期间发出的更新
                let cache_version = app.session_mgr.sessions[app.session_mgr.active]
                    .messages
                    .render_cache
                    .read()
                    .version;
                let cache_updated = cache_version
                    != app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .last_render_version;
                if cache_updated
                    || agent_updated
                    || bg_updated
                    || app.session_mgr.sessions[app.session_mgr.active].ui.loading
                {
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
            }
        }
        // /exit 或 /quit 命令设置的退出标志
        if app.global_ui.quit_requested {
            break 'event_loop;
        }
    }

    // 关闭 MCP 连接池（断开所有 MCP 服务器连接，清理子进程）
    if let Some(pool) = app.services.mcp_pool.take() {
        tracing::info!("正在关闭 MCP 连接池...");
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(pool.shutdown()));
        tracing::info!("MCP 连接池已关闭");
    }

    // 等待最后一次 Langfuse flush 完成，防止 runtime drop 前 batcher 数据丢失
    if let Some(handle) = app.session_mgr.sessions[app.session_mgr.active]
        .langfuse
        .langfuse_flush_handle
        .take()
    {
        let _ = handle.await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_priority_process_over_settings() {
        // 测试进程环境变量优先于 settings.json
        // 设置一个测试环境变量
        std::env::set_var("TEST_ENV_PRIORITY_VAR", "from_process");

        // 调用注入函数（即使 settings.json 存在该变量也不应覆盖）
        inject_env_from_settings();

        // 验证进程环境变量未被覆盖
        assert_eq!(
            std::env::var("TEST_ENV_PRIORITY_VAR").unwrap(),
            "from_process"
        );

        // 清理
        std::env::remove_var("TEST_ENV_PRIORITY_VAR");
    }
}
// test
