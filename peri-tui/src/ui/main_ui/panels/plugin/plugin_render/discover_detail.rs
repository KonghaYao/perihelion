use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{
        plugin_panel::{DiscoverDetailAction, PluginPanel},
        App,
    },
    ui::theme,
};

use super::detail_kv_line;

pub(crate) fn render_discover_detail(
    f: &mut Frame,
    panel: &PluginPanel,
    app: &mut App,
    area: Rect,
) {
    let (lines, scroll_offset) = {
        let plugin_idx = match panel.discover_detail_index {
            Some(i) => i,
            None => return,
        };
        let filtered = panel.discover_filtered_plugins();
        let plugin = match filtered.get(plugin_idx) {
            Some(p) => p,
            None => return,
        };
        let scroll_offset = panel.discover_list.scroll_offset();
        let detail_cursor = panel.discover_detail_cursor;
        let mut lines: Vec<Line> = Vec::new();

        // Header
        let header_text = if plugin.marketplace.is_empty() {
            plugin.name.clone()
        } else {
            format!("{} @ {}", plugin.name, plugin.marketplace)
        };
        lines.push(Line::from(Span::styled(
            format!("  {}", header_text),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )));

        // Version
        lines.push(detail_kv_line("Version:", &plugin.version));

        // Description
        if !plugin.description.is_empty() {
            lines.push(Line::from(""));
            for desc_line in plugin.description.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", desc_line),
                    Style::default().fg(theme::MUTED),
                )));
            }
        }

        // Author
        if let Some(ref author) = plugin.author {
            lines.push(Line::from(""));
            lines.push(detail_kv_line("Author:", author));
        }

        // Status
        lines.push(Line::from(""));
        let (status_icon, status_style, status_text) = if plugin.installed {
            ("\u{2714}", Style::default().fg(theme::SAGE), "Installed")
        } else {
            (
                "\u{25CB}",
                Style::default().fg(theme::MUTED),
                "Not installed",
            )
        };
        lines.push(Line::from(vec![
            Span::styled("  Status: ".to_string(), Style::default().fg(theme::MUTED)),
            Span::styled(format!("{} {}", status_icon, status_text), status_style),
        ]));

        // Action menu
        lines.push(Line::from(""));
        lines.push(Line::from(""));

        let actions = if plugin.installed {
            &[DiscoverDetailAction::BackToList] as &[DiscoverDetailAction]
        } else {
            &DiscoverDetailAction::ALL
        };

        for (i, action) in actions.iter().enumerate() {
            let is_cursor = i == detail_cursor;
            let cursor_char = if is_cursor { "\u{276F} " } else { "  " };
            let label = action.label();
            let style = if is_cursor {
                Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    cursor_char.to_string(),
                    Style::default().fg(theme::THINKING),
                ),
                Span::styled(label.to_string(), style),
            ]));
        }

        (lines, scroll_offset)
    };

    let inner = BorderedPanel::new(Span::styled(
        " Plugins ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = 0;
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
