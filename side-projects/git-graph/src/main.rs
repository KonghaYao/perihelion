mod app;
mod editor;
mod event;
mod git;
mod graph;
mod render;
mod theme;
mod ui;
mod update;

use anyhow::Result;
use app::App;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gig", about = "Git Graph TUI", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 仓库路径（默认当前目录）
    repo_path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// 更新 gig 到最新版本
    Update,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Update) => {
            update::run_update()?;
        }
        None => {
            let repo_path = cli.repo_path.unwrap_or_else(|| PathBuf::from("."));
            run_tui(&repo_path)?;
        }
    }

    Ok(())
}

fn run_tui(repo_path: &std::path::Path) -> Result<()> {
    let repo = git::repo::GitRepo::open(repo_path)?;
    let mut app = App::new(repo)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    while app.running {
        // 刷新 sidebar 数据（每 2 秒自动刷新 git status）
        app.refresh_sidebar();

        // Toast 过期检测
        if let Some(toast) = &app.toast {
            if std::time::Instant::now() >= toast.expires_at {
                app.toast = None;
                app.dirty = true;
            }
        }

        // 检查后台语法高亮进度（非阻塞）
        app.check_preview_progress();
        if let Some(ref mut editor) = app.editor {
            let scroll_y = editor.scroll_y();
            let vp_h = app.editor_area.height.saturating_sub(1) as usize; // -1 status bar
            if editor.sync_highlight_visible(scroll_y, vp_h) {
                app.dirty = true;
            }
        }

        // 只在 dirty 时重绘
        if app.dirty {
            terminal.draw(|f| render::draw(f, &mut app))?;
            app.dirty = false;
        }

        // 更新视口尺寸（实际高度由 graph_panel 渲染时设置）
        let area = terminal.size()?;
        let sidebar_width = area.width * 25 / 100;
        let graph_pct = area.width * 45 / 100;
        let new_graph_width = graph_pct;
        let new_detail_width = area.width - sidebar_width - graph_pct;
        if app.graph_width != new_graph_width || app.detail_width != new_detail_width {
            app.graph_width = new_graph_width;
            app.detail_width = new_detail_width;
            app.dirty = true;
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            let event = crossterm::event::read()?;

            // 鼠标禁用时跳过鼠标事件，让终端原生选择复制可用
            if !app.mouse_enabled {
                if let crossterm::event::Event::Mouse(_) = event {
                    continue;
                }
            }

            // 鼠标模式切换：切换后需要通知终端
            let prev_mouse = app.mouse_enabled;
            event::handle_event(&mut app, event)?;
            if app.mouse_enabled != prev_mouse {
                if app.mouse_enabled {
                    execute!(terminal.backend_mut(), EnableMouseCapture)?;
                } else {
                    execute!(terminal.backend_mut(), DisableMouseCapture)?;
                }
            }
            app.dirty = true;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
