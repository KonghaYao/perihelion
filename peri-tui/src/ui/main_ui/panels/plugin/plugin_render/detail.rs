use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{
        plugin_panel::{DetailAction, PluginPanel},
        App,
    },
    ui::theme,
};

use peri_middlewares::plugin::InstallScope;

use super::detail_kv_line;

pub(crate) fn render_detail(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    let (lines, scroll_offset) = {
        let entry_idx = match panel.detail_index {
            Some(i) => i,
            None => return,
        };
        let entry = match panel.entries.get(entry_idx) {
            Some(e) => e,
            None => return,
        };
        let scroll_offset = panel.scroll_offset();
        let detail_cursor = panel.detail_cursor;
        let mut lines: Vec<Line> = Vec::new();

        // Header: name @ marketplace
        let header_text = if entry.marketplace.is_empty() {
            entry.name.clone()
        } else {
            format!("{} @ {}", entry.name, entry.marketplace)
        };
        lines.push(Line::from(Span::styled(
            format!("  {}", header_text),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )));

        // Scope
        let scope_label = match entry.scope {
            InstallScope::Project => "project",
            InstallScope::Local => "local",
            InstallScope::User => "user",
        };
        lines.push(detail_kv_line("Scope:", scope_label));
        lines.push(detail_kv_line("Version:", &entry.version));

        // Description
        if !entry.description.is_empty() {
            lines.push(Line::from(""));
            for desc_line in entry.description.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", desc_line),
                    Style::default().fg(theme::MUTED),
                )));
            }
        }

        // Author
        if let Some(ref author) = entry.author {
            lines.push(Line::from(""));
            lines.push(detail_kv_line("Author:", author));
        }

        // Status
        lines.push(Line::from(""));
        let is_uninstalling = panel.uninstalling.contains(&entry.id);
        let (status_icon, status_style, status_text) = if is_uninstalling {
            (
                "\u{26A0}",
                Style::default().fg(theme::WARNING),
                "Uninstalling\u{2026}",
            )
        } else if entry.enabled {
            ("\u{2714}", Style::default().fg(theme::SAGE), "Enabled")
        } else {
            ("\u{25CB}", Style::default().fg(theme::MUTED), "Disabled")
        };
        lines.push(Line::from(vec![
            Span::styled("  Status: ".to_string(), Style::default().fg(theme::MUTED)),
            Span::styled(format!("{} {}", status_icon, status_text), status_style),
        ]));

        // Installed components
        let has_components = !entry.commands.is_empty()
            || !entry.skills.is_empty()
            || !entry.agents.is_empty()
            || !entry.mcp_servers.is_empty();

        if has_components {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Installed components:".to_string(),
                Style::default().fg(theme::MUTED),
            )));

            if !entry.commands.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  \u{2022} Commands: {}", entry.commands.join(", ")),
                    Style::default().fg(theme::TEXT),
                )));
            }
            if !entry.skills.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  \u{2022} Skills: {}", entry.skills.join(", ")),
                    Style::default().fg(theme::TEXT),
                )));
            }
            if !entry.agents.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  \u{2022} Agents: {}", entry.agents.join(", ")),
                    Style::default().fg(theme::TEXT),
                )));
            }
            if !entry.mcp_servers.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  \u{2022} MCP servers: {}", entry.mcp_servers.join(", ")),
                    Style::default().fg(theme::TEXT),
                )));
            }
        }

        // Action menu
        lines.push(Line::from(""));
        lines.push(Line::from(""));

        for (i, action) in DetailAction::ALL.iter().enumerate() {
            let is_cursor = i == detail_cursor;
            let cursor_char = if is_cursor { "\u{276F} " } else { "  " };
            let label = action.label(entry.enabled);
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
