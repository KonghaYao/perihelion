use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::message_view::{ContentBlockView, MessageViewModel};

/// 将单个 ViewModel 渲染为 Vec<Line>
pub fn render_view_model(vm: &MessageViewModel, _width: usize) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble { rendered, .. } => {
            let mut lines = vec![Line::from(vec![
                Span::styled("▶ 你  ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ])];
            for line in rendered.lines.iter() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(line.to_string(), Style::default().fg(Color::White)),
                ]));
            }
            lines
        }
        MessageViewModel::AssistantBubble { blocks, is_streaming } => {
            let streaming_suffix = if *is_streaming { "…" } else { "" };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("◆ Agent  {}", streaming_suffix),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ])];
            for block in blocks {
                match block {
                    ContentBlockView::Text { rendered, .. } => {
                        for line in rendered.lines.iter() {
                            lines.push(Line::from(vec![
                                Span::raw("  "),
                                Span::styled(line.to_string(), Style::default().fg(Color::White)),
                            ]));
                        }
                    }
                    ContentBlockView::Reasoning { char_count } => {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("💭 思考 ({} chars)", char_count),
                                Style::default().fg(Color::Rgb(150, 120, 200)),
                            ),
                        ]));
                    }
                    ContentBlockView::ToolUse { name, input_preview } => {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("🔧 {}: {}", name, input_preview),
                                Style::default().fg(Color::Rgb(100, 181, 246)),
                            ),
                        ]));
                    }
                }
            }
            lines
        }
        MessageViewModel::ToolBlock {
            collapsed,
            display_name,
            content,
            color,
            is_error,
            ..
        } => {
            let icon = if *is_error { "✗" } else { "⚙" };
            let arrow = if *collapsed { "▸" } else { "▾" };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{} {} {}", icon, display_name, arrow),
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ),
            ])];
            if !collapsed {
                for line in content.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  │ "),
                        Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
            lines
        }
        MessageViewModel::SystemNote { content } => {
            let mut lines = Vec::new();
            for line in content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("ℹ ", Style::default().fg(Color::Blue)),
                    Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                ]));
            }
            lines
        }
        MessageViewModel::TodoStatus { rendered } => {
            rendered
                .lines()
                .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::White))))
                .collect()
        }
    }
}
