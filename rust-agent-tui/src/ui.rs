use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};

use rust_create_agent::messages::BaseMessage;
use crate::app::App;
use crate::app::model_panel::{EditField, ModelPanelMode, PROVIDER_TYPES};

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // 动态输入框高度：行数 + 边框（上下各 1），最少 3 行，最多 40%
    let line_count = app.textarea.lines().len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),            // 标题栏
            Constraint::Min(3),               // 聊天区
            Constraint::Length(input_height), // 输入框（动态）
            Constraint::Length(1),            // 帮助栏
        ])
        .split(area);

    render_title(f, app, chunks[0]);
    render_messages(f, app, chunks[1]);
    f.render_widget(&app.textarea, chunks[2]);
    render_help(f, app, chunks[3]);

    // 命令/Skills 提示条（浮动在输入框上方）
    render_command_hint(f, app, chunks[2]);
    render_skill_hint(f, app, chunks[2]);

    // HITL 弹窗（覆盖层）
    if app.hitl_prompt.is_some() {
        render_hitl_popup(f, app);
    }

    // AskUser 弹窗（覆盖层）
    if app.ask_user_prompt.is_some() {
        render_ask_user_popup(f, app);
    }

    // /model 面板（覆盖层）
    if app.model_panel.is_some() {
        render_model_panel(f, app);
    }

    // Thread 浏览面板（覆盖层，最高优先级）
    if app.thread_browser.is_some() {
        render_thread_browser(f, app);
    }
}

fn render_title(f: &mut Frame, app: &App, area: Rect) {
    let subtitle = format!(
        "  —  {} · {} | FilesystemMiddleware + TerminalMiddleware",
        app.provider_name, app.model_name
    );
    let title = Paragraph::new(
        Line::from(vec![
            Span::styled(" 🦀 ", Style::default().fg(Color::Red)),
            Span::styled("Rust Agent TUI", Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)),
            Span::styled(subtitle, Style::default().fg(Color::DarkGray)),
        ])
    )
    .style(Style::default().bg(Color::Black));
    f.render_widget(title, area);
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    // 右侧留 1 列给滚动条
    let inner = area;
    let inner_width = inner.width.saturating_sub(1) as usize;
    let mut all_lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        let is_conversational = matches!(msg.inner, BaseMessage::Human { .. } | BaseMessage::Ai { .. });
        if is_conversational {
            all_lines.push(Line::from(""));
        }
        all_lines.extend(message_to_lines(msg, inner_width));
        if is_conversational {
            all_lines.push(Line::from(""));
        }
    }

    // 计算每条 Line 经过自动换行后的实际视觉行数
    let visual_total: u16 = all_lines.iter().map(|l| visual_rows(l, inner_width)).sum();
    let visible_height = inner.height;

    let max_scroll = visual_total.saturating_sub(visible_height);
    // 计算本帧实际偏移，并写回 scroll_offset 保持同步
    let offset = if app.scroll_follow {
        max_scroll
    } else {
        app.scroll_offset.min(max_scroll)
    };
    app.scroll_offset = offset;

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
    if visual_total > visible_height {
        let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
            .position(offset as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::DarkGray));
        f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

