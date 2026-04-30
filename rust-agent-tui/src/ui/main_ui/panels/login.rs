use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::login_panel::{LoginEditField, LoginPanelMode};
use crate::app::App;
use crate::ui::theme;

/// /login 面板渲染（底部展开区）
pub(crate) fn render_login_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(panel) = &app.core.login_panel else {
        return;
    };

    let (border_color, title) = match panel.mode {
        LoginPanelMode::Browse => (theme::MUTED, " /login — Provider 管理 "),
        LoginPanelMode::Edit => (theme::WARNING, " /login — 编辑 Provider "),
        LoginPanelMode::New => (theme::SAGE, " /login — 新建 Provider "),
        LoginPanelMode::ConfirmDelete => (theme::ERROR, " /login — 确认删除 "),
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(border_color))
    .render(f, area);

    let active_provider_id = app
        .zen_config
        .as_ref()
        .map(|c| c.config.active_provider_id.as_str())
        .unwrap_or("");

    match panel.mode {
        LoginPanelMode::Browse => {
            let mut lines: Vec<Line> = Vec::new();
            for (i, p) in panel.providers.iter().enumerate() {
                let is_cursor = i == panel.cursor;
                let is_active = p.id == active_provider_id;
                let bullet = if is_active { "●" } else { "○" };
                let cursor_char = if is_cursor { "▶" } else { " " };
                let name = p.display_name().to_string();
                let type_tag = format!("({})", p.provider_type);
                let row_style = if is_cursor {
                    Style::default().fg(Color::White).bg(theme::ACCENT)
                } else if is_active {
                    Style::default().fg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
                    Span::styled(format!("{} ", name), row_style.add_modifier(Modifier::BOLD)),
                    Span::styled(
                        type_tag,
                        row_style.fg(if is_cursor {
                            Color::White
                        } else {
                            theme::MUTED
                        }),
                    ),
                ]));
                // 模型名子行（一行展示三个模型，不同文字颜色区分）
                let m = &p.models;
                let fmt_model = |v: &str| -> String {
                    if v.is_empty() {
                        "（未设置）".to_string()
                    } else {
                        v.to_string()
                    }
                };
                let label_fg = if is_cursor { Color::White } else { theme::TEXT };
                let (opus_fg, sonnet_fg, haiku_fg) = if is_cursor {
                    (Color::Cyan, Color::Magenta, Color::Yellow)
                } else {
                    (Color::Cyan, Color::Magenta, Color::Yellow)
                };
                let sep = if is_cursor {
                    Style::default().fg(Color::White).bg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::MUTED)
                };
                lines.push(Line::from(vec![
                    Span::styled("       ", sep),
                    Span::styled(
                        "Opus ",
                        Style::default().fg(label_fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.opus), Style::default().fg(opus_fg)),
                    Span::styled(
                        "  Sonnet ",
                        Style::default().fg(label_fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.sonnet), Style::default().fg(sonnet_fg)),
                    Span::styled(
                        "  Haiku ",
                        Style::default().fg(label_fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.haiku), Style::default().fg(haiku_fg)),
                ]));
            }
            if panel.providers.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  （无 provider，按 Ctrl+N 新建）",
                    Style::default().fg(theme::MUTED),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    " Enter",
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":选中  ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":编辑  ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    "Ctrl+N",
                    Style::default()
                        .fg(theme::SAGE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":新建  ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    "Ctrl+D",
                    Style::default()
                        .fg(theme::ERROR)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":删除", Style::default().fg(theme::MUTED)),
            ]));
            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }

        LoginPanelMode::Edit | LoginPanelMode::New => {
            let fields: &[(LoginEditField, &str, &str)] = &[
                (LoginEditField::Name, "Name        ", &panel.buf_name),
                (LoginEditField::Type, "Type        ", &panel.buf_type),
                (LoginEditField::BaseUrl, "Base URL    ", &panel.buf_base_url),
                (LoginEditField::ApiKey, "API Key     ", &panel.buf_api_key),
                (
                    LoginEditField::OpusModel,
                    "Opus Model  ",
                    &panel.buf_opus_model,
                ),
                (
                    LoginEditField::SonnetModel,
                    "Sonnet Model",
                    &panel.buf_sonnet_model,
                ),
                (
                    LoginEditField::HaikuModel,
                    "Haiku Model ",
                    &panel.buf_haiku_model,
                ),
            ];

            let mut lines: Vec<Line> = Vec::new();
            for (field, label, value) in fields {
                let is_active = *field == panel.edit_field;
                let value_display = if *field == LoginEditField::Type {
                    let types = ["openai", "anthropic"];
                    types
                        .iter()
                        .map(|t| {
                            if *t == *value {
                                format!("[{}]", t)
                            } else {
                                t.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("  ")
                } else if *field == LoginEditField::ApiKey && !is_active {
                    mask_api_key(value)
                } else if is_active {
                    format!("{}█", value)
                } else {
                    value.to_string()
                };

                let (label_style, value_style) = if is_active {
                    (
                        Style::default()
                            .fg(Color::White)
                            .bg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                        Style::default().fg(Color::White).bg(theme::ACCENT),
                    )
                } else {
                    (
                        Style::default().fg(theme::MUTED),
                        Style::default().fg(theme::TEXT),
                    )
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", label), label_style),
                    Span::styled(format!(" {}", value_display), value_style),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    " ↑↓",
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":切换字段  ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    "←→/Space",
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":切换Type  ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":保存  ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    "Ctrl+V",
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(":粘贴  ", Style::default().fg(theme::MUTED)),
                Span::styled("Esc", Style::default().fg(theme::MUTED)),
                Span::styled(":取消", Style::default().fg(theme::MUTED)),
            ]));
            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }

        LoginPanelMode::ConfirmDelete => {
            let mut list_lines: Vec<Line> = Vec::new();
            for (i, p) in panel.providers.iter().enumerate() {
                let is_cursor = i == panel.cursor;
                let is_active = p.id == active_provider_id;
                let bullet = if is_active { "●" } else { "○" };
                let cursor_char = if is_cursor { "▶" } else { " " };
                let row_style = if is_cursor {
                    Style::default().fg(Color::White).bg(theme::ACCENT)
                } else if is_active {
                    Style::default().fg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                list_lines.push(Line::from(vec![
                    Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
                    Span::styled(
                        p.display_name().to_string(),
                        row_style.add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            list_lines.truncate(inner.height.saturating_sub(5) as usize);
            f.render_widget(Paragraph::new(Text::from(list_lines)), inner);

            let confirm_y = inner.y + inner.height.saturating_sub(4);
            let confirm_area = Rect {
                y: confirm_y,
                height: 4,
                ..inner
            };
            if let Some(p) = panel.providers.get(panel.cursor) {
                let confirm_lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  确认删除 ", Style::default().fg(theme::TEXT)),
                        Span::styled(
                            p.display_name().to_string(),
                            Style::default()
                                .fg(theme::ERROR)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" ？", Style::default().fg(theme::TEXT)),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            " Enter",
                            Style::default()
                                .fg(theme::ERROR)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(":确认删除  ", Style::default().fg(theme::MUTED)),
                        Span::styled(
                            "Esc",
                            Style::default()
                                .fg(theme::SAGE)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(":取消", Style::default().fg(theme::MUTED)),
                    ]),
                ];
                f.render_widget(Paragraph::new(Text::from(confirm_lines)), confirm_area);
            }
        }
    }
}

fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[len - 4..].iter().collect();
    format!("{}****{}", prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::login_panel::{LoginEditField, LoginPanel, LoginPanelMode};
    use crate::app::App;
    use crate::config::ProviderConfig;

    fn render_headless_login_browse() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.core.login_panel = Some(LoginPanel {
            providers: vec![ProviderConfig {
                id: "test".to_string(),
                provider_type: "openai".to_string(),
                base_url: "http://localhost".to_string(),
                api_key: "sk-test".to_string(),
                models: crate::config::ProviderModels {
                    opus: "opus-model".to_string(),
                    sonnet: "sonnet-model".to_string(),
                    haiku: "haiku-model".to_string(),
                },
                ..Default::default()
            }],
            mode: LoginPanelMode::Browse,
            cursor: 0,
            edit_field: LoginEditField::Name,
            buf_name: String::new(),
            buf_type: String::new(),
            buf_base_url: String::new(),
            buf_api_key: String::new(),
            buf_opus_model: String::new(),
            buf_sonnet_model: String::new(),
            buf_haiku_model: String::new(),
            scroll_offset: 0,
        });
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_login_browse_no_single_letter_hints() {
        let (_, handle) = render_headless_login_browse();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("Ctrl+N"),
            "新建应显示 Ctrl+N 而非单字母 n，实际:\n{}",
            snap
        );
        assert!(
            snap.contains("Ctrl+D"),
            "删除应显示 Ctrl+D 而非单字母 d，实际:\n{}",
            snap
        );
    }

    fn render_headless_login_edit() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.core.login_panel = Some(LoginPanel {
            providers: vec![],
            mode: LoginPanelMode::New,
            cursor: 0,
            edit_field: LoginEditField::Name,
            buf_name: String::new(),
            buf_type: "openai".to_string(),
            buf_base_url: String::new(),
            buf_api_key: String::new(),
            buf_opus_model: String::new(),
            buf_sonnet_model: String::new(),
            buf_haiku_model: String::new(),
            scroll_offset: 0,
        });
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_login_edit_has_paste_hint() {
        let (_, handle) = render_headless_login_edit();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("Ctrl+V"),
            "编辑模式应显示 Ctrl+V 粘贴提示，实际:\n{}",
            snap
        );
    }
}
