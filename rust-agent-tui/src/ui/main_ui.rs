pub(crate) mod panels;
mod popups;
mod status_bar;
mod sticky_header;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::app::App;
use crate::ui::render_thread::RenderEvent;
use crate::ui::theme;
use crate::ui::welcome;
use rust_agent_middlewares::prelude::TodoStatus;

pub fn render(f: &mut Frame, app: &mut App) {
    // Setup 向导：全屏覆盖，优先于所有正常界面
    if app.global_ui.setup_wizard.is_some() {
        popups::setup_wizard::render_setup_wizard(f, app);
        return;
    }

    let area = f.area();

    if app.session_mgr.sessions.len() > 1 {
        // ── 多 Session 分栏布局 ──
        // 外层：水平切分（各 session 列）+ 底部共享状态栏
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // session 列区域
                Constraint::Length(3), // 共享状态栏
            ])
            .split(area);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(50);
                app.session_mgr.sessions.len()
            ])
            .split(outer[0]);

        app.session_mgr.session_areas = cols.iter().copied().collect();

        // 先渲染非 active session（不设置光标位置），再渲染 active session
        for (i, col_area) in cols.iter().enumerate() {
            if i == app.session_mgr.active {
                continue;
            }
            render_session_column(f, app, i, *col_area, false);
        }
        render_session_column(
            f,
            app,
            app.session_mgr.active,
            cols[app.session_mgr.active],
            true,
        );

        status_bar::render_status_bar(f, app, outer[1]);
    } else {
        // ── 单 Session 布局（原有行为）──
        render_session_column(f, app, 0, area, true);
    }
}

