use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::ui::theme;

/// Thread 浏览面板（底部展开区）
pub(crate) fn render_thread_browser(f: &mut Frame, app: &App, area: Rect) {
    let Some(browser) = &app.thread_browser else { return };

    let popup_area = area;
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " 📝 选择对话  ↑↓:移动  Enter:确认  d:删除  Esc:关闭",
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 第 0 项：新建对话
    let is_new_cursor = browser.cursor == 0;
    lines.push(Line::from(vec![
        Span::styled(
            if is_new_cursor { "▶ " } else { "  " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            "+ 新建对话",
            if is_new_cursor {
                Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::SAGE)
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
                Style::default().fg(theme::ACCENT),
            ),
            Span::styled(
                label,
                if is_cursor {
                    Style::default().fg(Color::White).bg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::TEXT)
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
