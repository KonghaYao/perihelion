use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::model_panel::{AliasTab, ROW_HAIKU, ROW_LOGIN, ROW_OPUS, ROW_SONNET, ROW_THINKING};
use crate::app::App;
use crate::ui::theme;

pub(crate) fn render_model_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(panel) = &app.core.model_panel else { return };

    let inner = BorderedPanel::new(
        Span::styled(" /model — 模型选择 ", Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD))
    )
        .border_style(Style::default().fg(theme::ACCENT))
        .render(f, area);

    let active_alias = app.zen_config.as_ref()
        .map(|c| c.config.active_alias.as_str())
        .unwrap_or("opus");

    let models = app.zen_config.as_ref()
        .and_then(|c| c.config.providers.iter().find(|p| p.id == c.config.active_provider_id))
        .map(|p| &p.models);

    let mut lines: Vec<Line> = Vec::new();

    // Provider header
    if panel.provider_name.is_empty() {
        lines.push(Line::from(Span::styled(
            "  未配置 Provider",
            Style::default().fg(theme::WARNING),
        )));
        lines.push(Line::from(Span::styled(
            "  请选择下方 /login 或输入 /login 命令配置",
            Style::default().fg(theme::MUTED),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Provider: ", Style::default().fg(theme::MUTED)),
            Span::styled(panel.provider_name.clone(), Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)),
        ]));
    }
    lines.push(Line::from(""));

    // Model rows: Opus / Sonnet / Haiku
    let rows: [(usize, &AliasTab, &str); 3] = [
        (ROW_OPUS, &AliasTab::Opus, "Opus"),
        (ROW_SONNET, &AliasTab::Sonnet, "Sonnet"),
        (ROW_HAIKU, &AliasTab::Haiku, "Haiku"),
    ];

    for (row_idx, alias, label) in &rows {
        let is_active = alias.to_key() == active_alias;
        let is_cursor = panel.cursor == *row_idx;
        let model_name = models
            .and_then(|m| m.get_model(alias.to_key()))
            .unwrap_or("");

        let bullet = if is_active { "●" } else { "○" };
        let cursor_char = if is_cursor { "▶" } else { " " };

        let row_style = if is_cursor {
            Style::default().fg(Color::White).bg(theme::ACCENT)
        } else if is_active {
            Style::default().fg(theme::ACCENT)
        } else {
            Style::default().fg(theme::TEXT)
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} {} ", cursor_char, bullet), row_style),
            Span::styled(format!("{:8} ", label), row_style.add_modifier(Modifier::BOLD)),
            Span::styled(
                model_name.to_string(),
                row_style.fg(if is_cursor { Color::White } else { theme::MUTED }),
            ),
        ]));
    }

    // Thinking row
    {
        let is_cursor = panel.cursor == ROW_THINKING;
        let enabled_tag = if panel.buf_thinking_enabled { "[ON] " } else { "[OFF]" };
        let budget_display = if is_cursor {
            format!("{}█", panel.buf_thinking_budget)
        } else {
            panel.buf_thinking_budget.clone()
        };
        let enabled_color = if panel.buf_thinking_enabled { theme::THINKING } else { theme::MUTED };
        let row_style = if is_cursor {
            Style::default().fg(Color::White).bg(theme::ACCENT)
        } else {
            Style::default().fg(theme::TEXT)
        };

        lines.push(Line::from(vec![
            Span::styled(
                if is_cursor { " ▶   " } else { "     " },
                row_style,
            ),
            Span::styled("Thinking ", row_style.add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{} ", enabled_tag),
                if is_cursor {
                    Style::default().fg(enabled_color).bg(theme::ACCENT)
                } else {
                    Style::default().fg(enabled_color)
                },
            ),
            Span::styled(
                format!("budget: {}", budget_display),
                if is_cursor {
                    Style::default().fg(Color::White).bg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::TEXT)
                },
            ),
        ]));
    }

    // /login row
    {
        let is_cursor = panel.cursor == ROW_LOGIN;
        let row_style = if is_cursor {
            Style::default().fg(theme::WARNING).bg(theme::ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::WARNING)
        };

        lines.push(Line::from(vec![
            Span::styled(
                if is_cursor { " ▶   " } else { "     " },
                if is_cursor { row_style } else { Style::default().fg(theme::MUTED) },
            ),
            Span::styled("/login", row_style),
            Span::styled(
                "  管理 Provider…",
                if is_cursor {
                    Style::default().fg(Color::White).bg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::MUTED)
                },
            ),
        ]));
    }

    // Help line
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" ↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":导航  ", Style::default().fg(theme::MUTED)),
        Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":确认  ", Style::default().fg(theme::MUTED)),
        Span::styled("Space", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":Thinking开关  ", Style::default().fg(theme::MUTED)),
        Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":关闭", Style::default().fg(theme::MUTED)),
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
            buf_thinking_enabled: false,
            buf_thinking_budget: String::new(),
        });
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_model_no_provider_shows_guide() {
        let (_, handle) = render_headless_model_no_provider();
        let snap = handle.snapshot().join("\n");
        // 无 Provider 时应显示 /login 引导
        assert!(snap.contains("login"), "无 Provider 应显示 login 引导，实际:\n{}", snap);
    }
}
