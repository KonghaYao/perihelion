use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, ScrollbarState, Wrap},
    Frame,
};

use crate::app::App;
use crate::ui::render_thread::RenderEvent;
use crate::ui::theme;
use crate::ui::welcome;
use peri_middlewares::prelude::TodoStatus;

use super::sticky_header;

pub(crate) fn render_messages(
    f: &mut Frame,
    app: &mut App,
    header_area: Rect,
    messages_area: Rect,
) {
    // Welcome Card 或消息列表
    if app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .is_empty()
    {
        welcome::render_welcome(f, app, messages_area);
        return;
    }

    let inner = messages_area;
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .messages_area = Some(inner);
    let visible_height = inner.height;

    // 计算 loading spinner 行（Claude Code 风格：✻ verb (Xm Xs · ↓ X.Xk tokens)）
    // compact 时紫色，其余橙色；loading 结束后显示总结行：✻ Brewed for Xm Xs
    let spinner_line: Option<Line<'static>> =
        if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
            let frame = peri_widgets::spinner::animation::tick_to_frame(
                app.session_mgr.sessions[app.session_mgr.active]
                    .spinner_state
                    .tick(),
            );
            let verb = app.session_mgr.sessions[app.session_mgr.active]
                .spinner_state
                .verb();
            let elapsed = peri_widgets::spinner::animation::format_elapsed(
                app.session_mgr.sessions[app.session_mgr.active]
                    .spinner_state
                    .elapsed_ms(),
            );
            let tokens = app.session_mgr.sessions[app.session_mgr.active]
                .spinner_state
                .displayed_tokens();

            let is_compact = verb.starts_with("压缩上下文");
            let accent = if is_compact {
                Style::default().fg(theme::THINKING)
            } else {
                Style::default().fg(theme::ACCENT)
            };
            let gray = Style::default().fg(theme::MUTED);
            let mut parts = vec![
                Span::styled(format!(" {} {}", frame, verb), accent),
                Span::styled(format!(" ({elapsed}"), gray),
            ];
            if tokens > 0 {
                let tokens_fmt = peri_widgets::spinner::animation::format_tokens(tokens);
                parts.push(Span::styled(format!(" · ↓ {tokens_fmt} tokens"), gray));
            }
            parts.push(Span::styled(")", gray));
            Some(Line::from(parts))
        } else if app.session_mgr.sessions[app.session_mgr.active]
            .spinner_state
            .last_summary_elapsed_ms()
            > 0
        {
            let elapsed = peri_widgets::spinner::animation::format_elapsed(
                app.session_mgr.sessions[app.session_mgr.active]
                    .spinner_state
                    .last_summary_elapsed_ms(),
            );
            Some(Line::from(Span::styled(
                format!("  ✻  Brewed for {elapsed}"),
                Style::default().fg(theme::MUTED),
            )))
        } else {
            None
        };

    // 渲染驱动宽度同步：用 last_resize_width 去抖——宽度未变时跳过重复发送，
    // 避免每秒 N 次 resize 事件导致渲染线程队列积压和 CPU 暴涨
    // （参见 spec/issues/2026-05-14-streaming-resize-cpu-spike）。
    {
        let text_area_width = inner.width.saturating_sub(1);
        let cache_width = app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .render_cache
            .read()
            .width;
        let messages = &mut app.session_mgr.sessions[app.session_mgr.active].messages;
        if messages.last_resize_width != Some(text_area_width)
            && cache_width != text_area_width
            && text_area_width > 0
        {
            messages.last_resize_width = Some(text_area_width);
            let _ = messages
                .render_tx
                .send(RenderEvent::Resize(text_area_width));
        }
    }

    // 从 RenderCache 读取已渲染好的行（浅克隆 Vec 头，开销极小）
    let (mut all_lines, _total_lines, max_scroll, offset, scroll_follow, last_render_version) = {
        let cache = app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .render_cache
            .read();

        // total_lines 已是 wrap 后的真实视觉行数（由渲染线程通过 Paragraph::line_count 计算）
        let total_lines = cache.total_lines;
        let spinner_extra: u16 = if spinner_line.is_some() {
            let base = 1 + 2; // spinner line + 2 padding blank lines
            if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                // tip + blank + todo items + trailing blanks(3)
                base + 3
                    + app.session_mgr.sessions[app.session_mgr.active]
                        .todo_items
                        .len() as u16
            } else {
                base + 2 // trailing blanks(3)
            }
        } else {
            0
        };
        let visual_total = (total_lines as u16).saturating_add(spinner_extra);
        let max_scroll = visual_total.saturating_sub(visible_height);
        let scroll_follow = app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .scroll_follow;
        let scroll_offset = app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .scroll_offset;
        let (new_follow, off, ver) = if scroll_follow {
            (scroll_follow, max_scroll, cache.version)
        } else {
            let off = scroll_offset.min(max_scroll);
            let new_follow = off >= max_scroll;
            (new_follow, off, cache.version)
        };

        // Vec::clone() 是浅克隆，只复制指针+容量+长度头（3个 usize），不复制 Line 内容
        (
            cache.lines.clone(),
            total_lines,
            max_scroll,
            off,
            new_follow,
            ver,
        )
    };
    // 在 cache read guard 释放后写入
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .last_render_version = last_render_version;
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .scroll_follow = scroll_follow;
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .scroll_offset = offset;
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .scrollbar_max_offset = max_scroll;
    if let Some(line) = spinner_line {
        all_lines.push(Line::from(""));
        all_lines.push(line);
        // Tip + TODO 仅在活跃 loading 时显示
        if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
            let tip = crate::ui::tips::pick_tip(
                app.session_mgr.sessions[app.session_mgr.active]
                    .spinner_state
                    .raw_tick(),
                &app.services.lc,
            );
            all_lines.push(Line::from(vec![
                Span::styled("  ⎿  Tip: ", Style::default().fg(theme::MUTED)),
                Span::styled(tip, Style::default().fg(theme::MUTED)),
            ]));
            all_lines.push(Line::from(""));
            for item in &app.session_mgr.sessions[app.session_mgr.active].todo_items {
                let (icon, icon_style, text_style) = match item.status {
                    TodoStatus::InProgress => (
                        "  ◼  ",
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                        Style::default().fg(theme::TEXT),
                    ),
                    TodoStatus::Completed => (
                        "  ✔  ",
                        Style::default().fg(theme::SAGE),
                        Style::default()
                            .fg(theme::MUTED)
                            .add_modifier(Modifier::CROSSED_OUT),
                    ),
                    TodoStatus::Pending => (
                        "  ◻  ",
                        Style::default().fg(theme::MUTED),
                        Style::default().fg(theme::MUTED),
                    ),
                };
                let hint = match item.status {
                    TodoStatus::Pending => Some("可开始"),
                    _ => None,
                };
                let mut spans = vec![
                    Span::styled(icon, icon_style),
                    Span::styled(&item.content, text_style),
                ];
                if let Some(hint) = hint {
                    spans.push(Span::styled(
                        format!(" ({hint})"),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                all_lines.push(Line::from(spans));
            }
            for _ in 0..3 {
                all_lines.push(Line::from(""));
            }
        } else {
            for _ in 0..3 {
                all_lines.push(Line::from(""));
            }
        }
    }

    // 字符级选区高亮
    if app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .text_selection
        .is_active()
    {
        let ts = &app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .text_selection;
        if let (Some(start), Some(end)) = (ts.start, ts.end) {
            let cache = app.session_mgr.sessions[app.session_mgr.active]
                .messages
                .render_cache
                .read();
            let wrap_map = &cache.wrap_map;
            let usable_width = app.session_mgr.sessions[app.session_mgr.active]
                .ui
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
        let scrollbar =
            peri_widgets::unified_vertical_scrollbar().style(Style::default().fg(theme::MUTED));
        f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);

        // 滚动到底按钮（当用户滚离底部时显示）
        if offset < max_scroll {
            let btn_area = Rect {
                x: inner.right().saturating_sub(1),
                y: inner.bottom().saturating_sub(1),
                width: 1,
                height: 1,
            };
            let arrow = Paragraph::new(Text::from(Span::styled(
                "▼",
                Style::default()
                    .fg(theme::MUTED)
                    .add_modifier(Modifier::BOLD),
            )));
            f.render_widget(arrow, btn_area);
        }

        // 滚动到顶按钮（当用户滚离顶部时显示）
        if offset > 0 {
            let btn_area = Rect {
                x: inner.right().saturating_sub(1),
                y: inner.y,
                width: 1,
                height: 1,
            };
            let arrow = Paragraph::new(Text::from(Span::styled(
                "▲",
                Style::default()
                    .fg(theme::MUTED)
                    .add_modifier(Modifier::BOLD),
            )));
            f.render_widget(arrow, btn_area);
        }
    }
}

/// 对一行的 spans 做字符级选区高亮。
/// `char_start` / `char_end` 是该行 plain_text 的字符偏移（非 byte 索引）。
/// 将 spans 中对应范围的字符的 style 追加淡蓝色背景（深色主题选区色），范围外的 span 保持原样。
/// 使用 char_indices() 保证 unicode 安全切割。
pub(crate) fn highlight_line_spans<'a>(
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
            // 完全在选区内 → 淡蓝色背景（强制覆盖原有 bg）
            result.push(Span::styled(
                span.content,
                Style {
                    fg: span.style.fg,
                    bg: Some(theme::SELECTION_BG),
                    underline_color: span.style.underline_color,
                    add_modifier: span.style.add_modifier,
                    sub_modifier: span.style.sub_modifier,
                },
            ));
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
            // 中段（选区内，淡蓝色背景）
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
                Style {
                    fg: span.style.fg,
                    bg: Some(theme::SELECTION_BG),
                    underline_color: span.style.underline_color,
                    add_modifier: span.style.add_modifier,
                    sub_modifier: span.style.sub_modifier,
                },
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
#[path = "message_area_test.rs"]
mod message_area_test;
