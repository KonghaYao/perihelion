use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::config_panel::{ConfigEditField, ConfigPanel, ConfigPanelMode};
use crate::app::App;
use crate::ui::theme;

/// /config 面板渲染
pub(crate) fn render_config_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(panel) = &app.sessions[app.active].core.config_panel else {
        return;
    };

    let border_color = match panel.mode {
        ConfigPanelMode::Browse => theme::BORDER,
        ConfigPanelMode::Edit => theme::WARNING,
    };

    let title = match panel.mode {
        ConfigPanelMode::Browse => " /config — 配置 ",
        ConfigPanelMode::Edit => " /config — 编辑配置 ",
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(border_color))
    .render(f, area);

    match panel.mode {
        ConfigPanelMode::Browse => {
            let mut lines: Vec<Line> = Vec::new();
            for i in 0..ConfigPanel::field_count() {
                let is_cursor = i == panel.cursor;
                let cursor_char = if is_cursor { "❯ " } else { "  " };
                let label = ConfigPanel::field_label(i);
                let value = panel.field_display_value(i);

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
                    Span::styled(format!("{:<14}", label), style),
                    Span::styled(value, Style::default().fg(theme::TEXT)),
                ]));
            }
            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }

        ConfigPanelMode::Edit => {
            let mut lines: Vec<Line> = vec![Line::from("")];

            // Autocompact
            {
                let is_active = panel.edit_field == ConfigEditField::Autocompact;
                let on_off = if panel.buf_autocompact {
                    vec![
                        Span::styled(
                            "[开]",
                            Style::default()
                                .fg(theme::THINKING)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("  关", Style::default().fg(theme::MUTED)),
                    ]
                } else {
                    vec![
                        Span::styled("开  ", Style::default().fg(theme::MUTED)),
                        Span::styled(
                            "[关]",
                            Style::default()
                                .fg(theme::THINKING)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]
                };
                let label_style = if is_active {
                    Style::default()
                        .fg(theme::THINKING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::MUTED)
                };
                let mut spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("{:<14}", ConfigEditField::Autocompact.label()),
                        label_style,
                    ),
                ];
                spans.extend(on_off);
                lines.push(Line::from(spans));
            }

            // CompactThreshold (text input)
            render_text_field(
                &mut lines,
                ConfigEditField::CompactThreshold,
                &panel.edit_field,
                &panel.buf_threshold,
                panel.cur_threshold,
            );

            // Language (text input)
            render_text_field(
                &mut lines,
                ConfigEditField::Language,
                &panel.edit_field,
                &panel.buf_language,
                panel.cur_language,
            );

            // Persona (text input)
            render_text_field(
                &mut lines,
                ConfigEditField::Persona,
                &panel.edit_field,
                &panel.buf_persona,
                panel.cur_persona,
            );

            // Tone (text input)
            render_text_field(
                &mut lines,
                ConfigEditField::Tone,
                &panel.edit_field,
                &panel.buf_tone,
                panel.cur_tone,
            );

            // Proactiveness (radio)
            {
                let is_active = panel.edit_field == ConfigEditField::Proactiveness;
                let active_style = Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD);
                let inactive_style = Style::default().fg(theme::MUTED);
                let vals = ["low", "medium", "high"];
                let spans: Vec<Span> = vals
                    .iter()
                    .flat_map(|v| {
                        let cur = panel.buf_proactiveness.as_str();
                        if *v == cur {
                            vec![
                                Span::styled(format!("[{}]", v), active_style),
                                Span::styled("  ", Style::default()),
                            ]
                        } else {
                            vec![
                                Span::styled(v.to_string(), inactive_style),
                                Span::styled("  ", Style::default()),
                            ]
                        }
                    })
                    .collect();
                let label_style = if is_active {
                    Style::default()
                        .fg(theme::THINKING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::MUTED)
                };
                let mut line_spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("{:<14}", ConfigEditField::Proactiveness.label()),
                        label_style,
                    ),
                ];
                line_spans.extend(spans);
                lines.push(Line::from(line_spans));
            }

            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
    }
}

fn render_text_field(
    lines: &mut Vec<Line<'static>>,
    field: ConfigEditField,
    active_field: &ConfigEditField,
    buf: &str,
    cursor: usize,
) {
    let is_active = field == *active_field;
    let label_style = if is_active {
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let value_style = if is_active {
        Style::default().fg(theme::THINKING)
    } else {
        Style::default().fg(theme::TEXT)
    };

    let value_display = if is_active {
        let (before, after) = crate::app::edit_display_parts(buf, cursor);
        format!("{}█{}", before, after)
    } else if buf.is_empty() {
        "-".to_string()
    } else {
        buf.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(format!("{:<14}", field.label()), label_style),
        Span::styled(value_display, value_style),
    ]));
}
