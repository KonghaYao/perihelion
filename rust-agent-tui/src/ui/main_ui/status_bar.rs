use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{AgentPanel, App};
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
        let mode = app.services.permission_mode.load();
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
                .services
                .mode_highlight_until
                .is_some_and(|until| std::time::Instant::now() < until);
            let mut style = Style::default().fg(color);
            if is_highlight {
                style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
            }
            spans.push(Span::styled(format!(" {}", label), style));
        }
    }

    // 工作目录
    spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
    let cwd_short = std::path::Path::new(&app.services.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&app.services.cwd);
    spans.push(Span::styled(
        format!("📁 {}", cwd_short),
        Style::default().fg(theme::MUTED),
    ));

    // 模型名（只显示 model name）
    spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
    {
        let is_highlight = app
            .services
            .model_highlight_until
            .is_some_and(|until| std::time::Instant::now() < until);
        let mut style = Style::default().fg(theme::MODEL_INFO);
        if is_highlight {
            style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        }
        spans.push(Span::styled(format!(" {}", app.services.model_name), style));
    }

    // 上下文使用率 + 缓存命中率
    {
        let tracker = &app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .session_token_tracker;
        if let Some(pct) = tracker.context_usage_percent(
            app.session_mgr.sessions[app.session_mgr.active]
                .agent
                .context_window,
        ) {
            let total = app.session_mgr.sessions[app.session_mgr.active]
                .agent
                .context_window;
            let color = if pct >= 85.0 {
                theme::ERROR
            } else if pct >= 70.0 {
                theme::WARNING
            } else {
                theme::SAGE
            };
            spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
            // 缓存命中率（当次值，反映最近一次 LLM 调用的缓存效率）
            let cache_str = if tracker.cache_hit_rate() > 0.0 {
                format!(" {:.0}%", tracker.cache_hit_rate() * 100.0)
            } else {
                String::new()
            };
            spans.push(Span::styled(
                format!(
                    "ctx: {:.0}% [{:.0}k]{}",
                    pct,
                    total as f64 / 1000.0,
                    cache_str
                ),
                Style::default().fg(color),
            ));
        }
    }

    // 重试状态
    if let Some(ref retry) = app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .retry_status
    {
        let delay_sec = retry.delay_ms as f64 / 1000.0;
        spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        // 截取错误信息前 60 字符用于状态栏展示
        let err_preview: String = retry.error.chars().take(60).collect();
        let err_display = if retry.error.chars().count() > 60 {
            format!("{}...", err_preview)
        } else {
            err_preview
        };
        spans.push(Span::styled(
            format!(
                " 重试 {}/{} ({:.1}s): {}",
                retry.attempt, retry.max_attempts, delay_sec, err_display
            ),
            Style::default().fg(theme::WARNING),
        ));
    }

    // MCP 初始化进度
    if let Some(ref rx) = app.services.mcp_init_rx {
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
                if app.services.mcp_ready_shown_until.get().is_none() {
                    app.services.mcp_ready_shown_until.set(Some(
                        std::time::Instant::now() + std::time::Duration::from_secs(3),
                    ));
                }
                if let Some(until) = app.services.mcp_ready_shown_until.get() {
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

    // 进程内存监控
    {
        let mut monitor = app.services.resource_monitor.lock();
        monitor.refresh_if_needed();
        let mem = monitor.memory_mb();
        drop(monitor); // 释放锁后再渲染

        let mem_color = if mem > 1024 {
            theme::ERROR
        } else if mem > 512 {
            theme::WARNING
        } else {
            theme::SAGE
        };

        spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        spans.push(Span::styled(
            format!(" MEM {}MB", mem),
            Style::default().fg(mem_color),
        ));
    }

    // LSP 诊断计数
    let agent = &app.session_mgr.sessions[app.session_mgr.active].agent;
    if agent.lsp_errors > 0 || agent.lsp_warnings > 0 {
        spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        let diag = format!("diag: {}E/{}W", agent.lsp_errors, agent.lsp_warnings);
        spans.push(Span::styled(diag, Style::default().fg(theme::MUTED)));
    }

    render_truncated_line(f, spans, Vec::new(), area);
}

/// 第二行：[Agent 面板信息] │ [快捷键提示]
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    let mut left_spans: Vec<Span> = Vec::new();
    let mut has_content = false;

    // 复制成功提示
    if let Some(until) = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .copy_message_until
    {
        if std::time::Instant::now() < until {
            left_spans.push(Span::styled(
                format!(
                    " 已复制 {} 个字符",
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .copy_char_count
                ),
                Style::default().fg(theme::MUTED),
            ));
            has_content = true;
        }
    }

    // 后台任务指示器
    if app.session_mgr.sessions[app.session_mgr.active].background_task_count > 0 {
        if has_content {
            left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        }
        left_spans.push(Span::styled(
            format!(
                "[BG: {}]",
                app.session_mgr.sessions[app.session_mgr.active].background_task_count
            ),
            Style::default().fg(theme::WARNING),
        ));
        has_content = true;
    }

    // Agent 面板信息（仅面板激活时）
    if let Some(panel) = app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .get::<AgentPanel>()
    {
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
    let key_style = Style::default()
        .fg(theme::MUTED)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme::MUTED);

    let right_spans: Vec<Span> = match &app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .interaction_prompt
    {
        Some(_) if app.services.oauth_prompt.is_some() => format_hints(
            &[
                ("Ctrl+O", ":打开浏览器"),
                ("Enter", ":提交"),
                ("Esc", ":取消"),
            ],
            key_style,
            desc_style,
        ),
        Some(crate::app::InteractionPrompt::Questions(_)) => format_hints(
            &[
                ("Tab", ":切换"),
                ("↑↓", ":移动"),
                ("Space", ":选择"),
                ("Enter", ":确认"),
            ],
            key_style,
            desc_style,
        ),
        Some(crate::app::InteractionPrompt::Approval(_)) => format_hints(
            &[("↑↓", ":移动"), ("Space", ":切换"), ("Enter", ":确认")],
            key_style,
            desc_style,
        ),
        None => {
            let no_mouse = app.services.mouse_available == Some(false);
            let hints = if app.session_mgr.sessions[app.session_mgr.active]
                .session_panels
                .is_any_open()
            {
                app.session_mgr.sessions[app.session_mgr.active]
                    .session_panels
                    .status_bar_hints()
            } else if app.global_panels.is_any_open() {
                app.global_panels.status_bar_hints()
            } else if app.session_mgr.sessions.len() > 1 {
                if no_mouse {
                    vec![
                        ("/", "命令"),
                        ("Ctrl+N/P", ":切换Session"),
                        ("Ctrl+W", ":关闭"),
                        ("Ctrl+U/D", ":滚动"),
                    ]
                } else {
                    vec![
                        ("/", "命令"),
                        ("Ctrl+N/P", ":切换Session"),
                        ("Ctrl+W", ":关闭"),
                    ]
                }
            } else if app.services.quit_pending_since.is_some() {
                vec![("Ctrl+C", ":关闭"), ("其他键", ":取消")]
            } else if no_mouse {
                vec![("/", "命令"), ("Alt+Enter", ":换行"), ("Ctrl+U/D", ":滚动")]
            } else {
                vec![("/", "命令"), ("Alt+Enter", ":换行")]
            };
            format_hints(&hints, key_style, desc_style)
        }
    };

    render_truncated_line(f, left_spans, right_spans, area);
}

/// 将 (key, desc) 对列表格式化为 Span 列表
fn format_hints(
    hints: &[(&'static str, &'static str)],
    key_style: Style,
    desc_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = Vec::new();
    for (key, desc) in hints {
        spans.push(Span::styled(format!(" {} ", key), key_style));
        spans.push(Span::styled(format!(":{} ", desc), desc_style));
    }
    spans
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
