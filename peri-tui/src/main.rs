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
use peri_tui::acp_server::{run_acp_server, AcpServerConfig};
use peri_tui::app::App;
use peri_tui::event;
use peri_tui::ui;
use std::sync::Arc;

mod acp_stdio;
mod cli_args;
mod cli_plugin;

// ─── CLI 定义 ──────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "peri", version, about = "Peri AI Agent")]
struct Cli {
    // ── 向后兼容 ──
    /// 向后兼容，无操作（YOLO 已是默认行为）
    #[arg(short = 'y', long = "yolo")]
    yolo: bool,
    /// 启用 HITL 审批模式（等同 --permission-mode default）
    #[arg(short = 'a', long = "approve")]
    approve: bool,

    // ── 非交互模式 ──
    /// 非交互模式：输出响应后退出
    #[arg(short = 'p', long = "print")]
    print: Option<Option<String>>,
    /// 输出格式：text / json / stream-json（需 -p）
    #[arg(long = "output-format", visible_alias = "outputFormat")]
    output_format: Option<String>,
    /// 最大 agentic 轮数（需 -p）
    #[arg(long = "max-turns", visible_alias = "maxTurns")]
    max_turns: Option<u32>,
    /// 极简模式：跳过 hooks/LSP/插件等初始化（需 -p）
    #[arg(long = "bare")]
    bare: bool,

    // ── 权限与安全 ──
    /// 权限模式：bypass / default / dont-ask / accept-edit / auto-mode
    #[arg(long = "permission-mode", visible_alias = "permissionMode")]
    permission_mode: Option<String>,
    /// 绕过所有权限检查（仅限沙箱环境）
    #[arg(long = "dangerously-skip-permissions")]
    skip_permissions: bool,

    // ── 模型与推理 ──
    /// 指定模型（别名如 sonnet 或全名）
    #[arg(long = "model")]
    model: Option<String>,
    /// 推理强度：low / medium / high / max
    #[arg(long = "effort")]
    effort: Option<String>,

    // ── 会话与对话 ──
    /// 继续当前目录最近的对话
    #[arg(short = 'c', long = "continue")]
    cont: bool,
    /// 按 session ID 恢复对话
    #[arg(short = 'r', long = "resume")]
    resume: Option<Option<String>>,
    /// 指定会话 ID（必须是有效 UUID）
    #[arg(long = "session-id", visible_alias = "sessionId")]
    session_id: Option<String>,
    /// 设置会话显示名称
    #[arg(short = 'n', long = "name")]
    session_name: Option<String>,
    /// 禁用会话持久化（需 -p）
    #[arg(long = "no-session-persistence")]
    no_session_persistence: bool,

    // ── 工具控制 ──
    /// 允许的工具列表（如 "Bash(git:*)" "Edit"）
    #[arg(long = "allowedTools", visible_alias = "allowed-tools")]
    allowed_tools: Option<Vec<String>>,
    /// 禁止的工具列表
    #[arg(long = "disallowedTools", visible_alias = "disallowed-tools")]
    disallowed_tools: Option<Vec<String>>,

    // ── 配置 ──
    /// 加载额外 settings 文件或 JSON 字符串
    #[arg(long = "settings")]
    settings: Option<String>,

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
        /// Agent 类型（从 .claude/agents/ 中选择）
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
    /// 插件管理
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// 发送本地配置到远端设备
    Sender,
    /// 从远端设备接收配置
    Receiver,
}

#[derive(Subcommand)]
enum PluginAction {
    /// 列出已安装的插件
    List {
        /// JSON 输出
        #[arg(long)]
        json: bool,
    },
    /// 安装插件
    Install {
        /// 插件名称（格式: name@marketplace）
        plugin: String,
        /// 安装范围：user / project / local
        #[arg(short = 's', long, default_value = "user")]
        scope: String,
    },
    /// 卸载插件
    Uninstall {
        /// 插件 ID（格式: name@marketplace）
        plugin: String,
        /// 卸载范围（不指定则从所有范围移除）
        #[arg(short = 's', long)]
        scope: Option<String>,
    },
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
            rt.block_on(acp_stdio::run_acp_stdio(cwd))
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
        Some(Commands::Plugin { action }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                match action {
                    PluginAction::List { json } => cli_plugin::run_plugin_list(json),
                    PluginAction::Install { plugin, scope } => {
                        cli_plugin::run_plugin_install(&plugin, &scope).await
                    }
                    PluginAction::Uninstall { plugin, scope } => {
                        cli_plugin::run_plugin_uninstall(&plugin, scope.as_deref()).await
                    }
                }
            })
        }
    }
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
            .and_then(peri_tui::app::LlmProvider::from_config)
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
                run_acp_server(Arc::new(server_transport), server_config).await;
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