/// 渲染单个 session 列（含垂直布局拆分）
fn render_session_column(
    f: &mut Frame,
    app: &mut App,
    session_idx: usize,
    area: Rect,
    is_active: bool,
) {
    // 临时切换 active 以便现有 render 函数使用 app.session_mgr.sessions[app.session_mgr.active]
    let prev_active = app.session_mgr.active;
    app.session_mgr.active = session_idx;

    // 多 session 模式：外层 block 作为聚焦指示
    let area = if app.session_mgr.sessions.len() > 1 {
        let border_color = if is_active {
            theme::ACCENT
        } else {
            theme::BORDER_DIM
        };
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));
        let inner = outer_block.inner(area);
        f.render_widget(outer_block, area);
        inner
    } else {
        area
    };

    // 动态输入框高度
    let line_count = app.session_mgr.sessions[session_idx]
        .ui
        .textarea
        .lines()
        .len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);

    // 缓冲消息高度（loading 时在输入框上方显示待发送消息）
    let pending_count = app.session_mgr.sessions[session_idx]
        .messages
        .pending_messages
        .len();
    let queued_height: u16 =
        if pending_count > 0 && app.session_mgr.sessions[session_idx].ui.loading {
            (pending_count as u16).min(3)
        } else {
            0
        };

    // 附件栏高度
    let attachment_height: u16 = if app.session_mgr.sessions[session_idx]
        .metadata
        .pending_attachments
        .is_empty()
    {
        0
    } else {
        3
    };

    // 底部展开区高度
    let panel_height = active_panel_height(app, area.height, area.width);

    // Sticky header 高度
    let sticky_header_height: u16 = app.session_mgr.sessions[session_idx]
        .metadata
        .last_human_message
        .as_ref()
        .map(|msg| {
            let width = area.width.saturating_sub(2).max(1);
            let lines = sticky_header::estimate_header_lines(msg, width);
            lines as u16
        })
        .unwrap_or(0);

    let status_bar_height = if app.session_mgr.sessions.len() > 1 {
        0
    } else {
        3
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(sticky_header_height),
            Constraint::Min(1),
            Constraint::Length(attachment_height),
            Constraint::Length(panel_height),
            Constraint::Length(queued_height),
            Constraint::Length(input_height),
            Constraint::Length(status_bar_height),
        ])
        .split(area);

    render_messages(f, app, chunks[0], chunks[1]);
    render_attachment_bar(f, app, chunks[2]);

    // 底部展开区
    if panel_height > 0 {
        let panel_area = chunks[3];
        match &app.session_mgr.sessions[session_idx]
            .agent
            .interaction_prompt
        {
            Some(crate::app::InteractionPrompt::Approval(_)) => {
                popups::hitl::render_hitl_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Questions(_)) => {
                popups::ask_user::render_ask_user_popup(f, app, panel_area);
            }
            None => {}
        }
        if app.global_ui.oauth_prompt.is_some() {
            popups::oauth::render_oauth_popup(f, app, panel_area);
        }
        // PanelManager 统一渲染分发：session 面板优先，global 面板次之
        if app.session_mgr.sessions[session_idx]
            .agent
            .interaction_prompt
            .is_none()
            && app.global_ui.oauth_prompt.is_none()
        {
            if app.session_mgr.sessions[session_idx]
                .session_panels
                .is_any_open()
            {
                let mut state = app.session_mgr.sessions[session_idx]
                    .session_panels
                    .take_active()
                    .expect("is_any_open was true");
                state.render(f, app, panel_area);
                app.session_mgr.sessions[session_idx]
                    .session_panels
                    .put_active(state);
            } else if app.global_panels.is_any_open() {
                let mut state = app
                    .global_panels
                    .take_active()
                    .expect("is_any_open was true");
                state.render(f, app, panel_area);
                app.global_panels.put_active(state);
            }
        }
    }

    // 缓冲消息预览（loading 时在输入框上方显示待发送消息）
    if queued_height > 0 {
        let queued_area = chunks[4];
        let msgs = &app.session_mgr.sessions[session_idx]
            .messages
            .pending_messages;
        let visible_count = (pending_count).min(queued_height as usize);
        let pending_style = Style::default().fg(theme::MUTED).bg(theme::USER_BG);
        for (i, msg) in msgs.iter().take(visible_count).enumerate() {
            let line_area = Rect {
                x: queued_area.x + 2,
                y: queued_area.y + i as u16,
                width: queued_area.width.saturating_sub(2),
                height: 1,
            };
            // 截断到可用宽度（字符级安全）
            let max_chars = line_area.width as usize;
            let display: String = msg.chars().take(max_chars.saturating_sub(3)).collect();
            let suffix = if msg.chars().count() > max_chars.saturating_sub(3) {
                "…"
            } else {
                ""
            };
            f.render_widget(
                Paragraph::new(format!("{}{}", display, suffix)).style(pending_style),
                line_area,
            );
        }
        if pending_count > visible_count {
            let more_area = Rect {
                x: queued_area.x + 2,
                y: queued_area.y + visible_count as u16,
                width: queued_area.width.saturating_sub(2),
                height: 1,
            };
            f.render_widget(
                Paragraph::new(format!("… +{} more", pending_count - visible_count))
                    .style(pending_style),
                more_area,
            );
        }
    }

    // 输入框：非聚焦 session 或应用失焦时隐藏光标（移除 REVERSED 修饰符）
    let textarea_ref = &app.session_mgr.sessions[session_idx].ui.textarea;
    // 应用失焦 或 当前 session 未激活 → 隐藏光标
    let should_hide_cursor = !app.focused || !is_active;
    if should_hide_cursor {
        // 克隆并移除光标的 REVERSED 样式，使光标不可见
        let mut ta = textarea_ref.clone();
        // 将光标样式设置为与普通文本相同（无 REVERSED），光标字符将融入背景
        ta.set_cursor_style(Style::default().fg(theme::DIM));
        f.render_widget(&ta, chunks[5]);
    } else {
        f.render_widget(textarea_ref, chunks[5]);
    }
    app.session_mgr.sessions[session_idx].ui.textarea_area = Some(chunks[5]);

    // ❯ 前缀
    let prompt_x = chunks[5].x;
    let prompt_y = chunks[5].y + 1;
    let prompt_area = Rect {
        x: prompt_x,
        y: prompt_y,
        width: 2,
        height: 1,
    };
    let loading = app.session_mgr.sessions[session_idx].ui.loading;
    let prompt_color = if !is_active || loading {
        theme::MUTED
    } else {
        theme::TEXT
    };
    let prompt_style = Style::default().fg(prompt_color).add_modifier(if loading {
        Modifier::empty()
    } else {
        Modifier::BOLD
    });
    f.render_widget(Paragraph::new("❯").style(prompt_style), prompt_area);

    if is_active {
        // 统一命令/Skills 提示条
        popups::hints::render_unified_hint(f, app, chunks[5]);
    }

    // 单 session 模式下渲染状态栏
    if app.session_mgr.sessions.len() == 1 {
        status_bar::render_status_bar(f, app, chunks[6]);
    }

    // 恢复原始 active
    app.session_mgr.active = prev_active;
}

