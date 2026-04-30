use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::App;
use crate::ui::theme;

/// 统一提示浮层：输入 / 前缀时分组展示命令和 Skills 候选
pub(crate) fn render_unified_hint(f: &mut Frame, app: &App, input_area: Rect) {
    let first_line = app
        .core
        .textarea
        .lines()
        .first()
        .map(|s| s.as_str())
        .unwrap_or("");
    if !first_line.starts_with('/') {
        return;
    }

    let prefix = first_line.trim_start_matches('/');
    let cmd_candidates: Vec<(&str, &str)> = app.core.command_registry.match_prefix(prefix);
    let cmd_show: Vec<_> = cmd_candidates.into_iter().take(6).collect();
    let skill_candidates: Vec<_> = app
        .core
        .skills
        .iter()
        .filter(|s| prefix.is_empty() || s.name.contains(prefix))
        .take(4)
        .collect();
    let total_count = cmd_show.len() + skill_candidates.len();
    if total_count == 0 {
        return;
    }

    let has_skills = !skill_candidates.is_empty();
    let hint_height = total_count as u16
        + 2 // 边框
        + 1 // "命令" 组标题
        + if has_skills { 2 } else { 0 }; // "Skills" 组标题 + 分隔线

    let y = input_area.y.saturating_sub(hint_height);
    let hint_area = Rect {
        x: input_area.x,
        y,
        width: input_area.width,
        height: hint_height,
    };

    let inner = BorderedPanel::new(Span::styled(" / ", Style::default().fg(theme::MUTED)))
        .border_style(Style::default().fg(theme::MUTED))
        .render(f, hint_area);

    let width = hint_area.width.saturating_sub(2); // 内部可用宽度
    let selected = app.core.hint_cursor;
    let mut i = 0; // 扁平候选索引（仅候选项递增，标题行和分隔线不递增）

    let mut lines: Vec<Line> = Vec::new();

    // "命令" 组标题
    lines.push(Line::from(Span::styled(
        "命令",
        Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD),
    )));

    // 命令候选行
    for (name, desc) in &cmd_show {
        let is_selected = selected == Some(i);
        let bg = if is_selected {
            theme::CURSOR_BG
        } else {
            Color::Reset
        };
        let typed_len = prefix.len();
        let (matched, rest) = name.split_at(typed_len.min(name.len()));
        lines.push(Line::from(vec![
            Span::styled(
                if is_selected { "▸ /" } else { "  /" },
                Style::default().fg(theme::ACCENT).bg(bg),
            ),
            Span::styled(
                matched.to_string(),
                Style::default()
                    .fg(theme::ACCENT)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(rest.to_string(), Style::default().fg(theme::TEXT).bg(bg)),
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(desc.to_string(), Style::default().fg(theme::MUTED).bg(bg)),
        ]));
        i += 1;
    }

    // Skills 组（如有）
    if has_skills {
        // 分隔线
        lines.push(Line::from(Span::styled(
            "─".repeat(width as usize),
            Style::default().fg(theme::MUTED),
        )));
        // "Skills" 组标题
        lines.push(Line::from(Span::styled(
            "Skills",
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::BOLD),
        )));

        for skill in &skill_candidates {
            let is_selected = selected == Some(i);
            let bg = if is_selected {
                theme::CURSOR_BG
            } else {
                Color::Reset
            };
            let name = &skill.name;
            if !prefix.is_empty() {
                if let Some(pos) = name.find(prefix) {
                    let before = &name[..pos];
                    let matched = &name[pos..pos + prefix.len()];
                    let after = &name[pos + prefix.len()..];
                    lines.push(Line::from(vec![
                        Span::styled(
                            if is_selected { "▸ /" } else { "  /" },
                            Style::default().fg(theme::ACCENT).bg(bg),
                        ),
                        Span::styled(before.to_string(), Style::default().fg(theme::TEXT).bg(bg)),
                        Span::styled(
                            matched.to_string(),
                            Style::default()
                                .fg(theme::ACCENT)
                                .bg(bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(after.to_string(), Style::default().fg(theme::TEXT).bg(bg)),
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(
                            skill.description.clone(),
                            Style::default().fg(theme::MUTED).bg(bg),
                        ),
                    ]));
                    i += 1;
                    continue;
                }
            }
            lines.push(Line::from(vec![
                Span::styled(
                    if is_selected { "▸ /" } else { "  /" },
                    Style::default().fg(theme::ACCENT).bg(bg),
                ),
                Span::styled(name.clone(), Style::default().fg(theme::TEXT).bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(
                    skill.description.clone(),
                    Style::default().fg(theme::MUTED).bg(bg),
                ),
            ]));
            i += 1;
        }
    }

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
