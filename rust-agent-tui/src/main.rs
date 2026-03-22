mod app;
mod command;
mod config;
mod event;
mod prompt;
mod thread;
mod ui;

use anyhow::Result;
use ratatui::{
    crossterm::{
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode},
        event::{EnableMouseCapture, DisableMouseCapture},
    },
    prelude::*,
};
use std::io;

fn main() -> Result<()> {
    // 加载 .env 文件（仅开发环境，文件不存在时静默忽略）
    let _ = dotenvy::dotenv();

    // 解析命令行参数：--yolo / -y 启用 YOLO 模式（禁用 HITL 审批）
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--yolo" || a == "-y") {
        std::env::set_var("YOLO_MODE", "true");
    }

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
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // 运行应用
        let result = run_app(&mut terminal).await;

        // 恢复终端
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
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

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = app::App::new();

    loop {
        terminal.draw(|f| ui::main_ui::render(f, &mut app))?;

        // 轮询后台 agent 结果
        app.poll_agent();

        if let Some(action) = event::next_event(&mut app).await? {
            match action {
                event::Action::Quit => break,
                event::Action::Submit(input) => {
                    app.submit_message(input);
                }
                event::Action::Redraw => {}
            }
        }
    }

    Ok(())
}
