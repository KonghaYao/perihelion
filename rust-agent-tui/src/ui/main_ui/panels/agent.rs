use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::ui::theme;

/// /agents 面板渲染（底部展开区）
pub(crate) fn render_agent_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(panel) = &app.agent_panel else { return };

    let agent_count = panel.agents.len();
    let popup_area = area;

    f.render_widget(Clear, popup_area);

    let title = if agent_count == 0 {
        " 🤖 Agent 选择 (无) "
    } else {
        " 🤖 Agent 选择 "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 第 0 项：取消选择（无 agent）
    let is_none_cursor = panel.cursor == 0;
    let is_none_selected = panel.selected_id.is_none();
    lines.push(Line::from(vec![
        Span::styled(
            if is_none_cursor { "▶ " } else { "  " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            "○ 无 Agent（默认）",
            if is_none_cursor {
                Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else if is_none_selected {
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::MUTED)
            },
        ),
    ]));
    lines.push(Line::from("")); // 空行间隔

    // Agent 列表
    for (i, agent) in panel.agents.iter().enumerate() {
        let cursor_idx = i + 1; // +1 因为第 0 项是"无 Agent"
        let is_cursor = panel.cursor == cursor_idx;
        let is_selected = panel.selected_id.as_ref() == Some(&agent.id);

        let bullet = if is_selected { "●" } else { "○" };
        let cursor_char = if is_cursor { "▶" } else { " " };

        let name_style = if is_cursor {
            Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} {}", cursor_char, bullet), name_style),
            Span::styled(format!(" {}", agent.name), name_style),
        ]));

        // 描述行（次要信息）
        if !agent.description.is_empty() {
            let desc_style = if is_cursor {
                Style::default().fg(theme::MUTED).bg(theme::ACCENT)
            } else {
                Style::default().fg(theme::MUTED)
            };
            // 截断过长的描述
            let desc: String = agent.description.chars().take(50).collect();
            let desc = if agent.description.len() > 50 { format!("{}…", desc) } else { desc };
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(desc, desc_style),
            ]));
        } else {
            lines.push(Line::from(""));
        }
    }

    // 底部提示
    lines.push(Line::from(vec![
        Span::styled(" Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":选择  ", Style::default().fg(theme::MUTED)),
        Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":关闭", Style::default().fg(theme::MUTED)),
    ]));

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((panel.scroll_offset, 0)),
        inner,
    );
}

