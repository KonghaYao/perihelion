use crate::{
    app::{memory_panel::MemoryPanel, App},
    ui::theme,
};
use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

pub(crate) fn render_memory_panel(f: &mut Frame, panel: &MemoryPanel, app: &mut App, area: Rect) {
    let title = " Memory 文件 ";
    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let mut lines: Vec<Line> = Vec::new();

    for (i, entry) in panel.entries.iter().enumerate() {
        let is_cursor = i == panel.cursor();
        let cursor_char = if is_cursor { "❯ " } else { "  " };

        let style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let exist_icon = if entry.exists {
            ("✓", Style::default().fg(theme::SAGE))
        } else {
            ("✗", Style::default().fg(theme::MUTED))
        };

        let path_str = entry.path.to_string_lossy();
        let path_display = if path_str.len() > 40 {
            format!("...{}", &path_str[path_str.len() - 37..])
        } else {
            path_str.to_string()
        };

        lines.push(Line::from(vec![
            Span::styled(
                cursor_char.to_string(),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(format!("[{}] ", exist_icon.0), exist_icon.1),
            Span::styled(format!("{:<8} ", entry.label), style),
            Span::styled(path_display, Style::default().fg(theme::MUTED)),
        ]));

        // 文件不存在时显示创建提示
        if !entry.exists && is_cursor {
            lines.push(Line::from(Span::styled(
                "    按 Enter 创建并编辑",
                Style::default().fg(theme::MUTED),
            )));
        }
    }

    // 存储面板元数据
    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = panel.scroll_offset();
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();

    let mut scroll_state = ScrollState::with_offset(panel.scroll_offset());
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, inner, &mut scroll_state);
}
