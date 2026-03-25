mod popups;
mod panels;
mod status_bar;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};

use crate::app::App;
use rust_agent_middlewares::prelude::TodoStatus;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // 动态输入框高度：行数 + 边框（上下各 1），最少 3 行，最多 40%
    let line_count = app.textarea.lines().len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);

    // TODO 面板高度：无内容时为 0，有内容时为条目数 + 边框(2)，上限 10
    let todo_height = if app.todo_items.is_empty() {
        0
    } else {
        (app.todo_items.len() as u16 + 2).min(10)
    };

    // 附件栏高度：无附件时为 0，有附件时固定 3 行
    let attachment_height: u16 = if app.pending_attachments.is_empty() { 0 } else { 3 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                  // [0] 标题栏
            Constraint::Min(3),                     // [1] 聊天区
            Constraint::Length(todo_height),        // [2] TODO 面板（动态）
            Constraint::Length(attachment_height),  // [3] 附件栏（动态）
            Constraint::Length(input_height),       // [4] 输入框（动态）
            Constraint::Length(1),                  // [5] 帮助栏
        ])
        .split(area);

    render_title(f, app, chunks[0]);
    render_messages(f, app, chunks[1]);
    render_todo_panel(f, app, chunks[2]);
    render_attachment_bar(f, app, chunks[3]);
    f.render_widget(&app.textarea, chunks[4]);
    status_bar::render_status_bar(f, app, chunks[5]);

    // 命令/Skills 提示条（浮动在输入框上方）
    popups::hints::render_command_hint(f, app, chunks[4]);
    popups::hints::render_skill_hint(f, app, chunks[4]);

    // HITL 弹窗（覆盖层）
    if app.hitl_prompt.is_some() {
        popups::hitl::render_hitl_popup(f, app);
    }

    // AskUser 弹窗（覆盖层）
    if app.ask_user_prompt.is_some() {
        popups::ask_user::render_ask_user_popup(f, app);
    }

    // /model 面板（覆盖层）
    if app.model_panel.is_some() {
        panels::model::render_model_panel(f, app);
    }

    // /agents 面板（覆盖层）
    if app.agent_panel.is_some() {
        panels::agent::render_agent_panel(f, app);
    }

    // Thread 浏览面板（覆盖层，最高优先级）
    if app.thread_browser.is_some() {
        panels::thread_browser::render_thread_browser(f, app);
    }
}

fn render_title(f: &mut Frame, app: &App, area: Rect) {
    let model_info = app.zen_config.as_ref().map(|c| {
        let alias = &c.config.active_alias;
        let mapping = match alias.as_str() {
            "opus"   => &c.config.model_aliases.opus,
            "sonnet" => &c.config.model_aliases.sonnet,
            "haiku"  => &c.config.model_aliases.haiku,
            _        => &c.config.model_aliases.opus,
        };
        let model_part = if mapping.model_id.is_empty() { app.model_name.as_str() } else { mapping.model_id.as_str() };
        format!("{}:{}", alias, model_part)
    }).unwrap_or_else(|| format!("{} {}", app.provider_name, app.model_name));
    let subtitle = format!("  —  {} | FilesystemMiddleware + TerminalMiddleware + SubAgentMiddleware", model_info);
    let title = Paragraph::new(
        Line::from(vec![
            Span::styled(" 🦀 ", Style::default().fg(Color::Red)),
            Span::styled("Rust Agent TUI", Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)),
            Span::styled(subtitle, Style::default().fg(Color::DarkGray)),
        ])
    )
    .style(Style::default().bg(Color::Black));
    f.render_widget(title, area);
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    let inner = area;
    let visible_height = inner.height;

    // 从 RenderCache 读取已渲染好的行（读锁，持锁时间极短）
    let (all_lines, total_lines, cache_version) = {
        let cache = app.render_cache.read();
        (cache.lines.clone(), cache.total_lines, cache.version)
    };
    // 更新 UI 线程记录的版本
    app.last_render_version = cache_version;

    let visual_total = total_lines as u16;
    let max_scroll = visual_total.saturating_sub(visible_height);

    // 计算本帧实际偏移，并写回 scroll_offset 保持同步
    let offset = if app.scroll_follow {
        max_scroll
    } else {
        app.scroll_offset.min(max_scroll)
    };
    app.scroll_offset = offset;

    // 文字区域（留出右侧 1 列给滚动条）
    let text_area = Rect {
        width: inner.width.saturating_sub(1),
        ..inner
    };
    let paragraph = Paragraph::new(Text::from(all_lines))
        .scroll((offset, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, text_area);

    // 滚动条
    if visual_total > visible_height {
        let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
            .position(offset as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::DarkGray));
        f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

/// 待发送附件栏（有附件时显示在输入框上方）
fn render_attachment_bar(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }

    let block = Block::default()
        .title(Span::styled(
            " 待发送附件 ",
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    f.render_widget(&block, area);

    let inner = block.inner(area);

    // 第 1 行：所有附件标签
    let tags: String = app
        .pending_attachments
        .iter()
        .map(|att| {
            let size_kb = (att.size_bytes / 1024).max(1);
            format!("[img {} {}KB]", att.label, size_kb)
        })
        .collect::<Vec<_>>()
        .join("  ");

    let lines = vec![
        Line::from(Span::styled(tags, Style::default().fg(Color::White))),
        Line::from(Span::styled(
            "Del: 删除最后一张",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// TODO 状态面板（固定在输入框上方）
fn render_todo_panel(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }

    let border_color = if app.loading {
        Color::Yellow
    } else {
        Color::Cyan
    };

    let block = Block::default()
        .title(Span::styled(
            " 📋 TODO ",
            Style::default().fg(border_color).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    f.render_widget(&block, area);

    let inner = block.inner(area);
    let max_display = inner.height as usize;

    let lines: Vec<Line> = app
        .todo_items
        .iter()
        .take(max_display)
        .map(|item| {
            let (icon, style) = match item.status {
                TodoStatus::InProgress => (
                    "→",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                TodoStatus::Completed => ("✓", Style::default().fg(Color::DarkGray)),
                TodoStatus::Pending => ("○", Style::default().fg(Color::White)),
            };
            Line::from(vec![
                Span::styled(format!(" {} ", icon), style),
                Span::styled(item.content.clone(), style),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
