use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::App;

/// AskUser 批量弹窗：header tab 行 + 当前问题选项
pub(crate) fn render_ask_user_popup(f: &mut Frame, app: &App) {
    let Some(crate::app::InteractionPrompt::Questions(prompt)) = &app.interaction_prompt else { return };

    let area = f.area();
    let popup_width = (area.width * 8 / 10).max(54).min(area.width.saturating_sub(4));

    // 当前问题的行数
    let cur = &prompt.questions[prompt.active_tab];
    let option_rows = cur.data.options.len() as u16;
    let desc_extra_rows: u16 = cur.data.options.iter().filter(|o| o.description.is_some()).count() as u16;
    // 1 header tab行 + 1 分隔线 + question行 + 1 [单/多选] + 选项 + 选项description + 1空行 + 2自定义输入行 + 边框(2)
    let question_rows = cur.data.question.lines().count() as u16;
    let popup_height = (1 + 1 + question_rows + 1 + option_rows + desc_extra_rows + 1 + 2 + 2)
        .min(area.height * 4 / 5)
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
        let label_text: String = if q.data.header.is_empty() {
            format!("Q{}", i + 1)
        } else {
            q.data.header.chars().take(12).collect()
        };
        let done = prompt.confirmed.get(i).copied().unwrap_or(false);
        let check = if done { "✓" } else { " " };
        let label = format!(" {check} {} ", label_text);
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

    // 问题文本
    for l in cur.data.question.lines() {
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
        // 选项 description（若有）
        if let Some(ref desc) = opt.description {
            if !desc.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("      {}", desc),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    // 自定义输入行（始终显示）
    lines.push(Line::from(""));
    let is_cur = cur.in_custom_input;
    let ph = "输入自定义内容…";
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

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((prompt.scroll_offset, 0))
            .wrap(Wrap { trim: false }),
        content_area,
    );
}
