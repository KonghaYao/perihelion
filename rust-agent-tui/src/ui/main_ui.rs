mod panels;
mod popups;
mod status_bar;
mod sticky_header;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::app::login_panel::LoginPanelMode;
use crate::app::App;
use crate::ui::theme;
use crate::ui::welcome;
use rust_agent_middlewares::prelude::TodoStatus;

pub fn render(f: &mut Frame, app: &mut App) {
    // Setup 向导：全屏覆盖，优先于所有正常界面
    if app.setup_wizard.is_some() {
        popups::setup_wizard::render_setup_wizard(f, app);
        return;
    }

    let area = f.area();

    // 动态输入框高度：行数 + 边框（上下各 1），最少 3 行，最多 40%
    let line_count = app.core.textarea.lines().len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);

    // 附件栏高度：无附件时为 0，有附件时固定 3 行
    let attachment_height: u16 = if app.core.pending_attachments.is_empty() {
        0
    } else {
        3
    };

    // 底部展开区高度（替代居中弹窗）
    let panel_height = active_panel_height(app, area.height);

    // Sticky header 高度：有消息时动态高度（1-3 行），无消息时 0
    let sticky_header_height: u16 = app
        .core
        .last_human_message
        .as_ref()
        .map(|msg| {
            let width = area.width.saturating_sub(2).max(1);
            let lines = sticky_header::estimate_header_lines(msg, width);
            lines as u16 // 1-3 行，无分隔线
        })
        .unwrap_or(0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(sticky_header_height), // [0] sticky header
            Constraint::Min(1),                       // [1] 聊天区（可滚动）
            Constraint::Length(attachment_height),    // [2] 附件栏（动态）
            Constraint::Length(panel_height),         // [3] 底部展开区（动态）
            Constraint::Length(input_height),         // [4] 输入框（动态）
            Constraint::Length(3),                    // [5] 状态栏（两行 + 空行缓冲）
        ])
        .split(area);

    render_messages(f, app, chunks[0], chunks[1]);
    render_attachment_bar(f, app, chunks[2]);

    // 底部展开区（HITL / AskUser / 配置面板）
    if panel_height > 0 {
        let panel_area = chunks[3];
        match &app.agent.interaction_prompt {
            Some(crate::app::InteractionPrompt::Approval(_)) => {
                popups::hitl::render_hitl_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Questions(_)) => {
                popups::ask_user::render_ask_user_popup(f, app, panel_area);
            }
            None => {}
        }
        if app.core.login_panel.is_some() {
            panels::login::render_login_panel(f, app, panel_area);
        }
        if app.core.model_panel.is_some() {
            panels::model::render_model_panel(f, app, panel_area);
        }
        if app.core.agent_panel.is_some() {
            panels::agent::render_agent_panel(f, app, panel_area);
        }
        if app.core.thread_browser.is_some() {
            panels::thread_browser::render_thread_browser(f, app, panel_area);
        }
        if app.cron.cron_panel.is_some() {
            panels::cron::render_cron_panel(f, app, panel_area);
        }
    }

    // 输入框前缀 ❯ + 文本区
    f.render_widget(&app.core.textarea, chunks[4]);
    app.core.textarea_area = Some(chunks[4]);

    // ❯ 前缀：渲染在输入框文字前面（叠加在 textarea 第一行起始位置）
    let prompt_x = chunks[4].x;
    let prompt_y = chunks[4].y + 1; // 跳过顶部边框行
    let prompt_area = Rect {
        x: prompt_x,
        y: prompt_y,
        width: 2,
        height: 1,
    };
    let prompt_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new("❯").style(prompt_style), prompt_area);
    status_bar::render_status_bar(f, app, chunks[5]);

    // 统一命令/Skills 提示条（浮动在输入框上方）
    popups::hints::render_unified_hint(f, app, chunks[4]);
}

