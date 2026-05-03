use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::ui::theme;

pub(crate) fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_first_row(f, app, rows[0]);
    render_second_row(f, app, rows[1]);
    // 第三行留空，作为视觉缓冲
}

/// 第一行：权限模式 │ 工作目录 │ 模型名
fn render_first_row(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    // 权限模式标签
    {
        use rust_agent_middlewares::prelude::PermissionMode;
        let mode = app.permission_mode.load();
        let (label, color) = match mode {
            PermissionMode::Default => ("", theme::TEXT),
            PermissionMode::DontAsk => ("Don't Ask", theme::WARNING),
            PermissionMode::AcceptEdit => ("Accept Edit", theme::THINKING),
            PermissionMode::AutoMode => ("Auto Mode", theme::WARNING),
            PermissionMode::Bypass => ("Bypass", theme::ERROR),
        };

        // Default 模式不显示标签
        if !label.is_empty() {
            let is_highlight = app
                .mode_highlight_until
                .map_or(false, |until| std::time::Instant::now() < until);
            let mut style = Style::default().fg(color);
            if is_highlight {
                style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
            }
            spans.push(Span::styled(format!(" {}", label), style));
        }
    }

    // 工作目录
    spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
    let cwd_short = std::path::Path::new(&app.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&app.cwd);
    spans.push(Span::styled(
        format!("📁 {}", cwd_short),
        Style::default().fg(theme::MUTED),
    ));

    // 模型名（只显示 model name）
    spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
    spans.push(Span::styled(
        format!(" {}", app.model_name),
        Style::default().fg(theme::MODEL_INFO),
    ));

    // 上下文使用率
    {
        let tracker = &app.agent.session_token_tracker;
        if let Some(pct) = tracker.context_usage_percent(app.agent.context_window) {
            let used = tracker.estimated_context_tokens().unwrap_or(0);
            let total = app.agent.context_window;
            let color = if pct >= 85.0 {
                theme::ERROR
            } else if pct >= 70.0 {
                theme::WARNING
            } else {
                theme::SAGE
            };
            spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
            spans.push(Span::styled(
                format!(
                    "ctx: {:.0}% ({:.0}K/{:.0}K)",
                    pct,
                    used as f64 / 1000.0,
                    total as f64 / 1000.0
                ),
                Style::default().fg(color),
            ));
        }
    }

    // 重试状态
    if let Some(ref retry) = app.agent.retry_status {
        let delay_sec = retry.delay_ms as f64 / 1000.0;
        spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        spans.push(Span::styled(
            format!(
                " ⟳ 重试 {}/{} ({:.1}s)",
                retry.attempt, retry.max_attempts, delay_sec
            ),
            Style::default().fg(theme::WARNING),
        ));
    }

    // MCP 初始化进度
    if let Some(ref rx) = app.mcp_init_rx {
        let status = rx.borrow().clone();
        use rust_agent_middlewares::mcp::McpInitStatus;
        match status {
            McpInitStatus::Initializing { connected, total } => {
                spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
                spans.push(Span::styled(
                    format!(" [i] MCP ({}/{})...", connected, total),
                    Style::default().fg(theme::MUTED),
                ));
            }
            McpInitStatus::Ready { total } if total > 0 => {
                // 首次检测到 Ready 时设置 3 秒显示窗口
                if app.mcp_ready_shown_until.get().is_none() {
                    app.mcp_ready_shown_until.set(
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(3)),
                    );
                }
                if let Some(until) = app.mcp_ready_shown_until.get() {
                    if std::time::Instant::now() < until {
                        spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
                        spans.push(Span::styled(
                            format!(" [i] MCP ready ({} servers)", total),
                            Style::default().fg(theme::SAGE),
                        ));
                    }
                }
            }
            McpInitStatus::Failed(ref msg) => {
                spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
                spans.push(Span::styled(
                    format!(" [i] MCP failed: {}", msg),
                    Style::default().fg(theme::ERROR),
                ));
            }
            McpInitStatus::Pending | McpInitStatus::Ready { .. } => {}
        }
    }

    // 任务运行时长（仅在 loading 时显示）
    if app.core.loading {
        if let Some(duration) = app.get_current_task_duration() {
            let secs = duration.as_secs();
            let time_str = if secs >= 60 {
                format!("{}m{}s", secs / 60, secs % 60)
            } else {
                format!("{}s", secs)
            };
            spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
            spans.push(Span::styled(
                format!(" ⏱ {}", time_str),
                Style::default().fg(theme::MUTED),
            ));
        }
    }

    render_truncated_line(f, spans, Vec::new(), area);
}

