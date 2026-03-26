use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::relay_panel::{RelayEditField, RelayPanelMode};

/// /relay 面板渲染
pub(crate) fn render_relay_panel(f: &mut Frame, app: &crate::app::App) {
    let Some(panel) = &app.relay_panel else { return };

    let area = f.area();
    let popup_width = (area.width * 3 / 5).max(50).min(area.width.saturating_sub(4));
    let popup_height = 12u16.min(area.height * 3 / 5).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let (border_color, title) = match &panel.mode {
        RelayPanelMode::View => (Color::Cyan, " 远程控制配置 "),
        RelayPanelMode::Edit => (Color::Yellow, " 远程控制配置 (编辑) "),
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
        Span::styled(" URL:    ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if panel.buf_url.is_empty() {
                "(未设置)".to_string()
            } else {
                panel.buf_url.clone()
            },
            Style::default().fg(Color::White),
        ),
    ]));

    // Token（脱敏）
    lines.push(Line::from(vec![
        Span::styled(" Token:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(panel.display_token(), Style::default().fg(Color::White)),
    ]));

    // Name
    lines.push(Line::from(vec![
        Span::styled(" Name:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if panel.buf_name.is_empty() {
                "(未设置)".to_string()
            } else {
                panel.buf_name.clone()
            },
            Style::default().fg(Color::White),
        ),
    ]));

    // 状态消息
    if let Some(msg) = &panel.status_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(msg, Style::default().fg(Color::Green)),
        ]));
    }

    // 操作提示
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[e] 编辑", Style::default().fg(Color::Yellow)),
        Span::styled("  ", Style::default()),
        Span::styled("[Esc] 关闭", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(Text::from(lines));
    f.render_widget(paragraph, inner);
}

fn render_relay_edit(f: &mut Frame, panel: &crate::app::RelayPanel, inner: Rect) {
    let mut lines = Vec::new();

    // URL
    let url_focused = panel.edit_field == RelayEditField::Url;
    lines.push(Line::from(vec![
        Span::styled(" URL:    ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_input_field(&panel.buf_url, url_focused, panel.cursor),
            Style::default().fg(if url_focused { Color::Yellow } else { Color::White }),
        ),
    ]));

    // Token
    let token_focused = panel.edit_field == RelayEditField::Token;
    lines.push(Line::from(vec![
        Span::styled(" Token:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_input_field(&panel.buf_token, token_focused, panel.cursor),
            Style::default().fg(if token_focused { Color::Yellow } else { Color::White }),
        ),
    ]));

    // Name
    let name_focused = panel.edit_field == RelayEditField::Name;
    lines.push(Line::from(vec![
        Span::styled(" Name:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_input_field(&panel.buf_name, name_focused, panel.cursor),
            Style::default().fg(if name_focused { Color::Yellow } else { Color::White }),
        ),
    ]));

    // 错误消息
    if let Some(msg) = &panel.status_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(msg, Style::default().fg(Color::Red)),
        ]));
    }

    // 操作提示
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[Tab] 切换字段", Style::default().fg(Color::Cyan)),
        Span::styled("  ", Style::default()),
        Span::styled("[Enter] 保存", Style::default().fg(Color::Green)),
        Span::styled("  ", Style::default()),
        Span::styled("[Esc] 取消", Style::default().fg(Color::DarkGray)),
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
