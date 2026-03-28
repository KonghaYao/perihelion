use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;

/// HITL 批量确认弹窗
pub(crate) fn render_hitl_popup(f: &mut Frame, app: &App) {
    let Some(crate::app::InteractionPrompt::Approval(prompt)) = &app.interaction_prompt else { return };

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
