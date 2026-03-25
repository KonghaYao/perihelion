use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;

/// Thread 浏览面板
pub(crate) fn render_thread_browser(f: &mut Frame, app: &App) {
    let Some(browser) = &app.thread_browser else { return };

    let area = f.area();
    let popup_width = (area.width * 3 / 4).max(50).min(area.width.saturating_sub(4));
    let popup_height = (browser.total() as u16 + 4).min(area.height * 4 / 5).min(area.height.saturating_sub(4)).max(6);
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " 📝 选择对话  ↑↓:移动  Enter:确认  d:删除  Esc:关闭",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 第 0 项：新建对话
    let is_new_cursor = browser.cursor == 0;
    lines.push(Line::from(vec![
        Span::styled(
            if is_new_cursor { "▶ " } else { "  " },
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            "+ 新建对话",
            if is_new_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
    ]));

    // 历史 thread
    for (i, meta) in browser.threads.iter().enumerate() {
        let is_cursor = browser.cursor == i + 1;
        let title = meta.title.as_deref().unwrap_or("(无标题)");
        let updated = meta.updated_at.format("%m-%d %H:%M").to_string();
        let cwd_short: String = meta.cwd.chars().rev().take(20).collect::<String>().chars().rev().collect();
        let label = format!("{title}  [{updated}] …{cwd_short}");

        lines.push(Line::from(vec![
            Span::styled(
                if is_cursor { "▶ " } else { "  " },
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                label,
                if is_cursor {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((browser.scroll_offset, 0)),
        inner,
    );
}
