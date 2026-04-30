use anyhow::Result;
use ratatui::{
    crossterm::{
        event::{
            DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        },
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    prelude::*,
};
use std::io;

use rust_agent_tui::app::App;
use rust_agent_tui::event;
use rust_agent_tui::ui;

/// 从 settings.json 读取 env 字段并注入进程环境变量
/// 仅在进程环境变量不存在时设置（进程环境优先）
fn inject_env_from_settings() {
    let path = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".zen-code")
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

fn main() -> Result<()> {
    // 最先注入环境变量（进程环境变量优先）
    inject_env_from_settings();

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    // -y/--yolo 保留为向后兼容的无操作（YOLO 已是默认行为）
    // --approve / -a 显式启用 HITL 审批（设置 YOLO_MODE=false）
    if args.iter().any(|a| a == "--approve" || a == "-a") {
        std::env::set_var("YOLO_MODE", "false");
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
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
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
            DisableBracketedPaste
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
    let mut app = App::new();

    // 根据环境变量/CLI 参数设置初始权限模式
    {
        use rust_agent_middlewares::prelude::PermissionMode;
        let initial_mode = if std::env::var("YOLO_MODE")
            .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
            .unwrap_or(true)
        {
            PermissionMode::Bypass
        } else {
            PermissionMode::Default
        };
        app.permission_mode.store(initial_mode);
    }

    // 检测是否需要 Setup 向导
    if let Some(ref cfg) = app.zen_config {
        if rust_agent_tui::app::setup_wizard::needs_setup(&cfg.config) {
            app.setup_wizard = Some(rust_agent_tui::app::SetupWizardPanel::new());
        }
    } else {
        // 无配置文件 → 必然需要 setup
        app.setup_wizard = Some(rust_agent_tui::app::SetupWizardPanel::new());
    }

    // Spinner tick 驱动：每次渲染前推进一帧
    app.spinner_state.advance_tick();

    // 初始全量绘制一次
    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;

    'event_loop: loop {
        // 推进 Spinner 动画帧
        app.spinner_state.advance_tick();
        // 轮询后台 agent 结果
        let agent_updated = app.poll_agent();
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
                let cache_version = app.core.render_cache.read().version;
                let cache_updated = cache_version != app.core.last_render_version;
                if cache_updated || agent_updated || app.core.loading {
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
            }
        }
    }

    // 等待最后一次 Langfuse flush 完成，防止 runtime drop 前 batcher 数据丢失
    if let Some(handle) = app.langfuse.langfuse_flush_handle.take() {
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
