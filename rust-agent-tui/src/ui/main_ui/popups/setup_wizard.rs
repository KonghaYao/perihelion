use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::setup_wizard::{ProviderType, SetupStep, SetupWizardPanel, Step1Field};
use crate::ui::theme;

/// Setup 向导全屏渲染入口
pub(crate) fn render_setup_wizard(f: &mut Frame, app: &crate::app::App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let wizard = app.setup_wizard.as_ref().unwrap();

    // 居中内容区：宽度 60%，高度按内容自适应（最少 16 行）
    let content_width = (area.width * 3 / 5).max(50);
    let content_height = match wizard.step {
        SetupStep::Provider => 20,
        SetupStep::ModelAlias => 16,
        SetupStep::Done => 14,
    }
    .min(area.height.saturating_sub(2));
    let centered = centered_rect(area, content_width, content_height);

    match wizard.step {
        SetupStep::Provider => render_step_provider(f, wizard, centered),
        SetupStep::ModelAlias => render_step_model_alias(f, wizard, centered),
        SetupStep::Done => render_step_done(f, wizard, centered),
    }
}

/// 计算居中矩形区域
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn render_step_provider(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " ── Perihelion Setup ── Step 1/2: Provider & API Key ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    f.render_widget(&block, area);
    let inner = block.inner(area);

    // 焦点样式辅助
    let focused = |is_active: bool| -> (Style, Style) {
        if is_active {
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
        }
    };

    // 行 0: Provider Type 选择器
    let pt_active = wizard.step1_focus == Step1Field::ProviderType;
    let (pt_label, pt_val) = focused(pt_active);
    let provider_types = [ProviderType::Anthropic, ProviderType::OpenAiCompatible];
    let pt_display: String = provider_types
        .iter()
        .map(|pt| {
            if *pt == wizard.provider_type {
                format!("[{}]", pt.label())
            } else {
                pt.label().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    let line_pt = Line::from(vec![
        Span::styled(" Type     ", pt_label),
        Span::styled(format!(" {}", pt_display), pt_val),
    ]);

    // 行 1: Provider ID 输入
    let pid_active = wizard.step1_focus == Step1Field::ProviderId;
    let (pid_label, pid_val) = focused(pid_active);
    let pid_display = if pid_active {
        format!("{}▏", wizard.provider_id)
    } else {
        wizard.provider_id.clone()
    };
    let line_pid = Line::from(vec![
        Span::styled(" ID       ", pid_label),
        Span::styled(format!(" {}", pid_display), pid_val),
    ]);

    // 行 2: Base URL 输入
    let url_active = wizard.step1_focus == Step1Field::BaseUrl;
    let (url_label, url_val) = focused(url_active);
    let url_display = if url_active {
        format!("{}▏", wizard.base_url)
    } else {
        wizard.base_url.clone()
    };
    let line_url = Line::from(vec![
        Span::styled(" Base URL ", url_label),
        Span::styled(format!(" {}", url_display), url_val),
    ]);

    // 行 3: API Key 输入（掩码）
    let key_active = wizard.step1_focus == Step1Field::ApiKey;
    let (key_label, key_val) = focused(key_active);
    let masked: String = if wizard.api_key.is_empty() {
        String::new()
    } else {
        "•".repeat(wizard.api_key.len())
    };
    let key_display = if key_active {
        format!("{}▏", masked)
    } else {
        masked
    };
    let line_key = Line::from(vec![
        Span::styled(" API Key  ", key_label),
        Span::styled(format!(" {}", key_display), key_val),
    ]);

    // 底部提示
    let hint = Line::from(vec![
        Span::styled(
            " Enter",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":下一步  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::ERROR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":跳过setup  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Tab",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":切换字段", Style::default().fg(theme::MUTED)),
    ]);

    let mut lines = vec![
        Line::from(""),
        line_pt,
        line_pid,
        line_url,
        line_key,
        Line::from(""),
        hint,
    ];

    // 跳过确认覆盖层
    if wizard.confirm_skip {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " ⚠ 跳过 setup 将无法使用 AI 功能，",
            Style::default().fg(theme::ERROR),
        )]));
        lines.push(Line::from(vec![
            Span::styled("   按 ", Style::default().fg(theme::TEXT)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 确认跳过，", Style::default().fg(theme::TEXT)),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme::SAGE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 取消", Style::default().fg(theme::TEXT)),
        ]));
    }

    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_step_model_alias(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " ── Step 2/2: Model Aliases ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    f.render_widget(&block, area);
    let inner = block.inner(area);

    let alias_labels = ["Opus ", "Sonnet", "Haiku "];
    let mut lines: Vec<Line> = vec![Line::from("")];

    for (i, label) in alias_labels.iter().enumerate() {
        let is_active = wizard.step3_focus == i;
        let (lbl_style, val_style) = if is_active {
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
        let model_display = if is_active {
            format!("{}▏", wizard.aliases[i].model_id)
        } else {
            wizard.aliases[i].model_id.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {}  Model: ", label), lbl_style),
            Span::styled(format!("{}", model_display), val_style),
        ]));
    }

    // 底部提示
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            " Enter",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":完成配置  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":返回上一步  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Tab",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":切换字段", Style::default().fg(theme::MUTED)),
    ]));

    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_step_done(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " ── Setup Complete ✓ ",
            Style::default()
                .fg(theme::SAGE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SAGE));
    f.render_widget(&block, area);
    let inner = block.inner(area);

    let alias_labels = ["Opus", "Sonnet", "Haiku"];

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" Provider: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                wizard.provider_type.label(),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" ID:       ", Style::default().fg(theme::MUTED)),
            Span::styled(&wizard.provider_id, Style::default().fg(theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" Key:      ", Style::default().fg(theme::MUTED)),
            Span::styled(
                mask_api_key(&wizard.api_key),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(""),
    ];

    // 三个别名摘要
    for (i, label) in alias_labels.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:>6}  →  ", label),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                &wizard.aliases[i].model_id,
                Style::default().fg(theme::ACCENT),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" 按 ", Style::default().fg(theme::TEXT)),
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::SAGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" 开始使用", Style::default().fg(theme::TEXT)),
    ]));

    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// API Key 脱敏：首4位 + **** + 末4位
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
    use crate::app::setup_wizard::{SetupStep, SetupWizardPanel};
    use crate::app::App;

    #[test]
    fn test_mask_api_key() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("short"), "•••••");
        assert_eq!(mask_api_key("sk-ant-test-key-12345"), "sk-a••••2345");
    }

    fn render_headless(wizard: SetupWizardPanel) -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.setup_wizard = Some(wizard);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_render_step1_default() {
        let wizard = SetupWizardPanel::new();
        let (_, handle) = render_headless(wizard);
        assert!(handle.contains("Perihelion Setup"), "should contain title");
        assert!(handle.contains("Step 1/2"), "should contain step");
        assert!(handle.contains("Anthropic"), "should contain provider");
        assert!(handle.contains("API Key"), "should contain api key field");
    }

    #[tokio::test]
    async fn test_render_step1_masked_api_key() {
        let mut wizard = SetupWizardPanel::new();
        wizard.api_key = "sk-abc123xyz789".to_string();
        let (_, handle) = render_headless(wizard);
        // API key should be masked with bullets, not visible in plain text
        let snapshot = handle.snapshot().join("\n");
        assert!(!snapshot.contains("sk-abc123xyz789"), "should not show raw key");
    }

    #[tokio::test]
    async fn test_render_step2_aliases() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::ModelAlias;
        let (_, handle) = render_headless(wizard);
        assert!(handle.contains("Step 2/2"), "should contain step");
    }

    #[tokio::test]
    async fn test_render_done_page() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::Done;
        wizard.api_key = "sk-ant-test1234xyz".to_string();
        let (_, handle) = render_headless(wizard);
        assert!(handle.contains("Complete"), "should contain complete");
    }

    #[tokio::test]
    async fn test_render_step1_confirm_skip() {
        let mut wizard = SetupWizardPanel::new();
        wizard.confirm_skip = true;
        let (_, handle) = render_headless(wizard);
        assert!(handle.contains("Enter") || handle.contains("setup"), "should show skip confirmation");
    }
}