/// 计算底部展开区所需高度（无激活面板时返回 0）
fn active_panel_height(app: &App, screen_height: u16) -> u16 {
    let max_h = screen_height * 3 / 5; // 最多占 60% 屏高
    let raw = if let Some(panel) = &app.core.thread_browser {
        (panel.total() as u16 + 4).max(6)
    } else if let Some(panel) = &app.core.login_panel {
        let n = panel.providers.len() as u16;
        match panel.mode {
            // Edit/New: 7 fields + 1 blank + 1 help + 2 borders = 11
            LoginPanelMode::Edit | LoginPanelMode::New => 11,
            // ConfirmDelete: n providers + 4 confirm area + 2 borders
            LoginPanelMode::ConfirmDelete => (n + 6).max(7),
            // Browse: n * 2 (name + models) + 2 help/blank + 2 borders
            LoginPanelMode::Browse => (n * 2 + 4).max(6),
        }
    } else if app.core.model_panel.is_some() {
        14
    } else if let Some(panel) = &app.core.agent_panel {
        (panel.agents.len() as u16 * 2 + 6).max(6)
    } else if app.cron.cron_panel.is_some() {
        (app.cron
            .cron_panel
            .as_ref()
            .map(|p| p.tasks.len())
            .unwrap_or(0) as u16
            + 4)
        .max(6)
    } else if let Some(crate::app::InteractionPrompt::Approval(p)) = &app.agent.interaction_prompt {
        (p.items.len() as u16 * 2 + 5).max(5)
    } else if let Some(crate::app::InteractionPrompt::Questions(p)) = &app.agent.interaction_prompt
    {
        let cur = &p.questions[p.active_tab];
        let opt_rows = cur.data.options.len() as u16;
        let desc_rows = cur
            .data
            .options
            .iter()
            .filter(|o| o.description.is_some())
            .count() as u16;
        (cur.data.question.lines().count() as u16 + opt_rows + desc_rows + 7).max(8)
    } else {
        0
    };
    raw.min(max_h)
}

