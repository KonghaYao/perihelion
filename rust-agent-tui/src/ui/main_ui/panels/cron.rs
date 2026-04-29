use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use perihelion_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::app::App;
use crate::ui::theme;
use crate::ui::main_ui::highlight_line_spans;

/// CronPanel 渲染
pub(crate) fn render_cron_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(panel) = &app.cron.cron_panel else { return };

    let title = " 定时任务 ";
    let inner = BorderedPanel::new(
        Span::styled(title, Style::default().fg(theme::MUTED).add_modifier(Modifier::BOLD)),
    )
        .border_style(Style::default().fg(theme::MUTED))
        .render(f, area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, task) in panel.tasks.iter().enumerate() {
        let is_cursor = i == panel.cursor;
        let cursor_char = if is_cursor { "▶ " } else { "  " };
        let status_icon = if task.enabled { "✓启用" } else { "✗禁用" };
        let next = task
            .next_fire
            .map(|t| {
                // Convert UTC to local time display
                let local: chrono::DateTime<chrono::Local> = t.into();
                local.format("%H:%M:%S").to_string()
            })
            .unwrap_or_else(|| "N/A".to_string());

        let prompt_truncated: String = task.prompt.chars().take(30).collect();
        let prompt_display = if task.prompt.len() > 30 {
            format!("{}…", prompt_truncated)
        } else {
            prompt_truncated
        };

        let style = if is_cursor {
            Style::default()
                .fg(ratatui::style::Color::White)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let status_style = if task.enabled {
            Style::default().fg(theme::ACCENT)
        } else {
            Style::default().fg(theme::MUTED)
        };

        lines.push(Line::from(vec![
            Span::styled(cursor_char.to_string(), Style::default().fg(theme::ACCENT)),
            Span::styled(format!("[{}] ", status_icon), status_style),
            Span::styled(format!("{} ", task.expression), style),
            Span::styled(format!("| {} | ", next), Style::default().fg(theme::MUTED)),
            Span::styled(prompt_display, style),
        ]));
    }

    // 底部提示行
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            " Enter",
            Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD),
        ),
        Span::styled(":切换  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "d",
            Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD),
        ),
        Span::styled(":删除  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc",
            Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD),
        ),
        Span::styled(":关闭", Style::default().fg(theme::MUTED)),
    ]));

    // 存储面板元数据供鼠标选区使用
    app.core.panel_area = Some(inner);
    app.core.panel_scroll_offset = panel.scroll_offset;
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

    let mut scroll_state = ScrollState::with_offset(panel.scroll_offset);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, inner, &mut scroll_state);
}
