use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::app::setup_wizard::{
    FormField, FormMode, SetupSource, SetupStep, SetupWizardPanel, LANGUAGE_OPTIONS,
};
use crate::ui::theme;

/// Setup 向导全屏渲染入口
pub(crate) fn render_setup_wizard(f: &mut Frame, app: &crate::app::App) {
    let area = f.area();
    let wizard = app.global_ui.setup_wizard.as_ref().unwrap();
    let lc = &app.services.lc;

    match wizard.step {
        SetupStep::Choose => render_step_choose(f, wizard, lc, area),
        SetupStep::Language => render_step_language(f, wizard, lc, area),
        SetupStep::Form => render_step_form(f, wizard, lc, area),
        SetupStep::Done => render_step_done(f, wizard, lc, area),
    }
}

fn render_step_choose(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        lc.tr("setup-welcome-title"),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            lc.tr("setup-choose-provider"),
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
    ];

    for (i, src) in SetupSource::ALL.iter().enumerate() {
        let is_cursor = i == wizard.choose_cursor;
        let cursor_char = if is_cursor { "❯" } else { " " };
        let cursor_style = Style::default().fg(theme::THINKING);
        let label_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        };
        let desc_style = if is_cursor {
            Style::default().fg(theme::THINKING)
        } else {
            Style::default().fg(theme::MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", cursor_char), cursor_style),
            Span::styled(format!("{} ", src.label(lc)), label_style),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(src.description(lc), desc_style),
        ]));
        lines.push(Line::from(""));
    }

    lines.push(make_hint_line(vec![
        ("Enter".to_string(), lc.tr("setup-key-confirm")),
        ("↑/↓".to_string(), lc.tr("setup-key-select")),
        ("Esc".to_string(), lc.tr("setup-key-quit")),
    ]));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_step_language(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        lc.tr("setup-language-title"),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            lc.tr("setup-language-prompt"),
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
    ];

    for (i, (_code, name)) in LANGUAGE_OPTIONS.iter().enumerate() {
        let is_cursor = i == wizard.language_cursor;
        let cursor_char = if is_cursor { "❯" } else { " " };
        let cursor_style = Style::default().fg(theme::THINKING);
        let name_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", cursor_char), cursor_style),
            Span::styled(*name, name_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(make_hint_line(vec![
        ("Enter".to_string(), lc.tr("setup-key-confirm")),
        ("\u{2191}/\u{2193}".to_string(), lc.tr("setup-key-select")),
        ("Esc".to_string(), lc.tr("setup-key-quit")),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_step_form(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    match wizard.form_mode {
        FormMode::Browse => render_form_browse(f, wizard, lc, area),
        FormMode::Edit => render_form_edit(f, wizard, lc, area),
    }
}

/// Browse 模式：只读列表 + Submit
fn render_form_browse(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        lc.tr("setup-configure-title"),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![Line::from("")];

    let submit_pos = wizard.providers.len();

    for (idx, mp) in wizard.providers.iter().enumerate() {
        let is_cursor = idx == wizard.browse_cursor;
        let cursor = if is_cursor { "❯" } else { " " };
        let check_char = if mp.selected { "✓" } else { " " };
        let check_color = if mp.selected { theme::SAGE } else { theme::DIM };
        let name_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let detail_style = if is_cursor {
            Style::default().fg(theme::THINKING)
        } else {
            Style::default().fg(theme::MUTED)
        };

        let key_summary = if mp.api_key.is_empty() {
            lc.tr("setup-no-key")
        } else {
            mask_api_key(&mp.api_key)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", cursor), Style::default().fg(theme::THINKING)),
            Span::styled(
                format!("[{}] ", check_char),
                Style::default().fg(check_color),
            ),
            Span::styled(format!("{} ", mp.provider_type.label(lc)), name_style),
            Span::styled(
                format!("({}) ", mp.provider_id),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(key_summary, detail_style),
        ]));

        // 第二行：base_url 摘要
        if !mp.base_url.is_empty() {
            let url_style = if is_cursor {
                Style::default().fg(theme::DIM)
            } else {
                Style::default().fg(theme::DIM)
            };
            lines.push(Line::from(vec![
                Span::styled("     ", Style::default()),
                Span::styled(&mp.base_url, url_style),
            ]));
        }

        lines.push(Line::from(""));
    }

    // Submit 错误提示
    if let Some(ref err) = wizard.submit_error {
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {}", err),
            Style::default().fg(theme::WARNING),
        )));
        lines.push(Line::from(""));
    }

    // Submit 按钮
    let submit_active = wizard.browse_cursor == submit_pos;
    let submit_style = if submit_active {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let submit_cursor = if submit_active { "❯ " } else { "  " };
    lines.push(Line::from(vec![
        Span::styled(submit_cursor, Style::default().fg(theme::THINKING)),
        Span::styled(format!(" {}", lc.tr("setup-submit")), submit_style),
    ]));

    lines.push(Line::from(""));
    lines.push(make_hint_line(vec![
        ("Enter".to_string(), lc.tr("setup-key-edit-submit")),
        ("Space".to_string(), lc.tr("setup-key-check")),
        ("↑/↓".to_string(), lc.tr("setup-key-select")),
        ("Esc".to_string(), lc.tr("setup-key-back")),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Edit 模式：编辑单个 provider 的所有字段
fn render_form_edit(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    let mp = match wizard.providers.get(wizard.active_provider) {
        Some(provider) => provider,
        None => {
            let inner = BorderedPanel::new(Span::styled(
                lc.tr("setup-configure-title"),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(Style::default().fg(theme::ACCENT))
            .render(f, area);
            f.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "Internal error: invalid provider index",
                        Style::default().fg(theme::WARNING),
                    )),
                ])),
                inner,
            );
            return;
        }
    };
    let header = lc.tr_args(
        "setup-edit-title",
        &[
            ("type".into(), mp.provider_type.label(lc).into()),
            ("id".into(), mp.provider_id.clone().into()),
        ],
    );

    let inner = BorderedPanel::new(Span::styled(
        header,
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![Line::from("")];

    lines.push(render_field_line(
        &lc.tr("setup-field-type"),
        4,
        FormField::ProviderType,
        format!("[{}]", mp.provider_type.label(lc)),
        wizard.form_focus,
    ));

    let pid_display = edit_display(
        &mp.provider_id,
        mp.cur_provider_id,
        wizard.form_focus == FormField::ProviderId,
    );
    lines.push(render_field_line(
        &lc.tr("setup-field-id"),
        4,
        FormField::ProviderId,
        pid_display,
        wizard.form_focus,
    ));

    let url_display = edit_display(
        &mp.base_url,
        mp.cur_base_url,
        wizard.form_focus == FormField::BaseUrl,
    );
    lines.push(render_field_line(
        &lc.tr("setup-field-base-url"),
        4,
        FormField::BaseUrl,
        url_display,
        wizard.form_focus,
    ));

    let key_display = if wizard.form_focus == FormField::ApiKey {
        let (before, after) = crate::app::edit_display_parts(&mp.api_key, mp.cur_api_key);
        format!("{}▏{}", before, after)
    } else if mp.api_key.is_empty() {
        String::new()
    } else {
        "•".repeat(mp.api_key.chars().count())
    };
    lines.push(render_field_line(
        &lc.tr("setup-field-api-key"),
        4,
        FormField::ApiKey,
        key_display,
        wizard.form_focus,
    ));

    lines.push(Line::from(Span::styled(
        "  ─────────────────────────────────",
        Style::default().fg(theme::DIM),
    )));

    let alias_labels = [
        (lc.tr("setup-field-opus"), FormField::OpusModel, 0),
        (lc.tr("setup-field-sonnet"), FormField::SonnetModel, 1),
        (lc.tr("setup-field-haiku"), FormField::HaikuModel, 2),
    ];
    for (label, field, ai) in alias_labels {
        let model_display = edit_display(
            &mp.aliases[ai].model_id,
            mp.aliases[ai].cursor,
            wizard.form_focus == field,
        );
        lines.push(render_field_line(
            &format!("{} {} ", label, lc.tr("setup-model-label")),
            4,
            field,
            model_display,
            wizard.form_focus,
        ));
    }

    // Confirm 按钮
    let confirm_active = wizard.form_focus == FormField::Confirm;
    let confirm_style = if confirm_active {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let confirm_cursor = if confirm_active { "❯ " } else { "  " };
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(confirm_cursor, Style::default().fg(theme::THINKING)),
        Span::styled(format!(" {}", lc.tr("setup-confirm")), confirm_style),
    ]));

    lines.push(Line::from(""));
    lines.push(make_hint_line(vec![
        ("Enter".to_string(), lc.tr("setup-key-confirm")),
        ("←/→".to_string(), lc.tr("setup-key-switch-type")),
        ("Esc".to_string(), lc.tr("setup-key-back-list")),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// 渲染单个字段行（带光标指示器、标签固定宽度右对齐）
fn render_field_line(
    label: &str,
    label_width: usize,
    field: FormField,
    value: String,
    focus: FormField,
) -> Line<'static> {
    let is_active = focus == field;
    let cursor = if is_active { "❯ " } else { "  " };
    let lbl = if is_active {
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let val = if is_active {
        Style::default().fg(theme::THINKING)
    } else {
        Style::default().fg(theme::TEXT)
    };
    let padded = pad_display_columns(label, label_width);
    Line::from(vec![
        Span::styled(cursor, Style::default().fg(theme::THINKING)),
        Span::styled(padded, lbl),
        Span::styled(format!(" {}", value), val),
    ])
}

/// 将字符串右对齐填充到指定 Unicode 显示列宽
fn pad_display_columns(s: &str, target_cols: usize) -> String {
    let cols: usize = s
        .chars()
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    if cols >= target_cols {
        s.to_string()
    } else {
        let padding = target_cols - cols;
        format!("{}{}", s, " ".repeat(padding))
    }
}

/// 编辑字段显示：活跃时显示光标 ▏，否则显示值
fn edit_display(value: &str, cursor: usize, active: bool) -> String {
    if active {
        let (before, after) = crate::app::edit_display_parts(value, cursor);
        format!("{}▏{}", before, after)
    } else {
        value.to_string()
    }
}

fn render_step_done(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        lc.tr("setup-complete-title"),
        Style::default()
            .fg(theme::SAGE)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::SAGE))
    .render(f, area);

    let mut lines = vec![Line::from("")];

    let selected: Vec<_> = wizard.providers.iter().filter(|p| p.selected).collect();
    for mp in &selected {
        lines.push(Line::from(vec![
            Span::styled(" ● ", Style::default().fg(theme::SAGE)),
            Span::styled(
                format!("{} ", mp.provider_type.label(lc)),
                Style::default().fg(theme::TEXT),
            ),
            Span::styled(
                format!("({})", mp.provider_id),
                Style::default().fg(theme::MUTED),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                format!("   {} ", lc.tr("setup-label-key")),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(mask_api_key(&mp.api_key), Style::default().fg(theme::TEXT)),
        ]));
        let alias_labels = [
            lc.tr("setup-field-opus"),
            lc.tr("setup-field-sonnet"),
            lc.tr("setup-field-haiku"),
        ];
        for (i, label) in alias_labels.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("   {:>6} → ", label),
                    Style::default().fg(theme::MUTED),
                ),
                Span::styled(&mp.aliases[i].model_id, Style::default().fg(theme::ACCENT)),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", lc.tr("setup-press-enter")),
            Style::default().fg(theme::TEXT),
        ),
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::SAGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(lc.tr("setup-to-start"), Style::default().fg(theme::TEXT)),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// 生成底部快捷键提示行
fn make_hint_line(items: Vec<(String, String)>) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in items.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            key,
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(desc, Style::default().fg(theme::MUTED)));
    }
    Line::from(spans)
}

/// API Key 脱敏
fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    if len <= 8 {
        "•".repeat(len)
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[len - 4..].iter().collect();
        format!("{}••••{}", prefix, suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::setup_wizard::SetupWizardPanel;
    use crate::app::App;
    include!("setup_wizard_test.rs");
}
