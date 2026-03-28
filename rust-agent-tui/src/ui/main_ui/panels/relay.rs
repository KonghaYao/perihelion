use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::relay_panel::{RelayEditField, RelayPanelMode};
use crate::ui::theme;

/// /relay 面板渲染（底部展开区）
pub(crate) fn render_relay_panel(f: &mut Frame, app: &crate::app::App, area: Rect) {
    let Some(panel) = &app.relay_panel else { return };

    let popup_area = area;
    f.render_widget(Clear, popup_area);

    let (border_color, title) = match &panel.mode {
        RelayPanelMode::View => (theme::ACCENT, " 远程控制配置 "),
        RelayPanelMode::Edit => (theme::WARNING, " 远程控制配置 (编辑) "),
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    match &panel.mode {
        RelayPanelMode::View => {
            render_relay_view(f, panel, inner);
        }
        RelayPanelMode::Edit => {
            render_relay_edit(f, panel, inner);
        }
    }
}

fn render_relay_view(f: &mut Frame, panel: &crate::app::RelayPanel, inner: Rect) {
    let mut lines = Vec::new();

    // URL
    lines.push(Line::from(vec![
        Span::styled(" URL:    ", Style::default().fg(theme::MUTED)),
        Span::styled(
            if panel.buf_url.is_empty() {
                "(未设置)".to_string()
            } else {
                panel.buf_url.clone()
            },
            Style::default().fg(theme::TEXT),
        ),
    ]));

    // Token（脱敏）
    lines.push(Line::from(vec![
        Span::styled(" Token:  ", Style::default().fg(theme::MUTED)),
        Span::styled(panel.display_token(), Style::default().fg(theme::TEXT)),
    ]));

    // Name
    lines.push(Line::from(vec![
        Span::styled(" Name:   ", Style::default().fg(theme::MUTED)),
        Span::styled(
            if panel.buf_name.is_empty() {
                "(未设置)".to_string()
            } else {
                panel.buf_name.clone()
            },
            Style::default().fg(theme::TEXT),
        ),
    ]));

    // Web 接入 URL
    if let Some(url) = &panel.web_access_url {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" Web URL: ", Style::default().fg(theme::MUTED)),
            Span::styled(url, Style::default().fg(theme::ACCENT)),
        ]));
    }

    // 状态消息
    if let Some(msg) = &panel.status_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(msg, Style::default().fg(theme::SAGE)),
        ]));
    }

    // 操作提示
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[e] 编辑", Style::default().fg(theme::WARNING)),
        Span::styled("  ", Style::default()),
        Span::styled("[Esc] 关闭", Style::default().fg(theme::MUTED)),
    ]));

    let paragraph = Paragraph::new(Text::from(lines));
    f.render_widget(paragraph, inner);
}

fn render_relay_edit(f: &mut Frame, panel: &crate::app::RelayPanel, inner: Rect) {
    let mut lines = Vec::new();

    // URL
    let url_focused = panel.edit_field == RelayEditField::Url;
    lines.push(Line::from(vec![
        Span::styled(" URL:    ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format_input_field(&panel.buf_url, url_focused, panel.cursor),
            Style::default().fg(if url_focused { theme::WARNING } else { theme::TEXT }),
        ),
    ]));

    // Token
    let token_focused = panel.edit_field == RelayEditField::Token;
    lines.push(Line::from(vec![
        Span::styled(" Token:  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format_input_field(&panel.buf_token, token_focused, panel.cursor),
            Style::default().fg(if token_focused { theme::WARNING } else { theme::TEXT }),
        ),
    ]));

    // Name
    let name_focused = panel.edit_field == RelayEditField::Name;
    lines.push(Line::from(vec![
        Span::styled(" Name:   ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format_input_field(&panel.buf_name, name_focused, panel.cursor),
            Style::default().fg(if name_focused { theme::WARNING } else { theme::TEXT }),
        ),
    ]));

    // 错误消息
    if let Some(msg) = &panel.status_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(msg, Style::default().fg(theme::ERROR)),
        ]));
    }

    // 操作提示
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[Tab] 切换字段", Style::default().fg(theme::ACCENT)),
        Span::styled("  ", Style::default()),
        Span::styled("[Enter] 保存", Style::default().fg(theme::SAGE)),
        Span::styled("  ", Style::default()),
        Span::styled("[Esc] 取消", Style::default().fg(theme::MUTED)),
    ]));

    let paragraph = Paragraph::new(Text::from(lines));
    f.render_widget(paragraph, inner);
}

/// 格式化输入字段（带光标）
fn format_input_field(text: &str, focused: bool, cursor: usize) -> String {
    if text.is_empty() {
        return if focused { "▏".to_string() } else { "".to_string() };
    }

    if focused && cursor <= text.len() {
        let mut chars: Vec<char> = text.chars().collect();
        // 确保 cursor 在有效范围内
        let cursor = cursor.min(chars.len());
        chars.insert(cursor, '▏');
        chars.into_iter().collect()
    } else {
        text.chars().collect()
    }
}
