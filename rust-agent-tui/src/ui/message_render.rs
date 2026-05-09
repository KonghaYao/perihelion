use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::message_view::{ContentBlockView, MessageViewModel, ToolCategory};
use super::theme;

/// Generate always-visible error summary lines (up to 400 Unicode chars).
/// 2-space indent, no vertical bar, no prefix. Preserves newlines (multi-line render).
fn error_summary_lines(content: &str) -> Vec<Line<'static>> {
    let truncated: String = content.chars().take(400).collect();
    truncated
        .lines()
        .map(|line| {
            Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                Span::styled(line.to_string(), Style::default().fg(theme::ERROR)),
            ])
        })
        .collect()
}

/// AskUserQuestion 专用渲染：`⏺ User answered Peri's questions:` + `⎿ · H → V`
fn render_ask_user_block(content: &str, is_error: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let color = if is_error { theme::ERROR } else { theme::SAGE };
    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(color)),
        Span::styled(
            "User answered Peri's questions:".to_string(),
            Style::default().fg(theme::TEXT),
        ),
    ]));

    if content.is_empty() {
        return lines;
    }

    // 解析多问题格式: [问: H]\n回答: V\n\n[问: H2]\n回答: V2
    for block in content.split("\n\n") {
        let mut header = String::new();
        let mut answer = String::new();
        for line in block.lines() {
            if let Some(h) = line.strip_prefix("[问: ").and_then(|s| s.strip_suffix(']')) {
                header = h.to_string();
            } else if let Some(a) = line.strip_prefix("回答: ") {
                answer = a.to_string();
            }
        }
        let text = if !header.is_empty() {
            format!("{} → {}", header, answer)
        } else if !answer.is_empty() {
            answer
        } else {
            // 单问题模式：整个 block 就是回答文本
            block.to_string()
        };
        if text.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
            Span::styled("· ", Style::default().fg(theme::DIM)),
            Span::styled(
                text,
                Style::default().fg(if is_error { theme::ERROR } else { theme::MUTED }),
            ),
        ]));
    }

    lines
}

