use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::ui::theme;

/// AskUser 批量弹窗（底部展开区）：header tab 行 + 当前问题选项
pub(crate) fn render_ask_user_popup(f: &mut Frame, app: &App, area: Rect) {
    let Some(crate::app::InteractionPrompt::Questions(prompt)) = &app.agent.interaction_prompt else { return };

    let cur = &prompt.questions[prompt.active_tab];
    let popup_area = area;
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " ? Agent 提问 ",
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
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
            Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD)
        } else if done {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::MUTED)
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
        Paragraph::new(Span::styled(sep, Style::default().fg(theme::MUTED))),
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
        lines.push(Line::from(Span::styled(l.to_string(), Style::default().fg(theme::TEXT))));
    }
    let select_hint = if cur.data.multi_select { "[多选]" } else { "[单选]" };
    lines.push(Line::from(Span::styled(select_hint, Style::default().fg(theme::MUTED))));

    // 选项列表
    for (i, opt) in cur.data.options.iter().enumerate() {
        let is_cursor = !cur.in_custom_input && cur.option_cursor == i as isize;
        let is_selected = cur.selected.get(i).copied().unwrap_or(false);
        let check = if is_selected { "●" } else { "○" };
        let row_style = if is_cursor {
            Style::default().fg(Color::White).bg(theme::ACCENT)
        } else if is_selected {
            Style::default().fg(theme::ACCENT)
        } else {
            Style::default().fg(theme::TEXT)
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
                    Style::default().fg(theme::MUTED),
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
        Style::default().fg(Color::White).bg(theme::WARNING)
    } else {
        Style::default().fg(theme::MUTED)
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
