use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::message_view::{ContentBlockView, MessageViewModel, ToolCategory};
use super::theme;

/// 将单个 ViewModel 渲染为 Vec<Line>
pub fn render_view_model(
    vm: &MessageViewModel,
    _index: Option<usize>,
    _width: usize,
) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble { rendered, .. } => {
            let user_bg: Color = Color::Rgb(74, 70, 66);
            let mut lines = Vec::with_capacity(rendered.lines.len() + 1);
            for (i, line) in rendered.lines.iter().enumerate() {
                if i == 0 {
                    // 第一行：用户消息用 ❯ 前缀，带底色
                    let mut spans = vec![Span::styled(
                        "❯ ",
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD)
                            .bg(user_bg),
                    )];
                    for span in &line.spans {
                        spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                    }
                    lines.push(Line::from(spans));
                } else {
                    // 后续行：填充 + 原始 spans，带底色
                    let mut spans = vec![Span::styled("  ", Style::default().bg(user_bg))];
                    for span in &line.spans {
                        spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                    }
                    lines.push(Line::from(spans));
                }
            }
            lines
        }
        MessageViewModel::AssistantBubble { blocks, .. } => {
            let mut lines = Vec::new();
            let mut first_text_merged = false;

            for block in blocks {
                match block {
                    ContentBlockView::Text { rendered, raw, .. } => {
                        // 检测是否为 diff 内容，如果是则用 diff 着色覆盖
                        let is_diff =
                            perihelion_widgets::message_block::highlight::is_diff_content(raw);
                        for line in rendered.lines.iter() {
                            if !first_text_merged {
                                // 第一行文本合并到标题行，保留 markdown 样式 spans
                                let mut spans = vec![
                                    Span::styled(format!("●"), Style::default().fg(Color::White)),
                                    Span::raw(" "),
                                ];
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
                        // diff 内容着色覆盖：如果检测到 diff，重新渲染带颜色的行
                        if is_diff && !lines.is_empty() {
                            let diff_lines: Vec<Line<'static>> = raw.lines()
                                .map(|l| {
                                    let diff_spans = perihelion_widgets::message_block::highlight::highlight_diff_line(l);
                                    let mut spans = vec![Span::raw("  ")];
                                    spans.extend(diff_spans);
                                    Line::from(spans)
                                })
                                .collect();
                            if !diff_lines.is_empty() {
                                lines = diff_lines;
                            }
                        }
                    }
                    ContentBlockView::Reasoning { .. } => {
                        // 跳过思考内容渲染，不设置 first_text_merged
                    }
                    ContentBlockView::ToolUse { .. } => {
                        // 跳过 ToolUse 渲染（Task 2：AI 消息不再显示工具调用行）
                        if !first_text_merged {
                            first_text_merged = true;
                        }
                    }
                }
            }

            // 如果没有正文内容（仅有 Reasoning/ToolUse），不渲染任何行
            // 正常情况下有文本时会由 first_text_merged 创建首行

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
            // 使用 ToolCallState 构建渲染状态
            let status = if *is_error {
                perihelion_widgets::ToolCallStatus::Failed
            } else if content.is_empty() {
                perihelion_widgets::ToolCallStatus::Running
            } else {
                perihelion_widgets::ToolCallStatus::Completed
            };

            let mut state = perihelion_widgets::ToolCallState::new(display_name.clone(), *color);
            state.status = status;
            state.collapsed = *collapsed;
            state.is_error = *is_error;
            if let Some(args) = args_display {
                state.args_summary = args.clone();
            }
            if !content.is_empty() {
                state.set_result(content.clone());
            }

            // 运行中状态：● 闪烁
            let indicator = if matches!(state.status, perihelion_widgets::ToolCallStatus::Running) {
                let tick = std::time::Instant::now()
                    .elapsed()
                    .as_millis() as u64
                    / 200;
                perihelion_widgets::tool_call::display::format_indicator(
                    state.status.clone(),
                    tick,
                )
            } else {
                perihelion_widgets::tool_call::display::format_indicator(
                    state.status.clone(),
                    state.tick,
                )
            };
            let indicator_color = match state.status {
                perihelion_widgets::ToolCallStatus::Completed => Color::Green,
                _ => Color::White,
            };
            let mut header_spans = vec![
                Span::styled(indicator.to_string(), Style::default().fg(indicator_color)),
                Span::raw(" "),
                Span::styled(
                    state.tool_name.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if !state.args_summary.is_empty() {
                let summary = perihelion_widgets::tool_call::display::format_args_summary(
                    &state.args_summary,
                    40,
                );
                header_spans.push(Span::styled(
                    format!("({})", summary),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            let mut lines = vec![Line::from(header_spans)];
            if !state.collapsed && !state.result_lines.is_empty() {
                let result_color = if *is_error {
                    theme::ERROR
                } else {
                    theme::MUTED
                };
                let border_color = if *is_error {
                    theme::ERROR
                } else {
                    Color::DarkGray
                };
                for line in &state.result_lines {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ".to_string(), Style::default().fg(border_color)),
                        Span::styled(line.clone(), Style::default().fg(result_color)),
                    ]));
                }
                if let Some(_omitted) = state.omitted_lines {
                    // 省略提示已删除
                }
            }
            lines
        }
        MessageViewModel::SubAgentGroup {
            agent_id,
            task_preview,
            recent_messages,
            collapsed,
            ..
        } => {
            let agent_color = theme::SUB_AGENT;
            let mut lines: Vec<Line<'static>> = Vec::new();

            if *collapsed {
                // 折叠状态：两行显示
                lines.push(Line::from(vec![Span::styled(
                    format!("● {}", agent_id),
                    Style::default()
                        .fg(agent_color)
                        .add_modifier(Modifier::BOLD),
                )]));
                let task_label: String = task_preview.chars().take(50).collect();
                let suffix = if task_preview.chars().count() > 50 {
                    "…"
                } else {
                    ""
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}{}", task_label, suffix),
                    Style::default().fg(theme::MUTED),
                )]));
            } else {
                // 展开状态：名称 + 任务描述
                let task_label: String = task_preview.chars().take(50).collect();
                let suffix = if task_preview.chars().count() > 50 {
                    "…"
                } else {
                    ""
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("● {}", agent_id),
                    Style::default()
                        .fg(agent_color)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}{}", task_label, suffix),
                    Style::default().fg(theme::MUTED),
                )]));

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
            }

            lines
        }
        MessageViewModel::SystemNote { content } => {
            let mut lines = Vec::new();
            let is_error =
                content.contains("❌") || content.contains("失败") || content.contains("错误");
            let is_warn = content.contains("⚠") || content.contains("已中断");
            let (icon, icon_color, text_color) = if is_error {
                ("✗ ", theme::ERROR, theme::ERROR)
            } else if is_warn {
                ("⚠ ", theme::WARNING, theme::WARNING)
            } else {
                ("ℹ ", theme::SAGE, theme::MUTED)
            };
            for line in content.lines() {
                lines.push(Line::from(vec![
                    Span::styled(icon.to_string(), Style::default().fg(icon_color)),
                    Span::styled(line.to_string(), Style::default().fg(text_color)),
                ]));
            }
            lines
        }
        MessageViewModel::ToolCallGroup {
            tools, collapsed, ..
        } => {
            let count = tools.len();
            let mut lines = Vec::new();
            let summary = ToolCategory::summary_for_tools(tools);

            if *collapsed {
                // 折叠：单行摘要 + ▶ 展开提示
                lines.push(Line::from(vec![Span::styled(
                    format!("  ▶ {}", summary),
                    Style::default().fg(theme::MUTED),
                )]));
            } else {
                // 展开：标题 + 每个工具的参数
                let arrow = if count == 1 { " " } else { " " };
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}{}", arrow, summary),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )]));
                for entry in tools {
                    let mut detail = String::new();
                    if let Some(args) = &entry.args_display {
                        detail = args.clone();
                    }
                    if entry.is_error {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("│ {}", detail),
                                Style::default().fg(theme::ERROR),
                            ),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                            Span::styled(detail, Style::default().fg(theme::MUTED)),
                        ]));
                    }
                }
            }

            lines
        }
    }
}