/// 将单个 ViewModel 渲染为 Vec<Line>
pub fn render_view_model(
    vm: &MessageViewModel,
    _index: Option<usize>,
    _width: usize,
) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble { rendered, .. } => {
            let user_bg: Color = theme::USER_BG;
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
        MessageViewModel::AssistantBubble {
            blocks,
            is_streaming,
            ..
        } => {
            let mut lines = Vec::new();
            let mut first_text_merged = false;

            // 流式进行中：● 前缀闪烁动画（每 400ms 切换可见/暗淡）
            let indicator = if *is_streaming {
                let tick = std::time::Instant::now().elapsed().as_millis() as u64 / 400;
                let visible = tick % 2 == 0;
                Span::styled(
                    "●".to_string(),
                    Style::default().fg(if visible { theme::TEXT } else { theme::DIM }),
                )
            } else {
                Span::styled("●".to_string(), Style::default().fg(theme::TEXT))
            };

            for block in blocks {
                match block {
                    ContentBlockView::Text { rendered, raw, .. } => {
                        // 先检测 diff 内容，分支渲染避免双重渲染丢失前缀
                        let is_diff =
                            perihelion_widgets::message_block::highlight::is_diff_content(raw);
                        if is_diff {
                            // Diff 专用渲染路径：保留 ● 首行前缀
                            for (i, l) in raw.lines().enumerate() {
                                let diff_spans =
                                    perihelion_widgets::message_block::highlight::highlight_diff_line(l);
                                if i == 0 && !first_text_merged {
                                    let mut spans = vec![indicator.clone(), Span::raw(" ")];
                                    spans.extend(diff_spans);
                                    lines.push(Line::from(spans));
                                    first_text_merged = true;
                                } else {
                                    let mut spans = vec![Span::raw("  ")];
                                    spans.extend(diff_spans);
                                    lines.push(Line::from(spans));
                                }
                            }
                        } else {
                            // 正常 markdown 渲染路径
                            for line in rendered.lines.iter() {
                                if !first_text_merged {
                                    let mut spans = vec![indicator.clone(), Span::raw(" ")];
                                    spans.extend(line.spans.clone());
                                    lines.push(Line::from(spans));
                                    first_text_merged = true;
                                } else {
                                    let mut spans = vec![Span::raw("  ")];
                                    spans.extend(line.spans.clone());
                                    lines.push(Line::from(spans));
                                }
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
            color: _color,
            is_error,
            tool_name,
            ..
        } => {
            // AskUserQuestion 专用渲染路径
            if tool_name == "AskUserQuestion" {
                return render_ask_user_block(content, *is_error);
            }

            let is_running = content.is_empty() && !*is_error;

            // 构建状态（仅用于 result_lines 管理）
            let status = if *is_error {
                perihelion_widgets::ToolCallStatus::Failed
            } else if is_running {
                perihelion_widgets::ToolCallStatus::Running
            } else {
                perihelion_widgets::ToolCallStatus::Completed
            };

            let mut state =
                perihelion_widgets::ToolCallState::new(display_name.clone(), theme::TEXT);
            state.status = status;
            state.collapsed = *collapsed;
            state.is_error = *is_error;
            if let Some(args) = args_display {
                state.args_summary = args.clone();
            }
            if !content.is_empty() {
                state.set_result(content.clone());
            }

            let tool_color = if *is_error { theme::ERROR } else { theme::SAGE };

            // ⏺ 指示器：运行中闪烁，完成固定，失败 ✗
            let indicator = if is_running {
                let tick = std::time::Instant::now().elapsed().as_millis() as u64 / 200;
                if (tick / 4).is_multiple_of(2) {
                    "⏺"
                } else {
                    " "
                }
            } else if *is_error {
                "✗"
            } else {
                "⏺"
            };

            let mut header_spans = vec![
                Span::styled(indicator.to_string(), Style::default().fg(tool_color)),
                Span::raw(" "),
                Span::styled(
                    state.tool_name.clone(),
                    Style::default()
                        .fg(theme::TEXT)
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
                    Style::default().fg(theme::DIM),
                ));
            }
            let mut lines = vec![Line::from(header_spans)];
            if !state.collapsed && !state.result_lines.is_empty() {
                let result_color = if *is_error {
                    theme::ERROR
                } else {
                    theme::MUTED
                };
                let border_color = if *is_error { theme::ERROR } else { theme::DIM };
                for line in &state.result_lines {
                    lines.push(Line::from(vec![
                        Span::styled("  ⎿ ".to_string(), Style::default().fg(border_color)),
                        Span::styled(line.clone(), Style::default().fg(result_color)),
                    ]));
                }
            } else if *is_error && !content.is_empty() {
                lines.extend(error_summary_lines(content));
            }
            lines
        }
        MessageViewModel::SubAgentGroup {
            agent_id,
            task_preview,
            recent_messages,
            collapsed,
            is_error,
            final_result,
            ..
        } => {
            let agent_color = if *is_error {
                theme::ERROR
            } else {
                theme::SUB_AGENT
            };
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
                if *is_error {
                    if let Some(ref result) = final_result {
                        if !result.is_empty() {
                            lines.extend(error_summary_lines(result));
                        }
                    }
                }
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

                // 嵌套消息（不渲染序号），跳过无可见内容的条目
                let bg_style = Style::default().bg(theme::SUB_AGENT_BG);
                for inner_vm in recent_messages.iter() {
                    let inner_lines = render_view_model(inner_vm, None, _width);
                    if inner_lines.is_empty() {
                        continue;
                    }
                    for line in inner_lines {
                        // 每行前缀 2 空格缩进 + 背景色
                        let mut new_spans = vec![Span::styled("  ", bg_style)];
                        new_spans.extend(line.spans.into_iter().map(|s| s.patch_style(bg_style)));
                        lines.push(Line::from(new_spans));
                    }
                }

                // 子 agent 完成后，渲染 final_result（工具执行摘要 + 最终回复）
                if let Some(ref result) = final_result {
                    if !result.is_empty() {
                        // 空行分隔
                        lines.push(Line::from(vec![Span::raw("")]));
                        // 前缀缩进 + 分隔符
                        lines.push(Line::from(vec![Span::styled(
                            "  ── 执行结果 ──".to_string(),
                            Style::default().fg(theme::DIM).bg(theme::SUB_AGENT_BG),
                        )]));
                        // 逐行渲染 final_result（最多 20 行，过长截断）
                        let max_lines = 20;
                        for (i, line_text) in result.lines().take(max_lines).enumerate() {
                            if i == max_lines - 1 && result.lines().count() > max_lines {
                                let truncated: String =
                                    line_text.chars().take(80).collect::<String>() + "…";
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "  ⎿ ",
                                        Style::default().fg(theme::DIM).bg(theme::SUB_AGENT_BG),
                                    ),
                                    Span::styled(
                                        truncated,
                                        Style::default().fg(theme::MUTED).bg(theme::SUB_AGENT_BG),
                                    ),
                                ]));
                            } else {
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "  ⎿ ",
                                        Style::default().fg(theme::DIM).bg(theme::SUB_AGENT_BG),
                                    ),
                                    Span::styled(
                                        line_text.to_string(),
                                        Style::default().fg(theme::MUTED).bg(theme::SUB_AGENT_BG),
                                    ),
                                ]));
                            }
                        }
                    }
                }
            }

            lines
        }
        MessageViewModel::SystemNote { content } => {
            let mut lines = Vec::new();
            for line in content.lines() {
                let is_error =
                    line.contains("❌") || line.contains("失败") || line.contains("错误");
                let is_warn = line.contains("⚠") || line.contains("已中断");
                let text_color = if is_error {
                    theme::ERROR
                } else if is_warn {
                    theme::WARNING
                } else {
                    theme::MUTED
                };
                lines.push(Line::from(vec![
                    Span::styled("· ", Style::default().fg(theme::DIM)),
                    Span::styled(line.to_string(), Style::default().fg(text_color)),
                ]));
            }
            lines
        }
        MessageViewModel::CacheWarning { content } => {
            vec![Line::from(Span::styled(
                content.clone(),
                Style::default().fg(theme::WARNING),
            ))]
        }
        MessageViewModel::ToolCallGroup {
            category,
            tools,
            collapsed: _collapsed,
            ..
        } => {
            let mut lines = Vec::new();

            if *category == ToolCategory::AskUser {
                // AskUserQuestion 聚合：统一标题 + 所有问答对
                let has_error = tools.iter().any(|t| t.is_error);
                let color = if has_error { theme::ERROR } else { theme::SAGE };
                lines.push(Line::from(vec![
                    Span::styled("⏺ ", Style::default().fg(color)),
                    Span::styled(
                        "User answered Peri's questions:".to_string(),
                        Style::default().fg(theme::TEXT),
                    ),
                ]));

                for entry in tools {
                    let entry_color = if entry.is_error {
                        theme::ERROR
                    } else {
                        theme::MUTED
                    };
                    if entry.content.is_empty() {
                        continue;
                    }
                    // 解析每个工具结果中的问答对
                    for block in entry.content.split("\n\n") {
                        let mut header = String::new();
                        let mut answer = String::new();
                        for line in block.lines() {
                            if let Some(h) =
                                line.strip_prefix("[问: ").and_then(|s| s.strip_suffix(']'))
                            {
                                header = h.to_string();
                            } else if let Some(a) = line.strip_prefix("回答: ") {
                                answer = a.to_string();
                            }
                        }
                        let text = if !header.is_empty() {
                            format!("{} → {}", header, answer)
                        } else if !answer.is_empty() {
                            answer
                        } else {
                            // 单问题模式：整个 block 就是回答文本
                            block.to_string()
                        };
                        if text.is_empty() {
                            continue;
                        }
                        lines.push(Line::from(vec![
                            Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                            Span::styled("· ", Style::default().fg(theme::DIM)),
                            Span::styled(text, Style::default().fg(entry_color)),
                        ]));
                    }
                }
            } else {
                let summary = ToolCategory::summary_for_tools(tools);

                // 统一 ⏺ 前缀，仅显示汇总行
                lines.push(Line::from(vec![
                    Span::styled("⏺ ", Style::default().fg(theme::SAGE)),
                    Span::styled(summary, Style::default().fg(theme::MUTED)),
                ]));
                // 显示出错工具的错误摘要
                for entry in tools {
                    if entry.is_error && !entry.content.is_empty() {
                        lines.extend(error_summary_lines(&entry.content));
                    }
                }
            }

            lines
        }
    }
}
