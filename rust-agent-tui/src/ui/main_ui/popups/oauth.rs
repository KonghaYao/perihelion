use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::Frame;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{edit_display_parts, App};
use crate::ui::theme;

pub(crate) fn render_oauth_popup(f: &mut Frame, app: &mut App, area: Rect) {
    let prompt = match app.oauth_prompt.as_ref() {
        Some(p) => p,
        None => return,
    };

    let inner = area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });

    let title = format!(" OAuth 授权 — {} ", prompt.server_name);
    let title_span = Span::styled(
        title,
        ratatui::style::Style::default()
            .fg(theme::THINKING)
            .add_modifier(ratatui::style::Modifier::BOLD),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(theme::BORDER))
        .title(title_span);

    f.render_widget(block, area);

    let mut lines: Vec<ratatui::text::Line> = Vec::new();

    // 提示行
    lines.push(ratatui::text::Line::from(vec![Span::styled(
        "按 Ctrl+O 在浏览器中打开链接，完成后粘贴回调 URL：",
        ratatui::style::Style::default().fg(theme::TEXT),
    )]));

    // URL 行 — 单行，背景高亮
    let url_style = ratatui::style::Style::default()
        .fg(theme::SAGE)
        .bg(ratatui::style::Color::DarkGray);
    lines.push(ratatui::text::Line::from(vec![Span::styled(
        prompt.authorization_url.clone(),
        url_style,
    )]));

    // 空行
    lines.push(ratatui::text::Line::from(""));

    // 输入行
    let (before_cursor, after_cursor) = edit_display_parts(&prompt.input, prompt.cursor);
    lines.push(ratatui::text::Line::from(vec![
        Span::styled("回调 URL > ", ratatui::style::Style::default().fg(theme::MUTED)),
        Span::raw(before_cursor),
        Span::styled("█", ratatui::style::Style::default().fg(theme::TEXT)),
        Span::raw(after_cursor),
    ]));

    // 错误行
    if let Some(ref err) = prompt.error_message {
        lines.push(ratatui::text::Line::from(vec![Span::styled(
            err,
            ratatui::style::Style::default().fg(theme::ERROR),
        )]));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_render_oauth_popup_shows_url() {
        let (mut app, mut handle) = crate::app::App::new_headless(80, 30);
        let (tx, _rx) = tokio::sync::oneshot::channel();
        app.oauth_prompt = Some(crate::app::OAuthPrompt::new(
            "test-server".into(),
            "http://auth.example.com/authorize".into(),
            tx,
        ));
        handle
            .terminal
            .draw(|f| render_oauth_popup(f, &mut app, ratatui::layout::Rect::new(0, 0, 80, 9)))
            .unwrap();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("example.com"),
            "OAuth popup should show authorization URL domain"
        );
    }

    #[tokio::test]
    async fn test_render_oauth_popup_shows_error() {
        let (mut app, mut handle) = crate::app::App::new_headless(80, 30);
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut prompt = crate::app::OAuthPrompt::new(
            "srv".into(),
            "http://auth.example.com".into(),
            tx,
        );
        prompt.error_message = Some("parse error".to_string());
        app.oauth_prompt = Some(prompt);
        handle
            .terminal
            .draw(|f| render_oauth_popup(f, &mut app, ratatui::layout::Rect::new(0, 0, 80, 9)))
            .unwrap();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("parse error"),
            "OAuth popup should show error message"
        );
    }
}
