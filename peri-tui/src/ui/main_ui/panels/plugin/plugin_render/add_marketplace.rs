use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::{
    app::{plugin_panel::PluginPanel, App},
    ui::theme,
};

/// 渲染 Add Marketplace 面板
pub(crate) fn render_add_marketplace(
    f: &mut Frame,
    panel: &PluginPanel,
    app: &mut App,
    area: Rect,
) {
    let input_value = panel.add_marketplace_input.value();
    let display_text = panel.add_marketplace_input.display_text('\u{2022}');

    let inner = BorderedPanel::new(Span::styled(
        " Add Marketplace ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);

    let mut lines = Vec::new();

    lines.push(Line::from(""));

    lines.push(Line::from(vec![Span::styled(
        "  Enter marketplace source:",
        Style::default().fg(theme::TEXT),
    )]));

    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled("Examples:", Style::default().fg(theme::MUTED)),
    ]));

    let examples = [
        ("owner/repo", "GitHub"),
        ("git@github.com:owner/repo.git", "SSH"),
        ("https://example.com/marketplace.json", ""),
        ("./path/to/marketplace", ""),
    ];

    for (example, desc) in &examples {
        if desc.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("   \u{00B7} ", Style::default().fg(theme::MUTED)),
                Span::styled(*example, Style::default().fg(theme::MUTED)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("   \u{00B7} ", Style::default().fg(theme::MUTED)),
                Span::styled(*example, Style::default().fg(theme::MUTED)),
                Span::styled(format!(" ({})", desc), Style::default().fg(theme::MUTED)),
            ]));
        }
    }

    lines.push(Line::from(""));

    let input_line = if input_value.is_empty() {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("\u{2588}", Style::default().fg(theme::TEXT)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(display_text, Style::default().fg(theme::TEXT)),
            Span::styled("\u{2588}", Style::default().fg(theme::TEXT)),
        ])
    };
    lines.push(input_line);

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("   ", Style::default()),
        Span::styled(
            "Enter to add",
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled(" \u{00B7} ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc to cancel",
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::ITALIC),
        ),
    ]));

    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}
