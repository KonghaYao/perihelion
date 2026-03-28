use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::ui::theme;

/// 命令提示条：当输入以 / 开头时，在输入框上方浮动显示匹配命令
pub(crate) fn render_command_hint(f: &mut Frame, app: &App, input_area: Rect) {
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
        .border_style(Style::default().fg(theme::MUTED))
        .title(Span::styled(" 命令 ", Style::default().fg(theme::MUTED)));
    f.render_widget(&block, hint_area);

    let inner = block.inner(hint_area);

    let selected = if first_line.starts_with('/') { app.hint_cursor } else { None };

    let lines: Vec<Line> = candidates
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, (name, desc))| {
            let is_selected = selected == Some(i);
            let bg = if is_selected { theme::CURSOR_BG } else { Color::Reset };
            let typed_len = prefix.len();
            let (matched, rest) = name.split_at(typed_len.min(name.len()));
            Line::from(vec![
                Span::styled(if is_selected { "▸ /" } else { "  /" }, Style::default().fg(theme::ACCENT).bg(bg)),
                Span::styled(matched.to_string(), Style::default().fg(theme::ACCENT).bg(bg).add_modifier(Modifier::BOLD)),
                Span::styled(rest.to_string(), Style::default().fg(theme::TEXT).bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(desc.to_string(), Style::default().fg(theme::MUTED).bg(bg)),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Skills 提示浮层（输入以 # 开头时显示匹配的 skills）
pub(crate) fn render_skill_hint(f: &mut Frame, app: &App, input_area: Rect) {
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
        .border_style(Style::default().fg(theme::MUTED))
        .title(Span::styled(" Skills ", Style::default().fg(theme::MUTED)));
    f.render_widget(&block, hint_area);

    let inner = block.inner(hint_area);

    let selected = if first_line.starts_with('#') { app.hint_cursor } else { None };

    let lines: Vec<Line> = candidates
        .iter()
        .enumerate()
        .map(|(i, skill)| {
            let is_selected = selected == Some(i);
            let bg = if is_selected { theme::CURSOR_BG } else { Color::Reset };
            let name = &skill.name;
            if !prefix.is_empty() {
                if let Some(pos) = name.find(prefix) {
                    let before = &name[..pos];
                    let matched = &name[pos..pos + prefix.len()];
                    let after = &name[pos + prefix.len()..];
                    return Line::from(vec![
                        Span::styled(if is_selected { "▸ #" } else { "  #" }, Style::default().fg(theme::ACCENT).bg(bg)),
                        Span::styled(before.to_string(), Style::default().fg(theme::TEXT).bg(bg)),
                        Span::styled(matched.to_string(), Style::default().fg(theme::ACCENT).bg(bg).add_modifier(Modifier::BOLD)),
                        Span::styled(after.to_string(), Style::default().fg(theme::TEXT).bg(bg)),
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(skill.description.clone(), Style::default().fg(theme::MUTED).bg(bg)),
                    ]);
                }
            }
            Line::from(vec![
                Span::styled(if is_selected { "▸ #" } else { "  #" }, Style::default().fg(theme::ACCENT).bg(bg)),
                Span::styled(name.clone(), Style::default().fg(theme::TEXT).bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(skill.description.clone(), Style::default().fg(theme::MUTED).bg(bg)),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
