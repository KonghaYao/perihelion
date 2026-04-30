use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::App;
use crate::ui::theme;

/// HITL 批量确认弹窗（底部展开区）
pub(crate) fn render_hitl_popup(f: &mut Frame, app: &App, area: Rect) {
    let Some(crate::app::InteractionPrompt::Approval(prompt)) = &app.agent.interaction_prompt
    else {
        return;
    };

    let item_count = prompt.items.len();
    let popup_area = area;

    let title = if item_count == 1 {
        " ⚠ 工具审批 (1 项) "
    } else {
        " ⚠ 批量工具审批 "
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::WARNING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::WARNING))
    .render(f, popup_area);
    let max_width = inner.width as usize;

    // 渲染每个工具调用项
    let mut lines: Vec<Line> = Vec::new();

    for (i, (item, &approved)) in prompt.items.iter().zip(prompt.approved.iter()).enumerate() {
        let is_cursor = i == prompt.cursor;

        // 状态图标和颜色
        let (status_icon, status_color) = if approved {
            ("✓", theme::SAGE)
        } else {
            ("✗", theme::ERROR)
        };

        // 光标高亮
        let cursor_indicator = if is_cursor { "▶ " } else { "  " };
        let row_style = if is_cursor {
            Style::default().bg(theme::CURSOR_BG)
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
                Style::default()
                    .fg(status_color)
                    .bg(theme::CURSOR_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(status_color)
            },
        ));

        // 参数预览行
        let input_preview = format_input_preview(&item.input, max_width.saturating_sub(6));
        lines.push(Line::from(vec![
            Span::raw("     "),
            Span::styled(input_preview, row_style.fg(theme::DIM)),
        ]));
    }

    lines.push(Line::from(""));

    // 底部提示
    if item_count > 1 {
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "已选: {} 批准 / {} 拒绝  ",
                    prompt.approved.iter().filter(|&&v| v).count(),
                    prompt.approved.iter().filter(|&&v| !v).count()
                ),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(":确认", Style::default().fg(theme::MUTED)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                "Space",
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(":切换  ", Style::default().fg(theme::MUTED)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(":确认", Style::default().fg(theme::MUTED)),
        ]));
    }

    let para = Paragraph::new(Text::from(lines));
    f.render_widget(para, inner);
}

fn format_input_preview(input: &serde_json::Value, max_len: usize) -> String {
    let s = match input {
        serde_json::Value::Object(map) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::app::{HitlBatchPrompt, InteractionPrompt};
    use rust_agent_middlewares::hitl::BatchItem;

    fn render_headless_hitl_single() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let items = vec![BatchItem {
            tool_name: "bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        }];
        let prompt = HitlBatchPrompt::new(items, tx);
        app.agent.interaction_prompt = Some(InteractionPrompt::Approval(prompt));
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    fn render_headless_hitl_multi() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let items = vec![
            BatchItem {
                tool_name: "bash".to_string(),
                input: serde_json::json!({"command": "ls"}),
            },
            BatchItem {
                tool_name: "write_file".to_string(),
                input: serde_json::json!({"path": "test.rs"}),
            },
        ];
        let prompt = HitlBatchPrompt::new(items, tx);
        app.agent.interaction_prompt = Some(InteractionPrompt::Approval(prompt));
        // 通过 main_ui::render 渲染完整布局，确保面板高度正确
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_hitl_single_no_single_letter_hints() {
        let (_, handle) = render_headless_hitl_single();
        let snap = handle.snapshot().join("\n");
        // 不应出现单字母快捷键 y 或 n（作为独立快捷键提示）
        assert!(
            !snap.contains(":批准") || !snap.contains("y:"),
            "不应显示 y:批准 单字母快捷键"
        );
        assert!(
            !snap.contains(":拒绝") || !snap.contains("n:"),
            "不应显示 n:拒绝 单字母快捷键"
        );
        // 应显示合规快捷键
        assert!(handle.contains("Space"), "应显示 Space 快捷键");
        assert!(handle.contains("Enter"), "应显示 Enter 快捷键");
    }

    #[tokio::test]
    async fn test_hitl_multi_shows_enter_hint() {
        let (_, handle) = render_headless_hitl_multi();
        let snap = handle.snapshot().join("\n");
        // 多项应显示 Enter 确认
        assert!(
            snap.contains("Enter"),
            "多项应显示 Enter 快捷键，实际:\n{}",
            snap
        );
    }
}
