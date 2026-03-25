use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use ratatui::layout::Rect;

use crate::app::App;

pub(crate) fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    // ── 左侧：工作目录 | Agent 状态 | 运行时长 ────────────────────────────────
    let mut left_spans: Vec<Span> = Vec::new();

    // 工作目录（只显示最后一个文件夹名）
    let cwd_short = std::path::Path::new(&app.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&app.cwd);
    left_spans.push(Span::styled(
        format!(" 📁 {}", cwd_short),
        Style::default().fg(Color::DarkGray),
    ));
    left_spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));

    // Agent 状态
    if app.loading {
        left_spans.push(Span::styled("⠿ 运行中", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    } else {
        left_spans.push(Span::styled("● 空闲", Style::default().fg(Color::Green)));
    }

    // 运行时长
    if let Some(duration) = app.get_current_task_duration() {
        left_spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        left_spans.push(Span::styled(
            format!("⏱ {}", format_duration(duration)),
            Style::default().fg(Color::Cyan),
        ));
    }

    // 模型信息（始终显示在右侧）：★Alias → provider/model
    left_spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
    {
        let alias_display = app.zen_config.as_ref().map(|c| {
            let alias = &c.config.active_alias;
            let mapping = match alias.as_str() {
                "opus"   => &c.config.model_aliases.opus,
                "sonnet" => &c.config.model_aliases.sonnet,
                "haiku"  => &c.config.model_aliases.haiku,
                _        => &c.config.model_aliases.opus,
            };
            let alias_cap = alias.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
                + &alias[alias.char_indices().nth(1).map(|(i,_)|i).unwrap_or(alias.len())..];
            let model_part = if mapping.model_id.is_empty() { app.model_name.as_str() } else { mapping.model_id.as_str() };
            format!("★{} → {}/{}", alias_cap, mapping.provider_id, model_part)
        }).unwrap_or_else(|| format!(" {} {}", app.provider_name, app.model_name));
        left_spans.push(Span::styled(
            format!(" {}", alias_display),
            Style::default().fg(Color::Rgb(150, 180, 255)),
        ));
    }

    // Agent 面板选中信息
    if let Some(panel) = &app.agent_panel {
        left_spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        if let Some(agent) = panel.current_agent() {
            left_spans.push(Span::styled(
                format!(" 🤖 {}", agent.name),
                Style::default().fg(Color::Magenta),
            ));
        } else {
            left_spans.push(Span::styled(" 🤖 无", Style::default().fg(Color::DarkGray)));
        }
    } else if let Some(id) = app.get_agent_id() {
        // 已在运行中的 agent（非面板模式）
        left_spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        left_spans.push(Span::styled(
            format!(" 🤖 {}", id),
            Style::default().fg(Color::Magenta),
        ));
    }

    // ── 右侧：弹窗激活时显示快捷键提示 ─────────────────────────────────────
    let right_spans: Vec<Span> = if app.ask_user_prompt.is_some() {
        vec![
            Span::styled(" Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":切换  ", Style::default().fg(Color::DarkGray)),
            Span::styled("↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":移动  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":选择  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":确认", Style::default().fg(Color::DarkGray)),
        ]
    } else if app.hitl_prompt.is_some() {
        vec![
            Span::styled(" ↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":移动  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":切换  ", Style::default().fg(Color::DarkGray)),
            Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(":全批准  ", Style::default().fg(Color::DarkGray)),
            Span::styled("n", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(":全拒绝  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":确认", Style::default().fg(Color::DarkGray)),
        ]
    } else if app.agent_panel.is_some() {
        vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":选择  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":确认  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(":取消", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![]
    };

    // ── 计算左右侧宽度，确保右侧对齐 ───────────────────────────────────────
    let left_width: usize = left_spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.width()).sum();

    // 中间填充空格
    let total_content_width = left_width + right_width;
    let padding = if total_content_width < area.width as usize {
        " ".repeat(area.width as usize - total_content_width)
    } else {
        " ".to_string()
    };

    let mut all_spans = left_spans;
    all_spans.push(Span::raw(padding));
    all_spans.extend(right_spans);

    f.render_widget(Paragraph::new(Line::from(all_spans)), area);
}

fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}
