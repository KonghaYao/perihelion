use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{
        plugin_panel::{PluginPanel, PluginPanelView},
        App,
    },
    ui::theme,
};

use super::{discover_search::render_discover_search_box, truncate_display};

/// Tab 行占用的固定高度（Tab 行 + 空行）
pub(crate) const DISCOVER_TAB_OVERHEAD: u16 = 2;
/// 搜索框占用的固定高度（搜索框 3 行 + 空行 1 行）
pub(crate) const DISCOVER_SEARCH_OVERHEAD: u16 = 4;
/// Tab + 搜索框合计固定高度
pub(crate) const DISCOVER_FIXED_OVERHEAD: u16 = DISCOVER_TAB_OVERHEAD + DISCOVER_SEARCH_OVERHEAD; // 6

/// Discover 视图：Tab 行 -> 搜索框（固定） -> 可滚动插件列表（带跟随）
pub(crate) fn render_discover_list(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    // Tab 行 Spans
    let tab_labels: Vec<Span> = PluginPanelView::ALL
        .iter()
        .map(|v| {
            let label = v.label();
            let is_active = panel.view == *v;
            let style = if is_active {
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::THINKING)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::MUTED)
            };
            Span::styled(format!(" {} ", label), style)
        })
        .collect();

    let title_text = if panel.discover_loading {
        " Plugins \u{2026} "
    } else {
        " Plugins "
    };

    let inner = BorderedPanel::new(Span::styled(
        title_text,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let tab_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: DISCOVER_TAB_OVERHEAD,
    };
    let search_area = Rect {
        x: inner.x + 1,
        y: inner.y + DISCOVER_TAB_OVERHEAD,
        width: inner.width.saturating_sub(2),
        height: 3,
    };
    let list_area = Rect {
        x: inner.x,
        y: inner.y + DISCOVER_FIXED_OVERHEAD,
        width: inner.width,
        height: inner.height.saturating_sub(DISCOVER_FIXED_OVERHEAD),
    };

    let tab_para = Paragraph::new(vec![Line::from(tab_labels), Line::from("")]);
    f.render_widget(tab_para, tab_area);

    render_discover_search_box(f, panel, search_area);

    let mut lines: Vec<Line> = Vec::new();

    let filtered = panel.discover_filtered_plugins();
    let max_name_width = list_area.width.saturating_sub(8) as usize;

    if panel.discover_loading && filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Loading marketplace data\u{2026}",
            Style::default().fg(theme::MUTED),
        )));
    } else if filtered.is_empty() {
        let msg = if panel.discover_search.value().is_empty() {
            "  No plugins available"
        } else {
            "  No matching plugins"
        };
        lines.push(Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(theme::MUTED),
        )));
    } else {
        for (i, plugin) in filtered.iter().enumerate() {
            let is_cursor = i == panel.discover_list.cursor();
            let is_selected = panel.discover_selected.contains(&plugin.plugin_id);
            let is_installing = panel.installing.contains(&plugin.plugin_id);
            let is_uninstalling = panel.uninstalling.contains(&plugin.plugin_id);
            let cursor_char = if is_cursor { "\u{276F} " } else { "  " };
            let check_char = if is_selected { "\u{25C9}" } else { "\u{25CB}" };

            let name_style = if is_cursor {
                Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let display_name = truncate_display(&plugin.name, max_name_width);

            let mut spans = vec![
                Span::styled(
                    cursor_char.to_string(),
                    Style::default().fg(theme::THINKING),
                ),
                Span::styled(
                    format!("{} ", check_char),
                    if is_selected {
                        Style::default().fg(theme::ACCENT)
                    } else {
                        Style::default().fg(theme::MUTED)
                    },
                ),
                Span::styled(display_name.clone(), name_style),
            ];

            if !plugin.marketplace.is_empty() {
                spans.push(Span::styled(
                    format!(" \u{00B7} {}", plugin.marketplace),
                    Style::default().fg(theme::MUTED),
                ));
            }

            let mut right_parts: Vec<Span> = Vec::new();

            if let Some(count) = plugin.install_count {
                right_parts.push(Span::styled(
                    format!(
                        " {} {} installs",
                        peri_middlewares::plugin::format_install_count(count),
                        "\u{00B7}"
                    ),
                    Style::default().fg(theme::MUTED),
                ));
            }

            if is_installing {
                right_parts.push(Span::styled(
                    " installing\u{2026}",
                    Style::default().fg(theme::WARNING),
                ));
            } else if is_uninstalling {
                right_parts.push(Span::styled(
                    " uninstalling\u{2026}",
                    Style::default().fg(theme::WARNING),
                ));
            } else if plugin.installed {
                right_parts.push(Span::styled(" \u{2714}", Style::default().fg(theme::SAGE)));
            }

            if !right_parts.is_empty() {
                let content_width: usize = spans
                    .iter()
                    .map(|s| unicode_width::UnicodeWidthStr::width(&*s.content))
                    .sum();
                let right_width: usize = right_parts
                    .iter()
                    .map(|s| unicode_width::UnicodeWidthStr::width(&*s.content))
                    .sum();
                let available_width = list_area.width.saturating_sub(2) as usize;
                let padding = if content_width + right_width < available_width {
                    " ".repeat(available_width.saturating_sub(content_width + right_width))
                } else {
                    " ".repeat(2)
                };
                spans.push(Span::styled(padding, Style::default()));
                spans.extend(right_parts);
            }

            lines.push(Line::from(spans));

            let desc_width = list_area.width.saturating_sub(6) as usize;
            let desc = if plugin.description.is_empty() {
                String::new()
            } else {
                truncate_display(&plugin.description, desc_width)
            };
            if !desc.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(desc, Style::default().fg(theme::MUTED)),
                ]));
            } else {
                lines.push(Line::from(""));
            }
        }
    }

    let cursor_row = (panel.discover_list.cursor() * 2) as u16;
    let visible_height = list_area.height;
    let mut scroll_state = ScrollState::with_offset(panel.discover_list.scroll_offset());
    scroll_state.ensure_visible(cursor_row, visible_height);

    if let Some(p) = app.global_panels.get_mut::<PluginPanel>() {
        p.discover_list.set_scroll_offset(scroll_state.offset());
    }

    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = 0;
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, list_area, &mut scroll_state);
}
