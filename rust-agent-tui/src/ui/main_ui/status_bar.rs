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
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
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
            PermissionMode::Default           => ("DEFAULT",    theme::TEXT),
            PermissionMode::AcceptEdits       => ("AUTO-EDIT",  theme::SAGE),
            PermissionMode::Auto              => ("AUTO",       theme::LOADING),
            PermissionMode::BypassPermissions => ("YOLO",       theme::WARNING),
            PermissionMode::DontAsk           => ("NO-ASK",     theme::ERROR),
        };
        let is_highlight = app.mode_highlight_until
            .map_or(false, |until| std::time::Instant::now() < until);
        let mut style = Style::default().fg(color);
        if is_highlight {
            style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        }
        spans.push(Span::styled(format!(" {}", label), style));
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

    render_truncated_line(f, spans, Vec::new(), area);
}

/// 第二行：上下文使用率 │ [Agent 面板信息] │ [快捷键提示]
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    let mut left_spans: Vec<Span> = Vec::new();
    let mut has_content = false;

    // 上下文使用率
    {
        let tracker = &app.agent.session_token_tracker;
        if let Some(pct) = tracker.context_usage_percent(app.agent.context_window) {
            if has_content {
                left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
            }
            let used = tracker.estimated_context_tokens().unwrap_or(0);
            let total = app.agent.context_window;
            let color = if pct >= 85.0 {
                theme::ERROR
            } else if pct >= 70.0 {
                theme::WARNING
            } else {
                theme::SAGE
            };
            left_spans.push(Span::styled(
                format!("ctx: {:.0}% ({:.0}K/{:.0}K)", pct, used as f64 / 1000.0, total as f64 / 1000.0),
                Style::default().fg(color),
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
                format!(" 🤖 {}", agent.name),
                Style::default().fg(theme::MUTED),
            ));
        } else {
            left_spans.push(Span::styled(" 🤖 无", Style::default().fg(theme::MUTED)));
        }
    } else if let Some(id) = app.get_agent_id() {
        if has_content {
            left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        }
        left_spans.push(Span::styled(
            format!(" 🤖 {}", id),
            Style::default().fg(theme::MUTED),
        ));
    }

    // 右侧：弹窗快捷键提示（保持原有逻辑）
    let right_spans: Vec<Span> = match &app.agent.interaction_prompt {
        Some(crate::app::InteractionPrompt::Questions(_)) => {
            vec![
                Span::styled(" Tab", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":切换  ", Style::default().fg(theme::MUTED)),
                Span::styled("↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":移动  ", Style::default().fg(theme::MUTED)),
                Span::styled("Space", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":选择  ", Style::default().fg(theme::MUTED)),
                Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":确认", Style::default().fg(theme::MUTED)),
            ]
        }
        Some(crate::app::InteractionPrompt::Approval(_)) => {
            vec![
                Span::styled(" ↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":移动  ", Style::default().fg(theme::MUTED)),
                Span::styled("Space", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":切换  ", Style::default().fg(theme::MUTED)),
                Span::styled("y", Style::default().fg(theme::SAGE).add_modifier(Modifier::BOLD)),
                Span::styled(":全批准  ", Style::default().fg(theme::MUTED)),
                Span::styled("n", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                Span::styled(":全拒绝  ", Style::default().fg(theme::MUTED)),
                Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":确认", Style::default().fg(theme::MUTED)),
            ]
        }
        None => if app.core.agent_panel.is_some() {
            vec![
                Span::styled("↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":选择  ", Style::default().fg(theme::MUTED)),
                Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(":确认  ", Style::default().fg(theme::MUTED)),
                Span::styled("Esc", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                Span::styled(":取消", Style::default().fg(theme::MUTED)),
            ]
        } else {
            vec![]
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
