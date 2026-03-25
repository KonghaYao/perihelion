use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;

/// /agents 面板渲染
pub(crate) fn render_agent_panel(f: &mut Frame, app: &App) {
    let Some(panel) = &app.agent_panel else { return };

    let area = f.area();
    let agent_count = panel.agents.len();
    let _total_items = panel.total();

    // 弹窗高度：边框(2) + 标题(1) + 空行(1) + 每项(2行或1行) + 间隔 + 底部提示(1)
    let base_height = 2 + 1 + 1 + 1 + 1; // 边框 + 标题 + 空行 + 空行 + 底部提示
    let items_height: u16 = panel.agents.iter().map(|a| {
        if a.description.is_empty() { 1 } else { 2 }
    }).sum::<u16>();
    let popup_height = (base_height + items_height).min(area.height * 4 / 5).min(area.height.saturating_sub(4)).max(6);
    let popup_width = (area.width * 3 / 4).max(50).min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let title = if agent_count == 0 {
        " 🤖 Agent 选择 (无) "
    } else {
        " 🤖 Agent 选择 "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 第 0 项：取消选择（无 agent）
    let is_none_cursor = panel.cursor == 0;
    let is_none_selected = panel.selected_id.is_none();
    lines.push(Line::from(vec![
        Span::styled(
            if is_none_cursor { "▶ " } else { "  " },
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            "○ 无 Agent（默认）",
            if is_none_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if is_none_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
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
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} {}", cursor_char, bullet), name_style),
            Span::styled(format!(" {}", agent.name), name_style),
        ]));

        // 描述行（次要信息）
        if !agent.description.is_empty() {
            let desc_style = if is_cursor {
                Style::default().fg(Color::DarkGray).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
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
        Span::styled(" Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(":选择  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(":关闭", Style::default().fg(Color::DarkGray)),
    ]));

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((panel.scroll_offset, 0)),
        inner,
    );
}