fn render_messages(f: &mut Frame, app: &mut App, header_area: Rect, messages_area: Rect) {
    // Welcome Card 或消息列表
    if app.core.view_messages.is_empty() {
        welcome::render_welcome(f, app, messages_area);
        return;
    }

    let inner = messages_area;
    app.core.messages_area = Some(inner);
    let visible_height = inner.height;

    // 计算 loading spinner 行（Claude Code 风格：✻ verb (Xm Xs · ↓ X.Xk tokens)）
    // loading 结束后显示总结行：✻ Brewed for Xm Xs（橙色）
    let spinner_line: Option<Line<'static>> = if app.core.loading {
        let frame = perihelion_widgets::spinner::animation::tick_to_frame(app.spinner_state.tick());
        let verb = app.spinner_state.verb();
        let elapsed =
            perihelion_widgets::spinner::animation::format_elapsed(app.spinner_state.elapsed_ms());
        let tokens = app.spinner_state.displayed_tokens();

        let orange = Style::default().fg(Color::Rgb(255, 140, 0));
        let gray = Style::default().fg(theme::MUTED);
        let mut parts = vec![
            Span::styled(format!(" {} {}", frame, verb), orange),
            Span::styled(format!(" ({elapsed}"), gray),
        ];
        if tokens > 0 {
            let tokens_fmt = perihelion_widgets::spinner::animation::format_tokens(tokens);
            parts.push(Span::styled(format!(" · ↓ {tokens_fmt} tokens"), gray));
        }
        parts.push(Span::styled(")", gray));
        Some(Line::from(parts))
    } else if app.spinner_state.last_summary_elapsed_ms() > 0 {
        let elapsed = perihelion_widgets::spinner::animation::format_elapsed(
            app.spinner_state.last_summary_elapsed_ms(),
        );
        Some(Line::from(Span::styled(
            format!("  ✻  Brewed for {elapsed}"),
            Style::default().fg(theme::MUTED),
        )))
    } else {
        None
    };

    // 从 RenderCache 读取已渲染好的行（浅克隆 Vec 头，开销极小）
    let (mut all_lines, _total_lines, max_scroll, offset) = {
        let cache = app.core.render_cache.read();
        app.core.last_render_version = cache.version;

        // total_lines 已是 wrap 后的真实视觉行数（由渲染线程通过 Paragraph::line_count 计算）
        let total_lines = cache.total_lines;
        let spinner_extra: u16 = if spinner_line.is_some() {
            let base = 1; // spinner summary line
            if app.core.loading {
                base + 1 + app.todo_items.len() as u16 // tip + todo items
            } else {
                base
            }
        } else {
            0
        };
        let visual_total = (total_lines as u16).saturating_add(spinner_extra);
        let max_scroll = visual_total.saturating_sub(visible_height);
        let offset = if app.core.scroll_follow {
            max_scroll
        } else {
            let off = app.core.scroll_offset.min(max_scroll);
            // 用户手动滚到底部时，自动恢复吸底
            if off >= max_scroll {
                app.core.scroll_follow = true;
            }
            off
        };

        // Vec::clone() 是浅克隆，只复制指针+容量+长度头（3个 usize），不复制 Line 内容
        (cache.lines.clone(), total_lines, max_scroll, offset)
    };
    if let Some(line) = spinner_line {
        all_lines.push(line);
        // Tip + TODO 仅在活跃 loading 时显示
        if app.core.loading {
            let tip = crate::ui::tips::pick_tip(app.spinner_state.raw_tick());
            all_lines.push(Line::from(vec![
                Span::styled("  ⎿  Tip: ", Style::default().fg(theme::MUTED)),
                Span::styled(tip, Style::default().fg(theme::MUTED)),
            ]));
            all_lines.push(Line::from(""));
            for item in &app.todo_items {
                let (icon, style) = match item.status {
                    TodoStatus::InProgress => (
                        "  ✻  ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    TodoStatus::Completed => ("  ✓  ", Style::default().fg(Color::Green)),
                    TodoStatus::Pending => ("  ○  ", Style::default().fg(theme::MUTED)),
                };
                let hint = match item.status {
                    TodoStatus::Pending => Some("可开始"),
                    TodoStatus::InProgress => Some("可完成"),
                    TodoStatus::Completed => None,
                };
                let mut spans = vec![
                    Span::styled(icon, style),
                    Span::styled(&item.content, style),
                ];
                if let Some(hint) = hint {
                    spans.push(Span::styled(
                        format!(" ({hint})"),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                all_lines.push(Line::from(spans));
            }
        }
    }
    app.core.scroll_offset = offset;

    // 字符级选区高亮
    if app.core.text_selection.is_active() {
        let ts = &app.core.text_selection;
        if let (Some(start), Some(end)) = (ts.start, ts.end) {
            let cache = app.core.render_cache.read();
            let wrap_map = &cache.wrap_map;
            let usable_width = app
                .core
                .messages_area
                .map(|a| a.width.saturating_sub(1))
                .unwrap_or(0);

            // 映射为逻辑坐标
            let ((sr, sc), (er, ec)) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let logical_start =
                crate::app::text_selection::visual_to_logical(sr, sc, wrap_map, usable_width);
            let logical_end =
                crate::app::text_selection::visual_to_logical(er, ec, wrap_map, usable_width);

            if let (Some((start_line, start_char)), Some((end_line, end_char))) =
                (logical_start, logical_end)
            {
                for line_idx in start_line..=end_line {
                    if line_idx >= all_lines.len() {
                        continue;
                    }
                    let (cs, ce) = if line_idx == start_line && line_idx == end_line {
                        (start_char, end_char)
                    } else if line_idx == start_line {
                        (start_char, usize::MAX)
                    } else if line_idx == end_line {
                        (0, end_char)
                    } else {
                        (0, usize::MAX)
                    };
                    let spans = std::mem::take(&mut all_lines[line_idx].spans);
                    all_lines[line_idx] = Line::from(highlight_line_spans(spans, cs, ce));
                }
            }
            drop(cache);
        }
    }

    // 仅在有滚动条时显示 sticky header
    if max_scroll > 0 {
        sticky_header::render_sticky_header(f, app, header_area);
    }

    // 文字区域（留出右侧 1 列给滚动条）
    let text_area = Rect {
        width: inner.width.saturating_sub(1),
        ..inner
    };
    let paragraph = Paragraph::new(Text::from(all_lines))
        .scroll((offset, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, text_area);

    // 滚动条
    if max_scroll > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(offset as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(theme::MUTED));
        f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

/// 待发送附件栏（有附件时显示在输入框上方）
fn render_attachment_bar(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }

    let block = Block::default()
        .title(Span::styled(
            " 待发送附件 ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    f.render_widget(&block, area);

    let inner = block.inner(area);

    // 第 1 行：所有附件标签
    let tags: String = app
        .core
        .pending_attachments
        .iter()
        .map(|att| {
            let size_kb = (att.size_bytes / 1024).max(1);
            format!("[img {} {}KB]", att.label, size_kb)
        })
        .collect::<Vec<_>>()
        .join("  ");

    let lines = vec![
        Line::from(Span::styled(tags, Style::default().fg(theme::TEXT))),
        Line::from(Span::styled(
            "Del: 删除最后一张",
            Style::default().fg(theme::MUTED),
        )),
    ];

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// 对一行的 spans 做字符级选区高亮。
/// `char_start` / `char_end` 是该行 plain_text 的字符偏移（非 byte 索引）。
/// 将 spans 中对应范围的字符的 style 追加 Modifier::REVERSED，范围外的 span 保持原样。
/// 使用 char_indices() 保证 unicode 安全切割。
pub fn highlight_line_spans<'a>(
    spans: Vec<Span<'a>>,
    char_start: usize,
    char_end: usize,
) -> Vec<Span<'a>> {
    let mut result = Vec::new();
    let mut cursor: usize = 0; // 当前在 plain_text 中的字符位置
    for span in spans {
        let span_char_len = span.content.chars().count();
        let span_start = cursor;
        let span_end = cursor + span_char_len;

        if span_end <= char_start || span_start >= char_end {
            // 完全在选区外 → 保持原样
            result.push(span);
        } else if span_start >= char_start && span_end <= char_end {
            // 完全在选区内 → 整个 span 反色
            result.push(span.patch_style(Style::default().add_modifier(Modifier::REVERSED)));
        } else {
            // 部分重叠 → 拆分为 2~3 个子 span
            // 左段（选区外）
            if span_start < char_start {
                let skip = char_start - span_start;
                let byte_cut = span
                    .content
                    .char_indices()
                    .nth(skip)
                    .map(|(i, _)| i)
                    .unwrap_or(span.content.len());
                result.push(Span::styled(
                    span.content[..byte_cut].to_string(),
                    span.style,
                ));
            }
            // 中段（选区内，反色）
            let hl_char_start = span_start.max(char_start) - span_start;
            let hl_char_end = span_end.min(char_end) - span_start;
            let byte_start = span
                .content
                .char_indices()
                .nth(hl_char_start)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let byte_end = span
                .content
                .char_indices()
                .nth(hl_char_end)
                .map(|(i, _)| i)
                .unwrap_or(span.content.len());
            result.push(Span::styled(
                span.content[byte_start..byte_end].to_string(),
                span.style.add_modifier(Modifier::REVERSED),
            ));
            // 右段（选区外）
            if span_end > char_end {
                let skip = char_end - span_start;
                let byte_cut = span
                    .content
                    .char_indices()
                    .nth(skip)
                    .map(|(i, _)| i)
                    .unwrap_or(span.content.len());
                result.push(Span::styled(
                    span.content[byte_cut..].to_string(),
                    span.style,
                ));
            }
        }
        cursor = span_end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_line_spans_full_span() {
        let spans = vec![Span::from("Hello"), Span::from("World")];
        let result = highlight_line_spans(spans, 0, 10);
        assert_eq!(result.len(), 2);
        assert!(result[0].style.add_modifier.contains(Modifier::REVERSED));
        assert!(result[1].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_highlight_line_spans_partial_start() {
        let spans = vec![Span::from("Hello")];
        let result = highlight_line_spans(spans, 3, 10);
        // 前 3 字符原样，后 2 字符反色
        assert_eq!(result.len(), 2);
        assert!(!result[0].style.add_modifier.contains(Modifier::REVERSED));
        assert!(result[1].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[0].content, "Hel");
        assert_eq!(result[1].content, "lo");
    }

    #[test]
    fn test_highlight_line_spans_partial_both() {
        let spans = vec![Span::from("Hello")];
        let result = highlight_line_spans(spans, 1, 4);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].content, "H");
        assert!(!result[0].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[1].content, "ell");
        assert!(result[1].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[2].content, "o");
        assert!(!result[2].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_highlight_line_spans_multi_span() {
        let spans = vec![Span::from("Hel"), Span::from("lo Wo"), Span::from("rld")];
        let result = highlight_line_spans(spans, 2, 8);
        // 选中范围 char 2..8 = "llo Wo"
        // span0 "Hel": 前 2 原样 + 后 1 反色
        // span1 "lo Wo": 全部反色
        // span2 "rld": 不在选区（span2 starts at char 8）
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].content, "He");
        assert!(!result[0].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[1].content, "l");
        assert!(result[1].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[2].content, "lo Wo");
        assert!(result[2].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[3].content, "rld");
        assert!(!result[3].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_highlight_line_spans_outside() {
        let spans = vec![Span::from("Hello")];
        let result = highlight_line_spans(spans, 10, 15);
        assert_eq!(result.len(), 1);
        assert!(!result[0].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(result[0].content, "Hello");
    }
}