/// 第二行：[Agent 面板信息] │ [快捷键提示]
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    let mut left_spans: Vec<Span> = Vec::new();
    let mut has_content = false;

    // 复制成功提示
    if let Some(until) = app.core.copy_message_until {
        if std::time::Instant::now() < until {
            left_spans.push(Span::styled(
                format!(" 已复制 {} 个字符", app.core.copy_char_count),
                Style::default().fg(theme::MUTED),
            ));
            has_content = true;
        }
    }

    // Agent 面板信息（仅面板激活时）
    if let Some(panel) = &app.core.agent_panel {
        if has_content {
            left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        }
        if let Some(agent) = panel.current_agent() {
            left_spans.push(Span::styled(
                format!(" {}", agent.name),
                Style::default().fg(theme::MUTED),
            ));
        } else {
            left_spans.push(Span::styled(" 无", Style::default().fg(theme::MUTED)));
        }
    } else if let Some(id) = app.get_agent_id() {
        if has_content {
            left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        }
        left_spans.push(Span::styled(
            format!(" {}", id),
            Style::default().fg(theme::MUTED),
        ));
    }

    // 右侧：快捷键提示（统一灰色显示）
    let key_style = Style::default().fg(theme::MUTED).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme::MUTED);

    macro_rules! key { ($($key:expr => $desc:expr),+ $(,)?) => { vec![$( Span::styled($key, key_style), Span::styled($desc, desc_style) ),+] } }

    let right_spans: Vec<Span> = match &app.agent.interaction_prompt {
        Some(crate::app::InteractionPrompt::Questions(_)) => {
            key![" Tab" => ":切换  ", "↑↓" => ":移动  ", "Space" => ":选择  ", "Enter" => ":确认"]
        }
        Some(crate::app::InteractionPrompt::Approval(_)) => {
            key![" ↑↓" => ":移动  ", "Space" => ":切换  ", "Enter" => ":确认"]
        }
        None => {
            if app.core.agent_panel.is_some() {
                key!["↑↓" => ":选择  ", "Enter" => ":确认  ", "Esc" => ":取消"]
            } else if let Some(cron_panel) = &app.cron.cron_panel {
                if cron_panel.confirm_delete {
                    key!["Enter" => ":确认  ", "其他键" => ":取消"]
                } else {
                    key!["↑↓" => ":移动  ", "Enter" => ":切换  ", "Ctrl+D" => ":删除  ", "Esc" => ":关闭"]
                }
            } else if let Some(login_panel) = &app.core.login_panel {
                use crate::app::login_panel::LoginPanelMode;
                match login_panel.mode {
                    LoginPanelMode::ConfirmDelete => {
                        key!["Enter" => ":确认删除  ", "Esc" => ":取消"]
                    }
                    LoginPanelMode::Edit | LoginPanelMode::New => {
                        key!["↑↓" => ":切换字段  ", "←→/Space" => ":切换Type  ", "Enter" => ":保存  ", "Ctrl+V" => ":粘贴  ", "Esc" => ":取消"]
                    }
                    LoginPanelMode::Browse => {
                        key!["Enter" => ":编辑  ", "Ctrl+N" => ":新建  ", "Ctrl+D" => ":删除  ", "Esc" => ":关闭"]
                    }
                }
            } else if app.mcp_panel.is_some() {
                let view_label = app.mcp_panel.as_ref().map(|p| match &p.view {
                    crate::app::McpPanelView::ServerList => {
                        if p.confirm_delete.is_some() {
                            key!["Enter" => ":确认  ", "其他键" => ":取消"]
                        } else {
                            key!["↑↓" => ":移动  ", "Enter" => ":详情  ", "Ctrl+R" => ":重连  ", "Ctrl+D" => ":删除  ", "Esc" => ":关闭"]
                        }
                    }
                    crate::app::McpPanelView::ServerDetail { .. } => {
                        key!["↑↓" => ":移动  ", "Tab" => ":切换  ", "Esc" => ":返回"]
                    }
                });
                view_label.unwrap_or_default()
            } else if app.core.config_panel.is_some() {
                let panel = app.core.config_panel.as_ref().unwrap();
                match panel.mode {
                    crate::app::config_panel::ConfigPanelMode::Browse => {
                        key!["↑↓" => ":导航  ", "Enter" => ":编辑  ", "Esc" => ":关闭"]
                    }
                    crate::app::config_panel::ConfigPanelMode::Edit => {
                        key!["↑↓" => ":切换字段  ", "←→/Space" => ":切换  ", "Enter" => ":保存  ", "Ctrl+V" => ":粘贴  ", "Esc" => ":取消"]
                    }
                }
            } else if app.core.model_panel.is_some() {
                key!["↑↓" => ":导航  ", "Enter" => ":确认  ", "Space" => ":选择/切换  ", "Esc" => ":关闭"]
            } else if app.status_panel.is_some() {
                key!["←→" => ":切换Tab  ", "Esc" => ":关闭"]
            } else if app.memory_panel.is_some() {
                key!["↑↓" => ":选择  ", "Enter" => ":编辑  ", "Esc" => ":关闭"]
            } else if let Some(browser) = &app.core.thread_browser {
                if browser.confirm_delete {
                    key!["Enter" => ":确认  ", "其他键" => ":取消"]
                } else {
                    key!["↑↓" => ":移动  ", "Enter" => ":确认  ", "Ctrl+D" => ":删除  ", "Esc" => ":关闭  ", "/" => ":搜索"]
                }
            } else {
                key!["/" => "命令  ", "Alt+Enter" => ":换行"]
            }
        }
    };

    render_truncated_line(f, left_spans, right_spans, area);
}

/// 渲染一行 spans，右侧右对齐，超出宽度时截断右侧
fn render_truncated_line(f: &mut Frame, left_spans: Vec<Span>, right_spans: Vec<Span>, area: Rect) {
    let left_width: usize = left_spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.width()).sum();

    let total_content_width = left_width + right_width;
    let padding = if total_content_width < area.width as usize {
        " ".repeat(area.width as usize - total_content_width)
    } else {
        " ".to_string()
    };

    let mut all_spans = left_spans;
    all_spans.push(Span::raw(padding));
    all_spans.extend(right_spans);

    f.render_widget(Paragraph::new(Line::from(all_spans)), area);
}
