use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{
        hooks_panel::{hook_type_label, hook_type_summary, HooksPanel},
        App,
    },
    ui::theme,
};

/// /hooks 面板渲染（底部展开区）
pub(crate) fn render_hooks_panel(f: &mut Frame, panel: &HooksPanel, app: &mut App, area: Rect) {
    let total_hooks = panel.total_hooks();
    let entry_count = panel.total();

    let title = if entry_count == 0 {
        " Hooks (none configured) "
    } else {
        " Hooks "
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let mut lines: Vec<Line> = Vec::new();

    // 统计行
    if entry_count > 0 {
        lines.push(Line::from(vec![Span::styled(
            format!("{} hooks configured", total_hooks),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )]));
    }

    // 只读提示
    lines.push(Line::from(vec![Span::styled(
        "This panel is read-only. To add or modify hooks, edit plugin hooks.json.",
        Style::default().fg(theme::MUTED),
    )]));
    lines.push(Line::from(""));

    // 事件列表
    if entry_count == 0 {
        lines.push(Line::from(vec![Span::styled(
            "  No hooks configured.",
            Style::default().fg(theme::MUTED),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "  Hooks can be added via plugin hooks/hooks.json.",
            Style::default().fg(theme::MUTED),
        )]));
    } else {
        for (i, entry) in panel.list.items().iter().enumerate() {
            let is_cursor = panel.cursor() == i;
            let cursor_char = if is_cursor { "❯" } else { " " };

            let name_style = if is_cursor {
                Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            // 行号 + 事件名 + hook 数量
            lines.push(Line::from(vec![
                Span::styled(format!("{} {}. ", cursor_char, i + 1), name_style),
                Span::styled(format!("{} ", entry.display_name), name_style),
                Span::styled(
                    format!("({})  ", entry.hook_count),
                    Style::default().fg(theme::ACCENT),
                ),
                Span::styled(
                    entry.description.as_str(),
                    Style::default().fg(theme::MUTED),
                ),
            ]));

            // 如果当前光标在此事件上，展开显示 hook 详情
            if is_cursor {
                for detail in &entry.hooks {
                    let type_label = hook_type_label(&detail.hook_type);
                    let summary = hook_type_summary(&detail.hook_type);
                    let once_marker = if detail.hook_type.is_once() {
                        " [once]"
                    } else {
                        ""
                    };

                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(
                            format!("[{}] ", type_label),
                            Style::default().fg(theme::ACCENT),
                        ),
                        Span::styled(summary, Style::default().fg(theme::TEXT)),
                        Span::styled(once_marker.to_string(), Style::default().fg(theme::WARNING)),
                    ]));

                    // matcher 行
                    if let Some(matcher) = &detail.matcher {
                        lines.push(Line::from(vec![
                            Span::raw("         "),
                            Span::styled(
                                format!("matcher: {}", matcher),
                                Style::default().fg(theme::MUTED),
                            ),
                        ]));
                    }

                    // plugin 来源行
                    lines.push(Line::from(vec![
                        Span::raw("         "),
                        Span::styled(
                            format!("plugin: {}", detail.plugin_name),
                            Style::default().fg(theme::DIM),
                        ),
                    ]));
                }
                lines.push(Line::from(""));
            }
        }
    }

    // 存储面板元数据供鼠标选区使用
    let scroll_offset = panel.scroll_offset();
    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = scroll_offset;
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, inner, &mut scroll_state);
}

#[cfg(test)]
mod tests {
    use crate::app::{hooks_panel::HooksPanel, App};
    use peri_middlewares::hooks::types::{HookEvent, HookType, RegisteredHook};
    use std::{collections::HashMap, path::PathBuf};
    include!("hooks_test.rs");
}