fn message_to_lines(msg: &crate::app::ChatMessage, _width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let content = msg.content();

    match &msg.inner {
        BaseMessage::Human { .. } => {
            lines.push(Line::from(vec![
                Span::styled("▶ 你  ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(content, Style::default().fg(Color::White)),
            ]));
        }
        BaseMessage::Ai { .. } => {
            lines.push(Line::from(vec![
                Span::styled("◆ Agent  ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]));
            for text_line in content.lines() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(text_line.to_string(), Style::default().fg(Color::White)),
                ]));
            }
        }
        BaseMessage::Tool { is_error, .. } => {
            let name = msg.display_name.as_deref().unwrap_or("tool").to_string();
            let (icon, color) = if *is_error {
                ("✗", Color::Red)
            } else {
                let raw = msg.tool_name.as_deref().unwrap_or(&name);
                ("⚙", tool_color(raw))
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} {}", icon, name), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            ]));
            for line in content.lines() {
                lines.push(Line::from(vec![
                    Span::raw("  │ "),
                    Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
        BaseMessage::System { .. } => {
            for line in content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("ℹ ", Style::default().fg(Color::Blue)),
                    Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    lines
}

/// 按工具名分配颜色
fn tool_color(name: &str) -> Color {
    match name {
        "bash"                        => Color::Rgb(255, 165,   0), // 橙
        "read_file"                   => Color::Rgb( 97, 214, 214), // 青
        "write_file"                  => Color::Rgb(105, 240, 174), // 绿
        "edit_file"                   => Color::Rgb(179, 157, 219), // 紫
        "glob_files"                  => Color::Rgb(255, 213,  79), // 黄
        "search_files_rg"             => Color::Rgb(100, 181, 246), // 蓝
        "folder_operations"           => Color::Rgb(240, 128, 128), // 玫红
        _ if name.contains("error")   => Color::Red,
        _                             => Color::Yellow,
    }
}

/// 估算一条 Line 在给定宽度下占用的视觉行数（含自动换行）
fn visual_rows(line: &Line, width: usize) -> u16 {
    if width == 0 { return 1; }
    let char_count: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    ((char_count.max(1) + width - 1) / width) as u16
}

fn render_help(f: &mut Frame, app: &App, area: Rect) {
    let line = if app.ask_user_prompt.is_some() {
        Line::from(vec![
            Span::styled(" Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":切换问题  ", Style::default().fg(Color::DarkGray)),
            Span::styled("↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":移动  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":选择  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":确认此题/全部提交", Style::default().fg(Color::DarkGray)),
        ])
    } else if app.hitl_prompt.is_some() {
        Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":移动  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":切换  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":确认  ", Style::default().fg(Color::DarkGray)),
            Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(":全批准  ", Style::default().fg(Color::DarkGray)),
            Span::styled("n", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(":全拒绝", Style::default().fg(Color::DarkGray)),
        ])
    } else if app.loading {
        Line::from(vec![
            Span::styled(" ⠿ Agent 运行中", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("  ↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":滚动  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":退出", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":发送  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Alt+Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":换行  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc/Ctrl+C", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":退出  ", Style::default().fg(Color::DarkGray)),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

/// HITL 批量确认弹窗
fn render_hitl_popup(f: &mut Frame, app: &App) {
    let Some(prompt) = &app.hitl_prompt else { return };

    let area = f.area();
    let item_count = prompt.items.len();

    // 弹窗高度：标题(1) + 每项(2行) + 空行(1) + 底部提示(1) + 边框(2)
    let popup_height = ((item_count as u16 * 2) + 5).min(area.height.saturating_sub(4));
    let popup_width = (area.width * 4 / 5).max(55).min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let title = if item_count == 1 {
        " ⚠ 工具审批 (1 项) "
    } else {
        " ⚠ 批量工具审批 "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);
    let max_width = inner.width as usize;

    // 渲染每个工具调用项
    let mut lines: Vec<Line> = Vec::new();

    for (i, (item, &approved)) in prompt.items.iter().zip(prompt.approved.iter()).enumerate() {
        let is_cursor = i == prompt.cursor;

        // 状态图标和颜色
        let (status_icon, status_color) = if approved {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Red)
        };

        // 光标高亮
        let cursor_indicator = if is_cursor { "▶ " } else { "  " };
        let row_style = if is_cursor {
            Style::default().bg(Color::Rgb(40, 40, 60))
        } else {
            Style::default()
        };

        // 工具名行
        lines.push(Line::styled(
            format!(
                "{}{} {}  {}",
                cursor_indicator,
                status_icon,
                item.tool_name,
                if approved { "[批准]" } else { "[拒绝]" }
            ),
            if is_cursor {
                Style::default().fg(status_color).bg(Color::Rgb(40, 40, 60)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(status_color)
            },
        ));

        // 参数预览行
        let input_preview = format_input_preview(&item.input, max_width.saturating_sub(6));
        lines.push(Line::from(vec![
            Span::raw("     "),
            Span::styled(input_preview, row_style.fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(""));

    // 底部提示（仅多项时显示"按 Enter 按当前设置确认"）
    if item_count > 1 {
        lines.push(Line::from(vec![
            Span::styled(
                format!("已选: {} 批准 / {} 拒绝",
                    prompt.approved.iter().filter(|&&v| v).count(),
                    prompt.approved.iter().filter(|&&v| !v).count()
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let para = Paragraph::new(Text::from(lines));
    f.render_widget(para, inner);
}

/// AskUser 批量弹窗：header tab 行 + 当前问题选项
fn render_ask_user_popup(f: &mut Frame, app: &App) {
    let Some(prompt) = &app.ask_user_prompt else { return };

    let area = f.area();
    let popup_width = (area.width * 8 / 10).max(54).min(area.width.saturating_sub(4));

    // 当前问题的行数
    let cur = &prompt.questions[prompt.active_tab];
    let option_rows = cur.data.options.len() as u16;
    let extra_rows = if cur.data.allow_custom_input { 2u16 } else { 0 };
    // 1 header + 1 空行 + 描述行 + 空行 + 选项 + extra + 边框(2)
    let desc_rows = cur.data.description.lines().count() as u16;
    let popup_height = (1 + 1 + desc_rows + 1 + option_rows + extra_rows + 2)
        .min(area.height.saturating_sub(2));

    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " ? Agent 提问 ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    // ── header 行：每个问题一个 tab，激活的反色，已确认的显示 ✓ ──────────────
    let header_area = Rect { height: 1, ..inner };
    let mut tab_spans: Vec<Span> = Vec::new();
    for (i, q) in prompt.questions.iter().enumerate() {
        let short: String = q.data.description.chars().take(8).collect();
        let done = prompt.confirmed.get(i).copied().unwrap_or(false);
        let check = if done { "✓" } else { " " };
        let label = format!(" {check} Q{}: {} ", i + 1, short);
        let style = if i == prompt.active_tab {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if done {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(Span::styled(label, style));
        if i + 1 < prompt.questions.len() {
            tab_spans.push(Span::raw(" "));
        }
    }
    f.render_widget(Paragraph::new(Line::from(tab_spans)), header_area);

    // ── 分隔线 ────────────────────────────────────────────────────────────────
    let sep_area = Rect { y: inner.y + 1, height: 1, ..inner };
    let sep = "─".repeat(inner.width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(Color::DarkGray))),
        sep_area,
    );

    // ── 当前问题内容 ──────────────────────────────────────────────────────────
    let content_area = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(2),
        ..inner
    };
    let mut lines: Vec<Line> = Vec::new();

    // 描述
    for l in cur.data.description.lines() {
        lines.push(Line::from(Span::styled(l.to_string(), Style::default().fg(Color::White))));
    }
    let select_hint = if cur.data.multi_select { "[多选]" } else { "[单选]" };
    lines.push(Line::from(Span::styled(select_hint, Style::default().fg(Color::DarkGray))));

    // 选项列表
    for (i, opt) in cur.data.options.iter().enumerate() {
        let is_cursor = !cur.in_custom_input && cur.option_cursor == i as isize;
        let is_selected = cur.selected.get(i).copied().unwrap_or(false);
        let check = if is_selected { "●" } else { "○" };
        let row_style = if is_cursor {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else if is_selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} {} ", if is_cursor { "▶" } else { " " }, check),
                row_style,
            ),
            Span::styled(opt.label.clone(), row_style),
        ]));
    }

    // 自定义输入行
    if cur.data.allow_custom_input {
        lines.push(Line::from(""));
        let is_cur = cur.in_custom_input;
        let ph = cur.data.placeholder.as_deref().unwrap_or("输入自定义内容…");
        let display = if cur.custom_input.is_empty() && !is_cur {
            ph.to_string()
        } else {
            format!("{}{}", cur.custom_input, if is_cur { "█" } else { "" })
        };
        let style = if is_cur {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        lines.push(Line::from(vec![
            Span::styled(if is_cur { " ▶ " } else { "   " }, style),
            Span::styled(display, style),
        ]));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        content_area,
    );
}

/// 命令提示条：当输入以 / 开头时，在输入框上方浮动显示匹配命令
fn render_command_hint(f: &mut Frame, app: &App, input_area: Rect) {
    // 取输入框第一行内容
    let first_line = app.textarea.lines().first().map(|s| s.as_str()).unwrap_or("");
    if !first_line.starts_with('/') {
        return;
    }

    let prefix = first_line.trim_start_matches('/');
    let candidates = app.command_registry.match_prefix(prefix);

    // 无候选：不显示
    if candidates.is_empty() {
        return;
    }

    // 提示条高度 = 每行一条 + 边框(2)，最多显示 6 条
    let show_count = candidates.len().min(6) as u16;
    let hint_height = show_count + 2;

    // 紧贴输入框顶部向上偏移
    let y = input_area.y.saturating_sub(hint_height);
    let hint_area = Rect {
        x: input_area.x + 1,
        y,
        width: input_area.width.saturating_sub(2).min(50),
        height: hint_height,
    };

    f.render_widget(Clear, hint_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" 命令 ", Style::default().fg(Color::DarkGray)));
    f.render_widget(&block, hint_area);

    let inner = block.inner(hint_area);

    let selected = if first_line.starts_with('/') { app.hint_cursor } else { None };

    let lines: Vec<Line> = candidates
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, (name, desc))| {
            let is_selected = selected == Some(i);
            let bg = if is_selected { Color::DarkGray } else { Color::Reset };
            let typed_len = prefix.len();
            let (matched, rest) = name.split_at(typed_len.min(name.len()));
            Line::from(vec![
                Span::styled(if is_selected { "▸ /" } else { "  /" }, Style::default().fg(Color::Cyan).bg(bg)),
                Span::styled(matched.to_string(), Style::default().fg(Color::Cyan).bg(bg).add_modifier(Modifier::BOLD)),
                Span::styled(rest.to_string(), Style::default().fg(Color::White).bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray).bg(bg)),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// # Skills 提示浮层（输入以 # 开头时显示匹配的 skills）
fn render_skill_hint(f: &mut Frame, app: &App, input_area: Rect) {
    let first_line = app.textarea.lines().first().map(|s| s.as_str()).unwrap_or("");
    if !first_line.starts_with('#') {
        return;
    }

    let prefix = first_line.trim_start_matches('#');
    let candidates: Vec<_> = app.skills.iter()
        .filter(|s| prefix.is_empty() || s.name.contains(prefix))
        .take(8)
        .collect();

    if candidates.is_empty() {
        return;
    }

    let show_count = candidates.len().min(8) as u16;
    let hint_height = show_count + 2;

    let y = input_area.y.saturating_sub(hint_height);
    let hint_area = Rect {
        x: input_area.x + 1,
        y,
        width: input_area.width.saturating_sub(2).min(60),
        height: hint_height,
    };

    f.render_widget(Clear, hint_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Skills ", Style::default().fg(Color::DarkGray)));
    f.render_widget(&block, hint_area);

    let inner = block.inner(hint_area);

    let selected = if first_line.starts_with('#') { app.hint_cursor } else { None };

    let lines: Vec<Line> = candidates
        .iter()
        .enumerate()
        .map(|(i, skill)| {
            let is_selected = selected == Some(i);
            let bg = if is_selected { Color::DarkGray } else { Color::Reset };
            let name = &skill.name;
            if !prefix.is_empty() {
                if let Some(pos) = name.find(prefix) {
                    let before = &name[..pos];
                    let matched = &name[pos..pos + prefix.len()];
                    let after = &name[pos + prefix.len()..];
                    return Line::from(vec![
                        Span::styled(if is_selected { "▸ #" } else { "  #" }, Style::default().fg(Color::Cyan).bg(bg)),
                        Span::styled(before.to_string(), Style::default().fg(Color::White).bg(bg)),
                        Span::styled(matched.to_string(), Style::default().fg(Color::Cyan).bg(bg).add_modifier(Modifier::BOLD)),
                        Span::styled(after.to_string(), Style::default().fg(Color::White).bg(bg)),
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(skill.description.clone(), Style::default().fg(Color::DarkGray).bg(bg)),
                    ]);
                }
            }
            Line::from(vec![
                Span::styled(if is_selected { "▸ #" } else { "  #" }, Style::default().fg(Color::Cyan).bg(bg)),
                Span::styled(name.clone(), Style::default().fg(Color::White).bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(skill.description.clone(), Style::default().fg(Color::DarkGray).bg(bg)),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// /model 面板渲染
fn render_model_panel(f: &mut Frame, app: &App) {
    let Some(panel) = &app.model_panel else { return };

    let area = f.area();
    let popup_width = (area.width * 4 / 5).max(60).min(area.width.saturating_sub(4));
    let popup_height = 20u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    // 根据模式选颜色
    let (border_color, title) = match &panel.mode {
        ModelPanelMode::Browse        => (Color::Cyan,   " /model — Provider 配置 "),
        ModelPanelMode::Edit          => (Color::Yellow, " /model — 编辑 Provider "),
        ModelPanelMode::New           => (Color::Green,  " /model — 新建 Provider "),
        ModelPanelMode::ConfirmDelete => (Color::Red,    " /model — 确认删除 "),
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);
    let half = inner.height / 2;

    // ── 上半：provider 列表 ──────────────────────────────────────────────────
    let list_area = Rect { height: half.max(3), ..inner };
    let form_area = Rect {
        y: inner.y + list_area.height,
        height: inner.height.saturating_sub(list_area.height),
        ..inner
    };

    let mut list_lines: Vec<Line> = Vec::new();
    for (i, p) in panel.providers.iter().enumerate() {
        let is_cursor = i == panel.cursor;
        let is_active = p.id == panel.active_id;

        let bullet = if is_active { "●" } else { "○" };
        let cursor_char = if is_cursor { "▶" } else { " " };
        let name = p.display_name().to_string();
        let type_tag = format!("({})", p.provider_type);

        let row_style = if is_cursor {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };

        list_lines.push(Line::from(vec![
            Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
            Span::styled(format!("{} ", name), row_style.add_modifier(Modifier::BOLD)),
            Span::styled(type_tag, row_style.fg(if is_cursor { Color::Black } else { Color::DarkGray })),
        ]));
    }
    if panel.providers.is_empty() {
        list_lines.push(Line::from(Span::styled(
            "  （无 provider，按 n 新建）",
            Style::default().fg(Color::DarkGray),
        )));
    }
    f.render_widget(Paragraph::new(Text::from(list_lines)), list_area);

    // ── 下半：表单 or 确认删除 ────────────────────────────────────────────────
    match &panel.mode {
        ModelPanelMode::Browse => {
            // 显示当前选中 provider 的信息（只读）
            if let Some(p) = panel.providers.get(panel.cursor) {
                let model_display = app.zen_config.as_ref()
                    .map(|c| if c.config.provider_id == p.id { c.config.model_id.as_str() } else { "—" })
                    .unwrap_or("—");
                let key_masked = mask_api_key(&p.api_key);
                let mut info_lines = vec![
                    Line::from(vec![
                        Span::styled("  Model   ", Style::default().fg(Color::DarkGray)),
                        Span::styled(model_display.to_string(), Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled("  API Key ", Style::default().fg(Color::DarkGray)),
                        Span::styled(key_masked, Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled("  Base URL", Style::default().fg(Color::DarkGray)),
                        Span::styled(format!(" {}", p.base_url), Style::default().fg(Color::White)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(" Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":选择  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("e", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":编辑  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("n", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled(":新建  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("d", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                        Span::styled(":删除  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(":关闭", Style::default().fg(Color::DarkGray)),
                    ]),
                ];
                // 剪裁到可用高度
                info_lines.truncate(form_area.height as usize);
                f.render_widget(Paragraph::new(Text::from(info_lines)), form_area);
            }
        }
        ModelPanelMode::Edit | ModelPanelMode::New => {
            let fields = [
                (EditField::Name,         &panel.buf_name),
                (EditField::ProviderType, &panel.buf_type),
                (EditField::ModelId,      &panel.buf_model),
                (EditField::ApiKey,       &panel.buf_api_key),
                (EditField::BaseUrl,      &panel.buf_base_url),
            ];
            let mut form_lines: Vec<Line> = Vec::new();
            for (field, buf) in &fields {
                let is_active = *field == panel.edit_field;
                let label = field.label();

                // 特殊处理 ProviderType：显示可选值列表
                let value_display = if *field == EditField::ProviderType {
                    PROVIDER_TYPES.iter()
                        .map(|t| if *t == buf.as_str() { format!("[{}]", t) } else { t.to_string() })
                        .collect::<Vec<_>>()
                        .join("  ")
                } else if is_active {
                    format!("{}█", buf)
                } else {
                    let display = if *field == EditField::ApiKey { mask_api_key(buf) } else { buf.to_string() };
                    display
                };

                let (label_style, value_style) = if is_active {
                    (
                        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
                        Style::default().fg(Color::Black).bg(Color::Cyan),
                    )
                } else {
                    (
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::White),
                    )
                };
                form_lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", label), label_style),
                    Span::styled(format!(" {}", value_display), value_style),
                ]));
            }
            form_lines.push(Line::from(""));
            form_lines.push(Line::from(vec![
                Span::styled(" Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":切换字段  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":切换类型  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":保存  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(":取消", Style::default().fg(Color::DarkGray)),
            ]));
            form_lines.truncate(form_area.height as usize);
            f.render_widget(Paragraph::new(Text::from(form_lines)), form_area);
        }
        ModelPanelMode::ConfirmDelete => {
            if let Some(p) = panel.providers.get(panel.cursor) {
                let confirm_lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  确认删除 ", Style::default().fg(Color::White)),
                        Span::styled(p.display_name().to_string(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                        Span::styled(" ？", Style::default().fg(Color::White)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(" y", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                        Span::styled(":确认删除  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("n/Esc", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled(":取消", Style::default().fg(Color::DarkGray)),
                    ]),
                ];
                f.render_widget(Paragraph::new(Text::from(confirm_lines)), form_area);
            }
        }
    }
}

/// 遮盖 API Key 中间部分
fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[len - 4..].iter().collect();
    format!("{}****{}", prefix, suffix)
}

/// Thread 浏览面板
fn render_thread_browser(f: &mut Frame, app: &App) {
    let Some(browser) = &app.thread_browser else { return };

    let area = f.area();
    let popup_width = (area.width * 3 / 4).max(50).min(area.width.saturating_sub(4));
    let popup_height = (browser.total() as u16 + 4).min(area.height.saturating_sub(4)).max(6);
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " 📝 选择对话  ↑↓:移动  Enter:确认  d:删除  Esc:新建",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 第 0 项：新建对话
    let is_new_cursor = browser.cursor == 0;
    lines.push(Line::from(vec![
        Span::styled(
            if is_new_cursor { "▶ " } else { "  " },
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            "+ 新建对话",
            if is_new_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
    ]));

    // 历史 thread
    for (i, meta) in browser.threads.iter().enumerate() {
        let is_cursor = browser.cursor == i + 1;
        let title = meta.title.as_deref().unwrap_or("(无标题)");
        let updated = meta.updated_at.format("%m-%d %H:%M").to_string();
        let cwd_short: String = meta.cwd.chars().rev().take(20).collect::<String>().chars().rev().collect();
        let label = format!("{title}  [{updated}] …{cwd_short}");

        lines.push(Line::from(vec![
            Span::styled(
                if is_cursor { "▶ " } else { "  " },
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                label,
                if is_cursor {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]));
    }

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// 将工具参数格式化为单行预览
fn format_input_preview(input: &serde_json::Value, max_len: usize) -> String {
    let s = match input {
        serde_json::Value::Object(map) => {
            // 取最重要的字段：command > file_path > pattern > 第一个字段
            let key = ["command", "file_path", "pattern", "path"]
                .iter()
                .find(|k| map.contains_key(**k))
                .copied()
                .or_else(|| map.keys().next().map(|k| k.as_str()));

            if let Some(k) = key {
                if let Some(v) = map.get(k) {
                    let val = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    format!("{k}={val}")
                } else {
                    input.to_string()
                }
            } else {
                "{}".to_string()
            }
        }
        other => other.to_string(),
    };

    if s.chars().count() > max_len && max_len > 1 {
        format!("{}…", s.chars().take(max_len - 1).collect::<String>())
    } else {
        s
    }
}
