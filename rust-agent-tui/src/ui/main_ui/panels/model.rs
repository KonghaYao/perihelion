use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::model_panel::{AliasTab, ROW_EFFORT, ROW_HAIKU, ROW_OPUS, ROW_SONNET};
use crate::app::App;
use crate::ui::theme;

pub(crate) fn render_model_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(panel) = &app.core.model_panel else {
        return;
    };

    let inner = BorderedPanel::new(Span::styled(
        " Select model ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let active_alias = app
        .zen_config
        .as_ref()
        .map(|c| c.config.active_alias.as_str())
        .unwrap_or("opus");

    let models = app
        .zen_config
        .as_ref()
        .and_then(|c| {
            c.config
                .providers
                .iter()
                .find(|p| p.id == c.config.active_provider_id)
        })
        .map(|p| &p.models);

    let mut lines: Vec<Line> = Vec::new();

    // Description
    lines.push(Line::from(Span::styled(
        "  Switch between models. Applies to this session.",
        Style::default().fg(theme::MUTED),
    )));
    lines.push(Line::from(""));

    // Model rows: Opus / Sonnet / Haiku
    let rows: [(usize, &AliasTab, &str, &str); 3] = [
        (ROW_OPUS, &AliasTab::Opus, "Opus", "1"),
        (ROW_SONNET, &AliasTab::Sonnet, "Sonnet", "2"),
        (ROW_HAIKU, &AliasTab::Haiku, "Haiku", "3"),
    ];

    for (row_idx, alias, label, num) in &rows {
        let is_active = alias.to_key() == active_alias;
        let is_cursor = panel.cursor == *row_idx;
        let model_name = models
            .and_then(|m| m.get_model(alias.to_key()))
            .unwrap_or("");

        let check = if is_active { "\u{2714}" } else { " " };
        let cursor_char = if is_cursor { "\u{276f}" } else { " " };

        let label_style = if is_active {
            Style::default()
                .fg(theme::SAGE)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        };

        let model_style = Style::default().fg(theme::MUTED);

        let check_style = if is_active {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::MUTED)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", cursor_char),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(format!("{}. ", num), label_style),
            Span::styled(format!("{:8}", label), label_style),
            Span::styled(format!(" {}  ", check), check_style),
            Span::styled(model_name.to_string(), model_style),
        ]));
    }

    lines.push(Line::from(""));

    // Effort row
    {
        let is_cursor = panel.cursor == ROW_EFFORT;
        let effort_label = match panel.buf_thinking_effort.as_str() {
            "low" => "Low",
            "high" => "High",
            _ => "Medium",
        };

        let radio_color = theme::ACCENT;
        let effort_style = Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD);

        let mut spans = vec![
            Span::styled(
                "    \u{25cf} ",
                Style::default().fg(radio_color),
            ),
            Span::styled(format!("{} effort", effort_label), effort_style),
        ];

        if is_cursor {
            spans.push(Span::styled(
                " \u{2190} \u{2192} to adjust",
                Style::default().fg(theme::MUTED),
            ));
        }

        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    // Footer
    lines.push(Line::from(vec![
        Span::styled(
            "  Enter",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" to confirm ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "\u{00b7}",
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(" ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" to exit", Style::default().fg(theme::MUTED)),
    ]));

    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model_panel::{AliasTab, ModelPanel, ROW_OPUS};
    use crate::app::App;

    fn render_headless_model_no_provider() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.core.model_panel = Some(ModelPanel {
            provider_name: String::new(),
            cursor: ROW_OPUS,
            active_tab: AliasTab::Opus,
            buf_thinking_effort: "medium".to_string(),
        });
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_model_panel_renders_select_model_title() {
        let (_, handle) = render_headless_model_no_provider();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("Select model"),
            "Panel should show 'Select model' title, got:\n{}",
            snap
        );
    }

    #[tokio::test]
    async fn test_model_panel_shows_effort() {
        let (_, handle) = render_headless_model_no_provider();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("effort"),
            "Panel should show effort setting, got:\n{}",
            snap
        );
    }
}
