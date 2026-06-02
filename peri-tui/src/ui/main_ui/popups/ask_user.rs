use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea, TabBar, TabState, TabStyle};

use crate::{app::App, ui::theme};

/// AskUser 批量弹窗（底部展开区）：header tab 行 + 编号选项列表
///
/// 风格对齐 Claude Code 原生 AskUser UI：
/// - Tab 栏带 ☐/✔ 状态标记
/// - 选项编号格式（单选: `❯ 1. label`，多选: `❯ ● 1. label`）
/// - 自定义输入合并为最后一个编号选项
pub(crate) fn render_ask_user_popup(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(crate::app::InteractionPrompt::Questions(prompt)) =
        &app.session_mgr.current_mut().agent.interaction_prompt
    else {
        return;
    };

    let cur = &prompt.questions[prompt.active_tab];

    let inner = BorderedPanel::new(Span::styled("", Style::default()))
        .border_style(Style::default().fg(theme::THINKING))
        .render(f, area);

    // ── header 行：每个问题一个 tab，已确认 ✔ 未确认 ☐ ─────────────────────
    let header_area = Rect { height: 1, ..inner };
    let labels: Vec<String> = prompt
        .questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let done = prompt.confirmed.get(i).copied().unwrap_or(false);
            let header = if q.data.header.is_empty() {
                format!("Q{}", i + 1)
            } else {
                q.data.header.chars().take(10).collect()
            };
            if done {
                format!("✔ {}", header)
            } else {
                format!("☐ {}", header)
            }
        })
        .collect();
    let mut tab_state = TabState::new(labels);
    tab_state.set_active(prompt.active_tab);
    let tab_bar = TabBar::new().style(TabStyle {
        active: Style::default()
            .fg(Color::White)
            .bg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
        completed: Style::default().fg(theme::SAGE),
        incomplete: Style::default().fg(theme::MUTED),
        separator: " ",
    });
    f.render_stateful_widget(tab_bar, header_area, &mut tab_state);

    // ── 分隔线 ────────────────────────────────────────────────────────────────
    let sep_area = Rect {
        y: inner.y + 1,
        height: 1,
        ..inner
    };
    let sep = "─".repeat(inner.width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(theme::MUTED))),
        sep_area,
    );

    // ── 内容区 ────────────────────────────────────────────────────────────────
    let content_area = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(2),
        ..inner
    };
    let mut lines: Vec<Line> = Vec::new();
    let mut option_row_map: Vec<u16> = Vec::new();

    // 问题文本
    for l in cur.data.question.lines() {
        lines.push(Line::from(Span::styled(
            l.to_string(),
            Style::default().fg(theme::TEXT),
        )));
    }
    lines.push(Line::from(""));

    // 选项列表（编号格式）
    let multi = cur.data.multi_select;
    let option_count = cur.data.options.len();
    for (i, opt) in cur.data.options.iter().enumerate() {
        option_row_map.push(lines.len() as u16);
        let is_cursor = !cur.in_custom_input && cur.option_cursor == i as isize;
        let is_selected = cur.selected.get(i).copied().unwrap_or(false);
        let num = i + 1;
        let cursor_mark = if is_cursor { "❯" } else { " " };

        let row_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::TEXT)
        };

        if multi {
            let check = if is_selected { "●" } else { "○" };
            lines.push(Line::from(vec![
                Span::styled(format!("{} {} {}. ", cursor_mark, check, num), row_style),
                Span::styled(opt.label.clone(), row_style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("{} {}. ", cursor_mark, num), row_style),
                Span::styled(opt.label.clone(), row_style),
            ]));
        }

        // 选项 description（若有）
        if let Some(ref desc) = opt.description {
            if !desc.is_empty() {
                let indent = if multi { "       " } else { "     " };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", indent, desc),
                    Style::default().fg(theme::MUTED),
                )));
            }
        }

        // 选项之间空一行（最后一个不加）
        if i < option_count - 1 {
            lines.push(Line::from(""));
        }
    }

    // 自定义输入前加空行分隔
    lines.push(Line::from(""));

    // 自定义输入作为最后一个编号选项
    {
        let custom_num = option_count + 1;
        let is_cursor = cur.in_custom_input;
        let cursor_mark = if is_cursor { "❯" } else { " " };
        let display = if cur.custom_input.is_empty() && !is_cursor {
            app.services.lc.tr("ask-user-placeholder")
        } else if is_cursor {
            let (before, after) =
                crate::app::edit_display_parts(&cur.custom_input, cur.custom_cursor);
            format!("{}█{}", before, after)
        } else {
            cur.custom_input.clone()
        };
        let style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} {}. ", cursor_mark, custom_num), style),
            Span::styled(display, style),
        ]));
    }

    let mut scroll_state = ScrollState::with_offset(prompt.scroll_offset);
    let metrics = ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, content_area, &mut scroll_state);
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics = metrics;
    // 存储面板区域供鼠标事件路由
    app.session_mgr.current_mut().ui.panel_area = Some(area);
    if let Some(crate::app::InteractionPrompt::Questions(p)) = app
        .session_mgr
        .current_mut()
        .agent
        .interaction_prompt
        .as_mut()
    {
        p.scrollbar_metrics = metrics;
        p.option_row_map = option_row_map;
    }
}