/// 计算底部展开区所需高度（无激活面板时返回 0）
fn active_panel_height(app: &App, screen_height: u16, screen_width: u16) -> u16 {
    // plugin 面板可以占 70%，其他面板最多 60%
    let is_plugin_panel = app.global_panels.is_active(crate::app::PanelKind::Plugin);
    let max_h = if is_plugin_panel {
        screen_height * 70 / 100
    } else {
        screen_height * 3 / 5
    };
    let raw = if let Some(h) = app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .dispatch_desired_height(screen_height, screen_width)
    {
        h
    } else if let Some(h) = app
        .global_panels
        .dispatch_desired_height(screen_height, screen_width)
    {
        h
    } else if let Some(crate::app::InteractionPrompt::Approval(p)) = &app.session_mgr.sessions
        [app.session_mgr.active]
        .agent
        .interaction_prompt
    {
        (p.items.len() as u16 * 2 + 5).max(5)
    } else if app.global_ui.oauth_prompt.is_some() {
        9 // 标题1 + 提示1 + URL1 + 空行1 + 输入框1 + 错误1 + 快捷键1 + 边框2
    } else if let Some(crate::app::InteractionPrompt::Questions(p)) = &app.session_mgr.sessions
        [app.session_mgr.active]
        .agent
        .interaction_prompt
    {
        let cur = &p.questions[p.active_tab];
        // 自适应高度：考虑文本自动换行
        let panel_width = screen_width.saturating_sub(4) as usize; // 减去边框+内边距
        let mut content_lines: u16 = 0;

        // 问题文本（考虑自动换行）
        for line in cur.data.question.lines() {
            let w = unicode_width::UnicodeWidthStr::width(line);
            content_lines += (w as u16).div_ceil(panel_width.max(1) as u16);
        }

        // [多选]/[单选] 提示
        content_lines += 1;

        // 选项（每个选项可能因标签长而换行）
        for opt in &cur.data.options {
            let label_w = unicode_width::UnicodeWidthStr::width(opt.label.as_str()) + 6; // " ▶ ○ " 前缀
            content_lines += (label_w as u16).div_ceil(panel_width.max(1) as u16);
            if let Some(ref desc) = opt.description {
                if !desc.is_empty() {
                    let desc_w = unicode_width::UnicodeWidthStr::width(desc.as_str()) + 6; // "      " 缩进
                    content_lines += (desc_w as u16).div_ceil(panel_width.max(1) as u16);
                }
            }
        }

        // 自定义输入区 + 空行 + 快捷键提示（固定 3 行）
        content_lines += 3;

        // header tab + 分隔线 + 边框 = 4
        (content_lines + 4).max(8)
    } else {
        0
    };
    raw.min(max_h)
}

fn render_messages(f: &mut Frame, app: &mut App, header_area: Rect, messages_area: Rect) {
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
            let frame = perihelion_widgets::spinner::animation::tick_to_frame(
                app.session_mgr.sessions[app.session_mgr.active]
                    .spinner_state
                    .tick(),
            );
            let verb = app.session_mgr.sessions[app.session_mgr.active]
                .spinner_state
                .verb();
            let elapsed = perihelion_widgets::spinner::animation::format_elapsed(
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
                let tokens_fmt = perihelion_widgets::spinner::animation::format_tokens(tokens);
                parts.push(Span::styled(format!(" · ↓ {tokens_fmt} tokens"), gray));
            }
            parts.push(Span::styled(")", gray));
            Some(Line::from(parts))
        } else if app.session_mgr.sessions[app.session_mgr.active]
            .spinner_state
            .last_summary_elapsed_ms()
            > 0
        {
            let elapsed = perihelion_widgets::spinner::animation::format_elapsed(
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
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
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
    let tags: String = app.session_mgr.sessions[app.session_mgr.active]
        .metadata
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
/// 将 spans 中对应范围的字符的 style 追加淡蓝色背景（深色主题选区色），范围外的 span 保持原样。
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
mod tests {
    use super::*;
    include!("main_ui_test.rs");
}
