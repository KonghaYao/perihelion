use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{
        tasks_panel::{TasksPanel, TasksTab},
        App,
    },
    ui::{main_ui::highlight_line_spans, theme},
};

/// TasksPanel 渲染
pub(crate) fn render_tasks_panel(f: &mut Frame, panel: &mut TasksPanel, app: &mut App, area: Rect) {
    let tab_label = match panel.tab {
        TasksTab::AgentThreads => " Tasks \u{2502} Agent Threads \u{2502} Cron Tasks ",
        TasksTab::CronTasks => " Tasks \u{2502} Agent Threads \u{2502} Cron Tasks ",
    };
    let title = Span::styled(
        tab_label,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    );
    let inner = BorderedPanel::new(title)
        .border_style(Style::default().fg(theme::BORDER))
        .render(f, area);

    let mut lines: Vec<Line> = Vec::new();

    // Tab indicator line
    {
        let agent_label = " Agent Threads ";
        let sep = "\u{2502}";
        let cron_label = " Cron Tasks ";
        let agent_style = if panel.tab == TasksTab::AgentThreads {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::MUTED)
        };
        let cron_style = if panel.tab == TasksTab::CronTasks {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::MUTED)
        };
        let sep_style = Style::default().fg(theme::BORDER);
        lines.push(Line::from(vec![
            Span::styled(agent_label, agent_style),
            Span::styled(format!(" {} ", sep), sep_style),
            Span::styled(cron_label, cron_style),
        ]));
    }

    // Detail view for agent thread
    if let Some(_thread_id) = &panel.detail_thread_id {
        render_agent_detail(panel, &mut lines);
    } else {
        match panel.tab {
            TasksTab::AgentThreads => render_agent_threads(panel, &mut lines),
            TasksTab::CronTasks => render_cron_tasks(panel, &mut lines),
        }
    }

    // Store panel metadata for mouse selection
    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = panel.active_scroll_offset();
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    // Apply selection highlighting
    if app.session_mgr.current_mut().ui.panel_selection.is_active() {
        let sel = &app.session_mgr.current_mut().ui.panel_selection;
        if let (Some(start), Some(end)) = (sel.start, sel.end) {
            let ((sr, sc), (er, ec)) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let scroll = app.session_mgr.current_mut().ui.panel_scroll_offset as usize;
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

    let scroll_offset = panel.active_scroll_offset();
    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, inner, &mut scroll_state);
}

fn render_agent_threads(panel: &mut TasksPanel, lines: &mut Vec<Line>) {
    let items = panel.agent_list.items();
    for (i, entry) in items.iter().enumerate() {
        let is_cursor = i == panel.agent_list.cursor();
        let cursor_char = if is_cursor { "\u{276f} " } else { "  " };
        let status_icon = if entry.is_active {
            "\u{25cf}" // ●
        } else {
            "\u{25cb}" // ○
        };

        let style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let status_style = if entry.is_active {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::MUTED)
        };

        let title_display = if entry.title.is_empty() {
            "(untitled)".to_string()
        } else {
            let truncated: String = entry.title.chars().take(30).collect();
            if entry.title.chars().count() > 30 {
                format!("{}\u{2026}", truncated)
            } else {
                truncated
            }
        };

        let id_short = if entry.thread_id.len() > 12 {
            format!("{}...", &entry.thread_id[..12])
        } else {
            entry.thread_id.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(
                cursor_char.to_string(),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(format!("{} ", status_icon), status_style),
            Span::styled(format!("{} ", title_display), style),
            Span::styled(format!("[{}] ", entry.status), status_style),
            Span::styled(
                format!("{}msg ", entry.message_count),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(id_short, Style::default().fg(theme::MUTED)),
        ]));
    }

    if items.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  (No agent threads)",
            Style::default().fg(theme::MUTED),
        )));
    }
}

fn render_agent_detail(panel: &mut TasksPanel, lines: &mut Vec<Line>) {
    if let Some(entry) = panel
        .detail_thread_id
        .as_ref()
        .and_then(|id| panel.agent_list.items().iter().find(|e| e.thread_id == *id))
    {
        let status_style = if entry.is_active {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled("Thread: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                entry.title.clone(),
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Status:  ", Style::default().fg(theme::MUTED)),
            Span::styled(format!("[{}]", entry.status), status_style),
        ]));
        lines.push(Line::from(vec![
            Span::styled("ID:      ", Style::default().fg(theme::MUTED)),
            Span::styled(entry.thread_id.clone(), Style::default().fg(theme::TEXT)),
        ]));
        lines.push(Line::from(""));
    }

    if panel.detail_messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (Loading messages...)",
            Style::default().fg(theme::MUTED),
        )));
    } else {
        for msg in &panel.detail_messages {
            let truncated: String = msg.chars().take(200).collect();
            let display = if msg.chars().count() > 200 {
                format!("{}\u{2026}", truncated)
            } else {
                truncated
            };
            lines.push(Line::from(Span::styled(
                display,
                Style::default().fg(theme::TEXT),
            )));
        }
    }
}

fn render_cron_tasks(panel: &mut TasksPanel, lines: &mut Vec<Line>) {
    let items = panel.cron_list.items();
    for (i, task) in items.iter().enumerate() {
        let is_cursor = i == panel.cron_list.cursor();
        let cursor_char = if is_cursor { "\u{276f} " } else { "  " };
        let status_icon = if task.enabled {
            "\u{2713}\u{542f}\u{7528}" // ✓启用
        } else {
            "\u{2717}\u{7981}\u{7528}" // ✗禁用
        };
        let next = task
            .next_fire
            .map(|t| {
                let local: chrono::DateTime<chrono::Local> = t.into();
                local.format("%H:%M:%S").to_string()
            })
            .unwrap_or_else(|| "N/A".to_string());

        let prompt_truncated: String = task.prompt.chars().take(30).collect();
        let prompt_display = if task.prompt.chars().count() > 30 {
            format!("{}\u{2026}", prompt_truncated)
        } else {
            prompt_truncated
        };

        let style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let status_style = if task.enabled {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::MUTED)
        };

        lines.push(Line::from(vec![
            Span::styled(
                cursor_char.to_string(),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(format!("[{}] ", status_icon), status_style),
            Span::styled(format!("{} ", task.expression), style),
            Span::styled(format!("| {} | ", next), Style::default().fg(theme::MUTED)),
            Span::styled(prompt_display, style),
        ]));
    }

    if items.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  （无定时任务，使用 /loop 命令创建）",
            Style::default().fg(theme::MUTED),
        )));
    }
}

// Helper: get active scroll offset
trait TasksPanelScroll {
    fn active_scroll_offset(&self) -> u16;
}
impl TasksPanelScroll for TasksPanel {
    fn active_scroll_offset(&self) -> u16 {
        match self.tab {
            TasksTab::AgentThreads => self.agent_list.scroll_offset(),
            TasksTab::CronTasks => self.cron_list.scroll_offset(),
        }
    }
}
