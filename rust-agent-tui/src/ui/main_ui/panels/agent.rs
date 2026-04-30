use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use perihelion_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::app::App;
use crate::ui::main_ui::highlight_line_spans;
use crate::ui::theme;

/// /agents 面板渲染（底部展开区）
pub(crate) fn render_agent_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(panel) = &app.core.agent_panel else {
        return;
    };

    let agent_count = panel.agents.len();
    let popup_area = area;

    let title = if agent_count == 0 {
        " Agent 选择 (无) "
    } else {
        " Agent 选择 "
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::MUTED))
    .render(f, popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // 第 0 项：取消选择（无 agent）
    let is_none_cursor = panel.cursor == 0;
    let is_none_selected = panel.selected_id.is_none();
    lines.push(Line::from(vec![
        Span::styled(
            if is_none_cursor { "▶ " } else { "  " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            "○ 无 Agent（默认）",
            if is_none_cursor {
                Style::default()
                    .fg(Color::White)
                    .bg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else if is_none_selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::MUTED)
            },
        ),
    ]));
    lines.push(Line::from("")); // 空行间隔

    // Agent 列表
    for (i, agent) in panel.agents.iter().enumerate() {
        let cursor_idx = i + 1; // +1 因为第 0 项是"无 Agent"
        let is_cursor = panel.cursor == cursor_idx;
        let is_selected = panel.selected_id.as_ref() == Some(&agent.id);

        let bullet = if is_selected { "●" } else { "○" };
        let cursor_char = if is_cursor { "▶" } else { " " };

        let name_style = if is_cursor {
            Style::default()
                .fg(Color::White)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} {}", cursor_char, bullet), name_style),
            Span::styled(format!(" {}", agent.name), name_style),
        ]));

        // 描述行（次要信息）
        if !agent.description.is_empty() {
            let desc_style = if is_cursor {
                Style::default().fg(theme::MUTED).bg(theme::ACCENT)
            } else {
                Style::default().fg(theme::MUTED)
            };
            // 截断过长的描述
            let desc: String = agent.description.chars().take(50).collect();
            let desc = if agent.description.len() > 50 {
                format!("{}…", desc)
            } else {
                desc
            };
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(desc, desc_style),
            ]));
        } else {
            lines.push(Line::from(""));
        }
    }

    // 空列表引导
    if agent_count == 0 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  在 .claude/agents/ 目录中添加 Agent 定义文件",
            Style::default().fg(theme::MUTED),
        )));
    }

    // 底部提示
    lines.push(Line::from(vec![
        Span::styled(
            " ↑↓",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":导航  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":选择  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":关闭", Style::default().fg(theme::MUTED)),
    ]));

    // 存储面板元数据供鼠标选区使用
    app.core.panel_area = Some(inner);
    app.core.panel_scroll_offset = panel.scroll_offset;
    app.core.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    // 应用面板选区高亮
    if app.core.panel_selection.is_active() {
        let sel = &app.core.panel_selection;
        if let (Some(start), Some(end)) = (sel.start, sel.end) {
            let ((sr, sc), (er, ec)) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let scroll = app.core.panel_scroll_offset as usize;
            let visible_start = scroll;
            let visible_end = scroll + inner.height as usize;
            for line_idx in sr as usize..=er as usize {
                if line_idx < visible_start || line_idx >= visible_end {
                    continue;
                }
                let visual_idx = line_idx - visible_start;
                if visual_idx >= lines.len() {
                    continue;
                }
                let (cs, ce) = if line_idx == sr as usize && line_idx == er as usize {
                    (sc as usize, ec as usize)
                } else if line_idx == sr as usize {
                    (sc as usize, usize::MAX)
                } else if line_idx == er as usize {
                    (0, ec as usize)
                } else {
                    (0, usize::MAX)
                };
                let spans = std::mem::take(&mut lines[visual_idx].spans);
                lines[visual_idx] = Line::from(highlight_line_spans(spans, cs, ce));
            }
        }
    }

    let mut scroll_state = ScrollState::with_offset(panel.scroll_offset);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, inner, &mut scroll_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent_panel::AgentPanel;
    use crate::app::App;

    fn render_headless_agent_empty() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.core.agent_panel = Some(AgentPanel::new(vec![], None));
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_agent_empty_shows_guide() {
        let (_, handle) = render_headless_agent_empty();
        let snap = handle.snapshot().join("\n");
        // 空列表应显示引导提示（用 ASCII 子串避免 CJK 宽字符问题）
        assert!(
            snap.contains("agents/"),
            "空列表应显示 Agent 定义文件引导，实际:\n{}",
            snap
        );
    }

    #[tokio::test]
    async fn test_agent_panel_has_nav_hint() {
        let (_, handle) = render_headless_agent_empty();
        let snap = handle.snapshot().join("\n");
        // 面板内或状态栏应包含导航相关提示
        let has_nav = snap.contains("导航") || snap.contains("选择") || snap.contains("Enter");
        assert!(has_nav, "Agent 面板应包含操作提示，实际:\n{}", snap);
    }
}
