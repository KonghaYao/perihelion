mod app;
mod command;
mod config;
mod event;
mod langfuse;
mod prompt;
mod thread;
mod ui;

use anyhow::Result;
use ratatui::{
    crossterm::{
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode},
        event::{EnableMouseCapture, DisableMouseCapture, EnableBracketedPaste, DisableBracketedPaste},
    },
    prelude::*,
};
use std::io;

fn main() -> Result<()> {
    // 加载 .env 文件（仅开发环境，文件不存在时静默忽略）
    let _ = dotenvy::dotenv();

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--yolo" || a == "-y") {
        std::env::set_var("YOLO_MODE", "true");
    }

    // 解析 --remote-control <url> [--relay-token <token>] [--relay-name <name>]
    let relay_cli = parse_relay_args(&args);

    // 在创建 tokio runtime 之前初始化 tracing，确保 reqwest::blocking::Client
    // 的内部 runtime 与应用 runtime 完全隔离，避免嵌套 runtime drop panic。
    let _telemetry = rust_create_agent::telemetry::init_tracing("agent-tui");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let result = rt.block_on(async {
        // 初始化终端
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // 运行应用
        let result = run_app(&mut terminal, relay_cli).await;

        // 恢复终端
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)?;
        terminal.show_cursor()?;

        result
    });

    // 先 drop rt（关闭所有 tokio 任务），再 drop _telemetry（flush + 关闭 OTel provider）
    // 此时已无任何 tokio 上下文，reqwest::blocking 的内部 runtime 可以安全 drop。
    drop(rt);
    drop(_telemetry);

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

/// CLI 参数解析结果：--remote-control [url] [--relay-token <token>] [--relay-name <name>]
/// url 为空字符串表示 `--remote-control` 无参数模式（从配置读取）
pub struct RelayCli {
    pub url: String,
    pub token: Option<String>,
    pub name: Option<String>,
}

fn parse_relay_args(args: &[String]) -> Option<RelayCli> {
    // 查找 --remote-control 参数位置
    let remote_idx = args.iter().position(|a| a == "--remote-control")?;

    // 检查是否有值（即 --remote-control <url> 格式）
    // 有值条件：下一个参数存在且不以 -- 开头
    let url = if remote_idx + 1 < args.len() && !args[remote_idx + 1].starts_with("--") {
        args[remote_idx + 1].clone()
    } else {
        // --remote-control 无参数，返回空字符串标记"从配置读取"
        String::new()
    };

    let token = args.windows(2)
        .find(|w| w[0] == "--relay-token")
        .map(|w| w[1].clone());
    let name = args.windows(2)
        .find(|w| w[0] == "--relay-name")
        .map(|w| w[1].clone());

    Some(RelayCli { url, token, name })
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, relay_cli: Option<RelayCli>) -> Result<()> {
    let mut app = app::App::new();

    // 尝试连接 Relay Server（CLI 参数优先，其次读 settings.json）
    app.try_connect_relay(relay_cli.as_ref()).await;

    // 初始全量绘制一次
    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;

    'event_loop: loop {
        // 轮询后台 agent 结果
        let agent_updated = app.poll_agent();
        // 轮询 Relay 事件（Web 端控制消息）
        let relay_updated = app.poll_relay();

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
                let cache_version = app.render_cache.read().version;
                let cache_updated = cache_version != app.last_render_version;
                if cache_updated || agent_updated || relay_updated {
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
            }
        }
    }

    // 等待最后一次 Langfuse flush 完成，防止 runtime drop 前 batcher 数据丢失
    if let Some(handle) = app.langfuse_flush_handle.take() {
        let _ = handle.await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_relay_args_no_url() {
        // --remote-control 无参数
        let args = vec!["agent-tui".to_string(), "--remote-control".to_string()];
        let result = parse_relay_args(&args);
        assert!(result.is_some());
        let relay = result.unwrap();
        assert_eq!(relay.url, "");
        assert!(relay.token.is_none());
        assert!(relay.name.is_none());
    }

    #[test]
    fn test_parse_relay_args_with_url() {
        // --remote-control ws://localhost:8080
        let args = vec![
            "agent-tui".to_string(),
            "--remote-control".to_string(),
            "ws://localhost:8080".to_string(),
        ];
        let result = parse_relay_args(&args);
        assert!(result.is_some());
        let relay = result.unwrap();
        assert_eq!(relay.url, "ws://localhost:8080");
        assert!(relay.token.is_none());
        assert!(relay.name.is_none());
    }

    #[test]
    fn test_parse_relay_args_with_all_params() {
        // --remote-control ws://localhost:8080 --relay-token abc123 --relay-name test
        let args = vec![
            "agent-tui".to_string(),
            "--remote-control".to_string(),
            "ws://localhost:8080".to_string(),
            "--relay-token".to_string(),
            "abc123".to_string(),
            "--relay-name".to_string(),
            "test".to_string(),
        ];
        let result = parse_relay_args(&args);
        assert!(result.is_some());
        let relay = result.unwrap();
        assert_eq!(relay.url, "ws://localhost:8080");
        assert_eq!(relay.token, Some("abc123".to_string()));
        assert_eq!(relay.name, Some("test".to_string()));
    }

    #[test]
    fn test_parse_relay_args_url_starts_with_dash() {
        // --remote-control 后的值如果以 -- 开头，应视为无参数模式
        let args = vec![
            "agent-tui".to_string(),
            "--remote-control".to_string(),
            "--other-flag".to_string(),
        ];
        let result = parse_relay_args(&args);
        assert!(result.is_some());
        let relay = result.unwrap();
        assert_eq!(relay.url, "");
    }

    #[test]
    fn test_parse_relay_args_none() {
        // 无 --remote-control 参数
        let args = vec!["agent-tui".to_string()];
        let result = parse_relay_args(&args);
        assert!(result.is_none());
    }
}
