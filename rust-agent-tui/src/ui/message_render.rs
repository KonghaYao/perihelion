use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::message_view::{ContentBlockView, MessageViewModel};
use super::theme;

/// 将单个 ViewModel 渲染为 Vec<Line>
/// `index`: Some(n) 表示外层消息，渲染时带序号前缀；None 表示 SubAgent 内部消息，不渲染序号
pub fn render_view_model(vm: &MessageViewModel, index: Option<usize>, _width: usize) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble { rendered, .. } => {
            let mut lines = Vec::with_capacity(rendered.lines.len() + 1);
            for (i, line) in rendered.lines.iter().enumerate() {
                if i == 0 {
                    // 第一行
                    let mut spans = if let Some(idx) = index {
                        vec![Span::styled(
                            format!("{} ", idx),
                            Style::default().fg(theme::MUTED).add_modifier(Modifier::BOLD),
                        )]
                    } else {
                        vec![Span::raw("  ")]
                    };
                    spans.extend(line.spans.clone());
                    lines.push(Line::from(spans));
                } else {
                    // 后续行：填充 + 原始 spans
                    let mut spans = vec![Span::raw("  ")];
                    spans.extend(line.spans.clone());
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

            for block in blocks {
                match block {
                    ContentBlockView::Text { rendered, .. } => {
                        for line in rendered.lines.iter() {
                            if !first_text_merged {
                                // 第一行文本合并到标题行，保留 markdown 样式 spans
                                let mut spans = if let Some(idx) = index {
                                    vec![
                                        Span::styled(
                                            format!("{} {}", idx, streaming_suffix),
                                            Style::default()
                                                .fg(theme::MUTED)
                                                .add_modifier(Modifier::BOLD),
                                        ),
                                        Span::raw(" "),
                                    ]
                                } else {
                                    vec![Span::raw("  ")]
                                };
                                spans.extend(line.spans.clone());
                                lines.push(Line::from(spans));
                                first_text_merged = true;
                            } else {
                                // 复用 spans Vec 内存，避免 iter().cloned() 的中间 Vec 分配
                                let mut spans = vec![Span::raw("  ")];
                                spans.extend(line.spans.clone());
                                lines.push(Line::from(spans));
                            }
                        }
                    }
                    ContentBlockView::Reasoning { char_count } => {
                        if !first_text_merged {
                            // 没有文本块，直接创建标题行
                            if let Some(idx) = index {
                                lines.push(Line::from(vec![Span::styled(
                                    format!("{} {}", idx, streaming_suffix),
                                    Style::default()
                                        .fg(theme::MUTED)
                                        .add_modifier(Modifier::BOLD),
                                )]));
                            }
                            first_text_merged = true;
                        }
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("💭 思考 ({} chars)", char_count),
                                Style::default().fg(theme::THINKING),
                            ),
                        ]));
                    }
                    ContentBlockView::ToolUse { .. } => {
                        // 跳过 ToolUse 渲染（Task 2：AI 消息不再显示工具调用行）
                        if !first_text_merged {
                            first_text_merged = true;
                        }
                    }
                }
            }

            // 如果没有任何 block，至少创建标题行
            if lines.is_empty() {
                if let Some(idx) = index {
                    lines.push(Line::from(vec![Span::styled(
                        format!("{} {}", idx, streaming_suffix),
                        Style::default()
                            .fg(theme::MUTED)
                            .add_modifier(Modifier::BOLD),
                    )]));
                }
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
            let prefix = index
                .map(|idx| format!("{} ", idx))
                .unwrap_or_default();
            let mut header_spans = vec![Span::styled(
                format!("{}{}{}{}", prefix, display_name, icon, arrow),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            )];
            if let Some(args) = args_display {
                header_spans.push(Span::styled(
                    format!("  {}", args),
                    Style::default().fg(theme::MUTED),
                ));
            }
            let mut lines = vec![Line::from(header_spans)];
            if !collapsed {
                for line in content.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  │ "),
                        Span::styled(line.to_string(), Style::default().fg(theme::MUTED)),
                    ]));
                }
            }
            lines
        }
        MessageViewModel::SubAgentGroup {
            agent_id,
            task_preview,
            total_steps,
            recent_messages,
            is_running,
            collapsed,
            final_result,
        } => {
            let agent_color = theme::SUB_AGENT;
            let mut lines: Vec<Line<'static>> = Vec::new();

            if *collapsed {
                // 折叠状态：单行显示摘要
                let result_preview = final_result
                    .as_deref()
                    .map(|r| {
                        let preview: String = r.chars().take(50).collect();
                        if r.chars().count() > 50 {
                            format!("{}…", preview)
                        } else {
                            preview
                        }
                    })
                    .unwrap_or_default();
                let prefix = index
                    .map(|idx| format!("{} ", idx))
                    .unwrap_or_default();
                let mut spans = vec![
                    Span::styled(
                        format!("{}▸ 🤖 {}  「已完成 {} 步」", prefix, agent_id, total_steps),
                        Style::default().fg(agent_color).add_modifier(Modifier::BOLD),
                    ),
                ];
                if !result_preview.is_empty() {
                    spans.push(Span::styled(
                        format!("  {}", result_preview),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                lines.push(Line::from(spans));
            } else {
                // 展开状态：头行 + 嵌套消息 + 摘要
                let status_span = if *is_running {
                    Span::styled(
                        format!("[运行中 · 已执行 {} 步]", total_steps),
                        Style::default().fg(theme::WARNING),
                    )
                } else {
                    Span::styled(
                        format!("[已完成 {} 步]", total_steps),
                        Style::default().fg(agent_color),
                    )
                };
                let task_label: String = task_preview.chars().take(40).collect();
                let prefix = index
                    .map(|idx| format!("{} ", idx))
                    .unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}▾ 🤖 {}  「{}」  ", prefix, agent_id, task_label),
                        Style::default().fg(agent_color).add_modifier(Modifier::BOLD),
                    ),
                    status_span,
                ]));

                // 嵌套消息（不渲染序号）
                for inner_vm in recent_messages.iter() {
                    let inner_lines = render_view_model(inner_vm, None, _width);
                    for line in inner_lines {
                        // 每行前缀 2 空格缩进
                        let mut new_spans = vec![Span::raw("  ")];
                        new_spans.extend(line.spans.into_iter());
                        lines.push(Line::from(new_spans));
                    }
                }

                // 步数超过 4 时显示提示
                if *total_steps > 4 {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("[仅显示最近 4/{} 步]", total_steps),
                            Style::default().fg(theme::MUTED),
                        ),
                    ]));
                }

                // 完成后显示结果摘要
                if !is_running {
                    if let Some(result) = final_result {
                        let result_preview: String = result.chars().take(100).collect();
                        let suffix = if result.chars().count() > 100 { "…" } else { "" };
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("结果: {}{}", result_preview, suffix),
                                Style::default().fg(agent_color),
                            ),
                        ]));
                    }
                }
            }

            lines
        }
        MessageViewModel::SystemNote { content } => {
            let mut lines = Vec::new();
            for line in content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("ℹ ", Style::default().fg(theme::SAGE)),
                    Span::styled(line.to_string(), Style::default().fg(theme::MUTED)),
                ]));
            }
            lines
        }
    }
}
