use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use perihelion_widgets::{BorderedPanel, ScrollState, ScrollableArea};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::ui::theme;
use crate::ui::main_ui::highlight_line_spans;

fn truncate_display(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return s.to_string();
    }
    let target = max_width.saturating_sub(1); // 留 1 列给 …
    let mut cum = 0;
    for (i, c) in s.char_indices() {
        let cw = c.width().unwrap_or(0);
        if cum + cw > target {
            return format!("{}…", &s[..i]);
        }
        cum += cw;
    }
    s.to_string()
}

/// Thread 浏览面板（底部展开区）
pub(crate) fn render_thread_browser(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(browser) = &app.core.thread_browser else { return };

    let popup_area = area;

    let inner = BorderedPanel::new(
        Span::styled(
            " 📝 选择对话 ",
            Style::default().fg(theme::MUTED).add_modifier(Modifier::BOLD),
        )
    )
        .border_style(Style::default().fg(theme::MUTED))
        .render(f, popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 工作目录 + 快捷键提示行
    lines.push(Line::from(vec![
        Span::styled(format!(" {} ", app.cwd), Style::default().fg(theme::DIM)),
        Span::styled(" ↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":移动 ", Style::default().fg(theme::MUTED)),
        Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":确认 ", Style::default().fg(theme::MUTED)),
        Span::styled("Ctrl+D", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":删除 ", Style::default().fg(theme::MUTED)),
        Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":关闭", Style::default().fg(theme::MUTED)),
    ]));

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
    let max_label = inner.width.saturating_sub(3) as usize;
    for (i, meta) in browser.threads.iter().enumerate() {
        let is_cursor = browser.cursor == i + 1;
        let title = meta.title.as_deref().unwrap_or("(无标题)");
        let label = truncate_display(title, max_label);

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

    // 存储面板元数据供鼠标选区使用
    app.core.panel_area = Some(inner);
    app.core.panel_scroll_offset = browser.scroll_offset;
    app.core.panel_plain_lines = lines.iter().map(|l| {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }).collect();

    // 应用面板选区高亮
    if app.core.panel_selection.is_active() {
        let sel = &app.core.panel_selection;
        if let (Some(start), Some(end)) = (sel.start, sel.end) {
            let ((sr, sc), (er, ec)) = if start <= end { (start, end) } else { (end, start) };
            let scroll = app.core.panel_scroll_offset as usize;
            let visible_start = scroll;
            let visible_end = scroll + inner.height as usize;
            for line_idx in sr as usize..=er as usize {
                if line_idx < visible_start || line_idx >= visible_end {
                    continue;
                }
                let visual_idx = line_idx - visible_start;
                if visual_idx >= lines.len() {
                    continue;
                }
                let (cs, ce) = if line_idx == sr as usize && line_idx == er as usize {
                    (sc as usize, ec as usize)
                } else if line_idx == sr as usize {
                    (sc as usize, usize::MAX)
                } else if line_idx == er as usize {
                    (0, ec as usize)
                } else {
                    (0, usize::MAX)
                };
                let spans = std::mem::take(&mut lines[visual_idx].spans);
                lines[visual_idx] = Line::from(highlight_line_spans(spans, cs, ce));
            }
        }
    }

    let mut scroll_state = ScrollState::with_offset(browser.scroll_offset);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, inner, &mut scroll_state);
}
