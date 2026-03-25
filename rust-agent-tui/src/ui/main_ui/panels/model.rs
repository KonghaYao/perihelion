use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::app::model_panel::{AliasEditField, AliasTab, EditField, ModelPanelMode, PROVIDER_TYPES};

/// /model 面板渲染
pub(crate) fn render_model_panel(f: &mut Frame, app: &App) {
    let Some(panel) = &app.model_panel else { return };

    let area = f.area();
    let popup_width = (area.width * 4 / 5).max(60).min(area.width.saturating_sub(4));
    let popup_height = 22u16.min(area.height * 4 / 5).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    // 根据模式选颜色/标题
    let (border_color, title) = match &panel.mode {
        ModelPanelMode::AliasConfig   => (Color::Cyan,   " /model — 模型别名配置 "),
        ModelPanelMode::Browse        => (Color::Cyan,   " /model — Provider 管理 "),
        ModelPanelMode::Edit          => (Color::Yellow, " /model — 编辑 Provider "),
        ModelPanelMode::New           => (Color::Green,  " /model — 新建 Provider "),
        ModelPanelMode::ConfirmDelete => (Color::Red,    " /model — 确认删除 "),
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    match &panel.mode {
        // ── 别名配置主界面 ─────────────────────────────────────────────────────
        ModelPanelMode::AliasConfig => {
            // 获取激活别名（从 ZenConfig 读取）
            let active_alias = app.zen_config.as_ref()
                .map(|c| c.config.active_alias.as_str())
                .unwrap_or("opus");

            // Tab 栏（第 0 行）
            let tabs_line = {
                let tabs = [AliasTab::Opus, AliasTab::Sonnet, AliasTab::Haiku];
                let mut spans: Vec<Span> = Vec::new();
                spans.push(Span::styled(" ", Style::default()));
                for tab in &tabs {
                    let is_current = *tab == panel.active_tab;
                    let is_active_alias = tab.to_key() == active_alias;
                    let label = if is_active_alias {
                        format!("[★ {}]", tab.label())
                    } else {
                        format!("[ {} ]", tab.label())
                    };
                    let style = if is_current {
                        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else if is_active_alias {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    spans.push(Span::styled(label, style));
                    spans.push(Span::styled("  ", Style::default()));
                }
                Line::from(spans)
            };

            // 当前 Tab 的 provider/model 编辑区
            let tab_idx = panel.active_tab.index();
            let cur_provider = &panel.buf_alias_provider[tab_idx];
            let cur_model = &panel.buf_alias_model[tab_idx];

            // Provider 行：显示所有 provider，当前选中用 [name] 包裹
            let provider_is_active = panel.alias_edit_field == AliasEditField::Provider;
            let model_is_active = panel.alias_edit_field == AliasEditField::ModelId;

            let provider_display: String = if panel.providers.is_empty() {
                "（无，按 p 管理）".to_string()
            } else {
                panel.providers.iter()
                    .map(|p| {
                        if &p.id == cur_provider {
                            format!("[{}]", p.display_name())
                        } else {
                            p.display_name().to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
            };

            let model_display = if model_is_active {
                format!("{}█", cur_model)
            } else if cur_model.is_empty() {
                "（空，使用 Provider 默认）".to_string()
            } else {
                cur_model.clone()
            };

            let (prov_label_style, prov_val_style) = if provider_is_active {
                (
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::Black).bg(Color::Cyan),
                )
            } else {
                (Style::default().fg(Color::DarkGray), Style::default().fg(Color::White))
            };

            let (model_label_style, model_val_style) = if model_is_active {
                (
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::Black).bg(Color::Cyan),
                )
            } else {
                (Style::default().fg(Color::DarkGray), Style::default().fg(Color::White))
            };

            let provider_line = Line::from(vec![
                Span::styled("  Provider", prov_label_style),
                Span::styled(format!("  {}", provider_display), prov_val_style),
            ]);
            let model_line = Line::from(vec![
                Span::styled("  Model ID", model_label_style),
                Span::styled(format!("  {}", model_display), model_val_style),
            ]);

            let hint_line = Line::from(vec![
                Span::styled(" Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":切换Tab  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":激活  ", Style::default().fg(Color::DarkGray)),
                Span::styled("↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":切换字段  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":切换Provider  ", Style::default().fg(Color::DarkGray)),
                Span::styled("p", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":管理  ", Style::default().fg(Color::DarkGray)),
                Span::styled("s", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":保存  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":关闭", Style::default().fg(Color::DarkGray)),
            ]);

            let mut lines = vec![
                tabs_line,
                Line::from(""),
                provider_line,
                model_line,
                Line::from(""),
                hint_line,
            ];
            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
        // ── Provider 管理子面板 ────────────────────────────────────────────────
        ModelPanelMode::Browse | ModelPanelMode::Edit | ModelPanelMode::New | ModelPanelMode::ConfirmDelete => {
            let half = inner.height / 2;
            let list_area = Rect { height: half.max(3), ..inner };
            let form_area = Rect {
                y: inner.y + list_area.height,
                height: inner.height.saturating_sub(list_area.height),
                ..inner
            };

            // 上半：provider 列表
            let mut list_lines: Vec<Line> = Vec::new();
            for (i, p) in panel.providers.iter().enumerate() {
                let is_cursor = i == panel.cursor;
                let is_active = p.id == panel.active_id;
                let bullet = if is_active { "●" } else { "○" };
                let cursor_char = if is_cursor { "▶" } else { " " };
                let name = p.display_name().to_string();
                let type_tag = format!("({})", p.provider_type);
                let row_style = if is_cursor {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else if is_active {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                list_lines.push(Line::from(vec![
                    Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
                    Span::styled(format!("{} ", name), row_style.add_modifier(Modifier::BOLD)),
                    Span::styled(type_tag, row_style.fg(if is_cursor { Color::Black } else { Color::DarkGray })),
                ]));
            }
            if panel.providers.is_empty() {
                list_lines.push(Line::from(Span::styled(
                    "  （无 provider，按 n 新建）",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            f.render_widget(Paragraph::new(Text::from(list_lines)), list_area);

            // 下半：表单或确认删除
            match &panel.mode {
                ModelPanelMode::Browse => {
                    if let Some(p) = panel.providers.get(panel.cursor) {
                        let key_masked = mask_api_key(&p.api_key);
                        let mut info_lines = vec![
                            Line::from(vec![
                                Span::styled("  API Key ", Style::default().fg(Color::DarkGray)),
                                Span::styled(key_masked, Style::default().fg(Color::White)),
                            ]),
                            Line::from(vec![
                                Span::styled("  Base URL", Style::default().fg(Color::DarkGray)),
                                Span::styled(format!(" {}", p.base_url), Style::default().fg(Color::White)),
                            ]),
                        ];
                        let thinking_status = if panel.buf_thinking_enabled {
                            format!(" ON  (budget: {} tokens)", panel.buf_thinking_budget)
                        } else {
                            " OFF".to_string()
                        };
                        let thinking_color = if panel.buf_thinking_enabled { Color::Rgb(150, 120, 200) } else { Color::DarkGray };
                        info_lines.push(Line::from(vec![
                            Span::styled("  Thinking", Style::default().fg(Color::DarkGray)),
                            Span::styled(thinking_status, Style::default().fg(thinking_color)),
                        ]));
                        info_lines.push(Line::from(""));
                        info_lines.push(Line::from(vec![
                            Span::styled(" e", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                            Span::styled(":编辑  ", Style::default().fg(Color::DarkGray)),
                            Span::styled("n", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                            Span::styled(":新建  ", Style::default().fg(Color::DarkGray)),
                            Span::styled("d", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                            Span::styled(":删除  ", Style::default().fg(Color::DarkGray)),
                            Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                            Span::styled(":返回", Style::default().fg(Color::DarkGray)),
                        ]));
                        info_lines.truncate(form_area.height as usize);
                        f.render_widget(Paragraph::new(Text::from(info_lines)), form_area);
                    }
                }
                ModelPanelMode::Edit | ModelPanelMode::New => {
                    let fields: &[(EditField, &str)] = &[
                        (EditField::Name,          &panel.buf_name),
                        (EditField::ProviderType,  &panel.buf_type),
                        (EditField::ApiKey,        &panel.buf_api_key),
                        (EditField::BaseUrl,       &panel.buf_base_url),
                    ];
                    let mut form_lines: Vec<Line> = Vec::new();
                    for (field, buf) in fields {
                        let is_active = *field == panel.edit_field;
                        let label = field.label();
                        let value_display = if *field == EditField::ProviderType {
                            PROVIDER_TYPES.iter()
                                .map(|t| if *t == *buf { format!("[{}]", t) } else { t.to_string() })
                                .collect::<Vec<_>>()
                                .join("  ")
                        } else if is_active {
                            format!("{}█", buf)
                        } else if *field == EditField::ApiKey { mask_api_key(buf) } else { buf.to_string() };
                        let (label_style, value_style) = if is_active {
                            (
                                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
                                Style::default().fg(Color::Black).bg(Color::Cyan),
                            )
                        } else {
                            (Style::default().fg(Color::DarkGray), Style::default().fg(Color::White))
                        };
                        form_lines.push(Line::from(vec![
                            Span::styled(format!("  {} ", label), label_style),
                            Span::styled(format!(" {}", value_display), value_style),
                        ]));
                    }
                    // ThinkingBudget 字段
                    {
                        let is_active = panel.edit_field == EditField::ThinkingBudget;
                        let label = EditField::ThinkingBudget.label();
                        let enabled_tag = if panel.buf_thinking_enabled { "[ON] " } else { "[OFF]" };
                        let budget_display = if is_active {
                            format!("{}█", panel.buf_thinking_budget)
                        } else {
                            panel.buf_thinking_budget.clone()
                        };
                        let enabled_color = if panel.buf_thinking_enabled { Color::Rgb(150, 120, 200) } else { Color::DarkGray };
                        let (label_style, enabled_style, budget_style) = if is_active {
                            (
                                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
                                Style::default().fg(if panel.buf_thinking_enabled { Color::Rgb(180, 100, 255) } else { Color::DarkGray }).bg(Color::Cyan),
                                Style::default().fg(Color::Black).bg(Color::Cyan),
                            )
                        } else {
                            (Style::default().fg(Color::DarkGray), Style::default().fg(enabled_color), Style::default().fg(Color::White))
                        };
                        form_lines.push(Line::from(vec![
                            Span::styled(format!("  {} ", label), label_style),
                            Span::styled(format!(" {} ", enabled_tag), enabled_style),
                            Span::styled(format!("budget:{}", budget_display), budget_style),
                        ]));
                    }
                    form_lines.push(Line::from(""));
                    form_lines.push(Line::from(vec![
                        Span::styled(" Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":切换字段  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":切换/开关  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":保存  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":取消", Style::default().fg(Color::DarkGray)),
                    ]));
                    form_lines.truncate(form_area.height as usize);
                    f.render_widget(Paragraph::new(Text::from(form_lines)), form_area);
                }
                ModelPanelMode::ConfirmDelete => {
                    if let Some(p) = panel.providers.get(panel.cursor) {
                        let confirm_lines = vec![
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("  确认删除 ", Style::default().fg(Color::White)),
                                Span::styled(p.display_name().to_string(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                                Span::styled(" ？", Style::default().fg(Color::White)),
                            ]),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled(" y", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                                Span::styled(":确认删除  ", Style::default().fg(Color::DarkGray)),
                                Span::styled("n/Esc", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                                Span::styled(":取消", Style::default().fg(Color::DarkGray)),
                            ]),
                        ];
                        f.render_widget(Paragraph::new(Text::from(confirm_lines)), form_area);
                    }
                }
                _ => {}
            }
        }
    }
}

/// 遮盖 API Key 中间部分
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
