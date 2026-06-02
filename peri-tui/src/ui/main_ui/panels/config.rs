use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::{
    app::{
        config_panel::{
            ConfigPanel, ROW_AUTOCOMPACT, ROW_COUNT, ROW_DIFF, ROW_GENERAL_HEADER, ROW_LANGUAGE,
            ROW_OVERRIDES_HEADER, ROW_PERSONA, ROW_PROACTIVENESS, ROW_SEPARATOR, ROW_THRESHOLD,
            ROW_TONE,
        },
        App,
    },
    ui::theme,
};

/// 行号 → i18n 字段标签键
fn field_label_key(row: usize) -> &'static str {
    match row {
        ROW_AUTOCOMPACT => "config-field-autocompact",
        ROW_THRESHOLD => "config-field-compact-threshold",
        ROW_LANGUAGE => "config-field-language",
        ROW_DIFF => "config-field-diff",
        ROW_PERSONA => "config-field-persona",
        ROW_TONE => "config-field-tone",
        ROW_PROACTIVENESS => "config-field-proactiveness",
        _ => "???",
    }
}

/// 语言代码 → 显示名（不需要 i18n，语言名本身就是自描述的）
fn lang_display(code: &str) -> &str {
    match code {
        "en" => "English",
        "zh-CN" => "简体中文",
        _ => "auto",
    }
}

/// /config 面板渲染（单一直接编辑模式）
pub(crate) fn render_config_panel(f: &mut Frame, panel: &ConfigPanel, app: &mut App, area: Rect) {
    let lc = &app.services.lc;

    let title = lc.tr("config-panel-title");

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);

    let mut lines: Vec<Line> = Vec::new();

    for row in 0..ROW_COUNT {
        match row {
            ROW_GENERAL_HEADER => {
                lines.push(Line::from(vec![Span::styled(
                    lc.tr("config-group-general"),
                    Style::default()
                        .fg(theme::SAGE)
                        .add_modifier(Modifier::BOLD),
                )]));
            }
            ROW_SEPARATOR => {
                lines.push(Line::from(""));
            }
            ROW_OVERRIDES_HEADER => {
                lines.push(Line::from(vec![Span::styled(
                    lc.tr("config-group-prompt-overrides"),
                    Style::default()
                        .fg(theme::SAGE)
                        .add_modifier(Modifier::BOLD),
                )]));
            }
            ROW_AUTOCOMPACT => {
                let is_active = panel.cursor == row;
                let label_style = active_or_text(is_active);
                let active_style = Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD);
                let inactive_style = Style::default().fg(theme::MUTED);
                let desc_style = Style::default().fg(theme::MUTED);

                let on_span = if panel.buf_autocompact {
                    Span::styled(format!("[{}]", lc.tr("config-value-on")), active_style)
                } else {
                    Span::styled(lc.tr("config-value-on"), inactive_style)
                };
                let off_span = if panel.buf_autocompact {
                    Span::styled(lc.tr("config-value-off"), inactive_style)
                } else {
                    Span::styled(format!("[{}]", lc.tr("config-value-off")), active_style)
                };

                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
                    on_span,
                    Span::styled("  ", Style::default()),
                    off_span,
                    Span::styled(
                        format!("  {}", lc.tr("config-desc-autocompact")),
                        desc_style,
                    ),
                ]));
            }
            ROW_LANGUAGE => {
                let is_active = panel.cursor == row;
                let label_style = active_or_text(is_active);
                let active_style = Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD);
                let inactive_style = Style::default().fg(theme::MUTED);
                let desc_style = Style::default().fg(theme::MUTED);

                let options = ["en", "zh-CN"];
                let mut value_spans: Vec<Span> = Vec::new();
                for (i, code) in options.iter().enumerate() {
                    let display = lang_display(code);
                    let is_selected = *code == panel.buf_language
                        || (code.is_empty() && panel.buf_language.is_empty());
                    if is_selected {
                        value_spans.push(Span::styled(format!("[{}]", display), active_style));
                    } else {
                        value_spans.push(Span::styled(display.to_string(), inactive_style));
                    }
                    if i < options.len() - 1 {
                        value_spans.push(Span::styled("  ", Style::default()));
                    }
                }
                value_spans.push(Span::styled(
                    format!("  {}", lc.tr("config-desc-language")),
                    desc_style,
                ));

                let mut line_spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
                ];
                line_spans.extend(value_spans);
                lines.push(Line::from(line_spans));
            }
            ROW_DIFF => {
                let is_active = panel.cursor == row;
                let label_style = active_or_text(is_active);
                let active_style = Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD);
                let inactive_style = Style::default().fg(theme::MUTED);
                let desc_style = Style::default().fg(theme::MUTED);

                let on_span = if panel.buf_diff {
                    Span::styled(format!("[{}]", lc.tr("config-value-on")), active_style)
                } else {
                    Span::styled(lc.tr("config-value-on"), inactive_style)
                };
                let off_span = if panel.buf_diff {
                    Span::styled(lc.tr("config-value-off"), inactive_style)
                } else {
                    Span::styled(format!("[{}]", lc.tr("config-value-off")), active_style)
                };

                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
                    on_span,
                    Span::styled("  ", Style::default()),
                    off_span,
                    Span::styled(format!("  {}", lc.tr("config-desc-diff")), desc_style),
                ]));
            }
            ROW_PROACTIVENESS => {
                let is_active = panel.cursor == row;
                let label_style = active_or_text(is_active);
                let active_style = Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD);
                let inactive_style = Style::default().fg(theme::MUTED);
                let desc_style = Style::default().fg(theme::MUTED);

                let vals = ["low", "medium", "high"];
                let mut value_spans: Vec<Span> = Vec::new();
                for (i, v) in vals.iter().enumerate() {
                    if *v == panel.buf_proactiveness.as_str() {
                        value_spans.push(Span::styled(format!("[{}]", v), active_style));
                    } else {
                        value_spans.push(Span::styled(v.to_string(), inactive_style));
                    }
                    if i < vals.len() - 1 {
                        value_spans.push(Span::styled("  ", Style::default()));
                    }
                }
                value_spans.push(Span::styled(
                    format!("  {}", lc.tr("config-desc-proactiveness")),
                    desc_style,
                ));

                let mut line_spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
                ];
                line_spans.extend(value_spans);
                lines.push(Line::from(line_spans));
            }
            ROW_THRESHOLD | ROW_PERSONA | ROW_TONE => {
                let is_active = panel.cursor == row;
                let desc_key = match row {
                    ROW_THRESHOLD => "config-desc-threshold",
                    ROW_PERSONA => "config-desc-persona",
                    ROW_TONE => "config-desc-tone",
                    _ => "",
                };

                let (buf, cursor) = match row {
                    ROW_THRESHOLD => (&panel.buf_threshold, panel.cur_threshold),
                    ROW_PERSONA => (&panel.buf_persona, panel.cur_persona),
                    ROW_TONE => (&panel.buf_tone, panel.cur_tone),
                    _ => unreachable!(),
                };

                let label_style = active_or_text(is_active);
                let value_style = if is_active {
                    Style::default().fg(theme::THINKING)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                let desc_style = Style::default().fg(theme::MUTED);

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
                    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
                    Span::styled(value_display, value_style),
                    Span::styled(format!("  {}", lc.tr(desc_key)), desc_style),
                ]));
            }
            _ => {}
        }
    }

    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn active_or_text(is_active: bool) -> Style {
    if is_active {
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    }
}
