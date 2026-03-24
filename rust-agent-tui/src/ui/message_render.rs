use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::message_view::{ContentBlockView, MessageViewModel};

/// 将单个 ViewModel 渲染为 Vec<Line>
pub fn render_view_model(vm: &MessageViewModel, index: usize, _width: usize) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble { rendered, .. } => {
            let mut lines = Vec::new();
            let mut first_line = true;
            for line in rendered.lines.iter() {
                if first_line {
                    let mut spans = vec![Span::styled(
                        format!("{} ", index),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    )];
                    spans.extend(line.spans.iter().cloned());
                    lines.push(Line::from(spans));
                    first_line = false;
                } else {
                    let mut spans = vec![Span::raw("  ")];
                    spans.extend(line.spans.iter().cloned());
                    lines.push(Line::from(spans));
                }
            }
            lines
        }
        MessageViewModel::AssistantBubble {
            blocks,
            is_streaming,
            ..
        } => {
            let streaming_suffix = if *is_streaming { "…" } else { "" };
            let mut lines = Vec::new();
            let mut first_text_merged = false;
            let mut tool_idx = 0;

            for block in blocks {
                match block {
                    ContentBlockView::Text { rendered, .. } => {
                        for line in rendered.lines.iter() {
                            if !first_text_merged {
                                // 第一行文本合并到标题行，保留 markdown 样式 spans
                                let mut spans = vec![
                                    Span::styled(
                                        format!("{} {}", index, streaming_suffix),
                                        Style::default()
                                            .fg(Color::Cyan)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::raw(" "),
                                ];
                                spans.extend(line.spans.iter().cloned());
                                lines.push(Line::from(spans));
                                first_text_merged = true;
                            } else {
                                let mut spans = vec![Span::raw("  ")];
                                spans.extend(line.spans.iter().cloned());
                                lines.push(Line::from(spans));
                            }
                        }
                    }
                    ContentBlockView::Reasoning { char_count } => {
                        if !first_text_merged {
                            // 没有文本块，直接创建标题行
                            lines.push(Line::from(vec![Span::styled(
                                format!("{} {}", index, streaming_suffix),
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            )]));
                            first_text_merged = true;
                        }
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("💭 思考 ({} chars)", char_count),
                                Style::default().fg(Color::Rgb(150, 120, 200)),
                            ),
                        ]));
                    }
                    ContentBlockView::ToolUse {
                        name,
                        input_preview: _,
                    } => {
                        if !first_text_merged {
                            // 没有文本块，直接创建标题行
                            lines.push(Line::from(vec![Span::styled(
                                format!("{} {}", index, streaming_suffix),
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            )]));
                            first_text_merged = true;
                        }
                        tool_idx += 1;
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("{}{} {}", index, tool_idx, name),
                                Style::default().fg(Color::Rgb(100, 181, 246)),
                            ),
                        ]));
                    }
                }
            }

            // 如果没有任何 block，至少创建标题行
            if lines.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    format!("{} {}", index, streaming_suffix),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )]));
            }

            lines
        }
        MessageViewModel::ToolBlock {
            collapsed,
            display_name,
            args_display,
            content,
            color,
            is_error,
            ..
        } => {
            let arrow = if *collapsed { "▸" } else { "▾" };
            let icon = if *is_error { "✗ " } else { "" };
            let mut header_spans = vec![Span::styled(
                format!("{} {}{}{}", index, display_name, icon, arrow),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            )];
            if let Some(args) = args_display {
                header_spans.push(Span::styled(
                    format!("  {}", args),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            let mut lines = vec![Line::from(header_spans)];
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
    }
}
