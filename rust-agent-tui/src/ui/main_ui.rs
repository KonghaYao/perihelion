mod panels;
mod popups;
mod status_bar;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::app::App;
use crate::ui::theme;
use crate::ui::welcome;
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
    let attachment_height: u16 = if app.pending_attachments.is_empty() {
        0
    } else {
        3
    };

    // 底部展开区高度（替代居中弹窗）
    let panel_height = active_panel_height(app, area.height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),                    // [0] 聊天区
            Constraint::Length(todo_height),       // [1] TODO 面板（动态）
            Constraint::Length(attachment_height), // [2] 附件栏（动态）
            Constraint::Length(panel_height),      // [3] 底部展开区（动态）
            Constraint::Length(input_height),      // [4] 输入框（动态）
            Constraint::Length(1),                 // [5] 状态栏
        ])
        .split(area);

    render_messages(f, app, chunks[0]);
    render_todo_panel(f, app, chunks[1]);
    render_attachment_bar(f, app, chunks[2]);

    // 底部展开区（HITL / AskUser / 配置面板）
    if panel_height > 0 {
        let panel_area = chunks[3];
        match &app.interaction_prompt {
            Some(crate::app::InteractionPrompt::Approval(_)) => {
                popups::hitl::render_hitl_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Questions(_)) => {
                popups::ask_user::render_ask_user_popup(f, app, panel_area);
            }
            None => {}
        }
        if app.model_panel.is_some() {
            panels::model::render_model_panel(f, app, panel_area);
        }
        if app.agent_panel.is_some() {
            panels::agent::render_agent_panel(f, app, panel_area);
        }
        if app.relay_panel.is_some() {
            panels::relay::render_relay_panel(f, app, panel_area);
        }
        if app.thread_browser.is_some() {
            panels::thread_browser::render_thread_browser(f, app, panel_area);
        }
    }

    f.render_widget(&app.textarea, chunks[4]);
    status_bar::render_status_bar(f, app, chunks[5]);

    // 命令/Skills 提示条（浮动在输入框上方）
    popups::hints::render_command_hint(f, app, chunks[4]);
    popups::hints::render_skill_hint(f, app, chunks[4]);
}

/// 计算底部展开区所需高度（无激活面板时返回 0）
fn active_panel_height(app: &App, screen_height: u16) -> u16 {
    let max_h = screen_height * 3 / 5; // 最多占 60% 屏高
    let raw = if let Some(panel) = &app.thread_browser {
        (panel.total() as u16 + 4).max(6)
    } else if app.model_panel.is_some() {
        14
    } else if let Some(panel) = &app.agent_panel {
        (panel.agents.len() as u16 * 2 + 6).max(6)
    } else if app.relay_panel.is_some() {
        10
    } else if let Some(crate::app::InteractionPrompt::Approval(p)) = &app.interaction_prompt {
        (p.items.len() as u16 * 2 + 5).max(5)
    } else if let Some(crate::app::InteractionPrompt::Questions(p)) = &app.interaction_prompt {
        let cur = &p.questions[p.active_tab];
        let opt_rows = cur.data.options.len() as u16;
        let desc_rows = cur
            .data
            .options
            .iter()
            .filter(|o| o.description.is_some())
            .count() as u16;
        (cur.data.question.lines().count() as u16 + opt_rows + desc_rows + 7).max(8)
    } else {
        0
    };
    raw.min(max_h)
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    // Welcome Card：空消息时显示品牌欢迎界面
    if app.view_messages.is_empty() {
        welcome::render_welcome(f, app, area);
        return;
    }

    let inner = area;
    let visible_height = inner.height;

    // 计算 loading spinner 帧（基于当前时间，200ms 切换一帧）
    let spinner_line: Option<Line<'static>> = if app.loading {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠸", "⠴", "⠦", "⠇"];
        let frame_idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 200) as usize
            % FRAMES.len();
        Some(Line::from(ratatui::text::Span::styled(
            format!(" {} 思考中…", FRAMES[frame_idx]),
            ratatui::style::Style::default()
                .fg(theme::LOADING)
                .add_modifier(Modifier::BOLD),
        )))
    } else {
        None
    };

    // 从 RenderCache 读取已渲染好的行（浅克隆 Vec 头，开销极小）
    let (mut all_lines, total_lines, max_scroll, offset) = {
        let cache = app.render_cache.read();
        app.last_render_version = cache.version;

        // total_lines 已是 wrap 后的真实视觉行数（由渲染线程通过 Paragraph::line_count 计算）
        let total_lines = cache.total_lines;
        let spinner_extra = if spinner_line.is_some() { 1u16 } else { 0 };
        let visual_total = (total_lines as u16).saturating_add(spinner_extra);
        let max_scroll = visual_total.saturating_sub(visible_height);
        let offset = if app.scroll_follow {
            max_scroll
        } else {
            app.scroll_offset.min(max_scroll)
        };

        // Vec::clone() 是浅克隆，只复制指针+容量+长度头（3个 usize），不复制 Line 内容
        (cache.lines.clone(), total_lines, max_scroll, offset)
    };
    if let Some(line) = spinner_line {
        all_lines.push(line);
    }
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
    let visual_total = total_lines as u16;
    if visual_total > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(offset as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(theme::MUTED));
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
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
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
        Line::from(Span::styled(tags, Style::default().fg(theme::TEXT))),
        Line::from(Span::styled(
            "Del: 删除最后一张",
            Style::default().fg(theme::MUTED),
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
        theme::WARNING
    } else {
        theme::ACCENT
    };

    let block = Block::default()
        .title(Span::styled(
            " 📋 TODO ",
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
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
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                TodoStatus::Completed => ("✓", Style::default().fg(theme::MUTED)),
                TodoStatus::Pending => ("○", Style::default().fg(theme::TEXT)),
            };
            Line::from(vec![
                Span::styled(format!(" {} ", icon), style),
                Span::styled(item.content.clone(), style),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
