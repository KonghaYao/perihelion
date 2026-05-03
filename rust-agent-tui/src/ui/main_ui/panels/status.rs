use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};
use perihelion_widgets::{BorderedPanel, tab_bar::TabBar};
use crate::app::status_panel::{STATUS_TAB_COST, STATUS_TAB_CONTEXT};
use crate::app::App;
use crate::ui::theme;

pub(crate) fn render_status_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(panel) = &app.status_panel else {
        return;
    };

    let inner = BorderedPanel::new(Span::styled(
        " Status ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    // Tab 栏（1 行）
    let tab_height = 1u16;
    let tab_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: tab_height,
    };
    let content_area = Rect {
        x: inner.x,
        y: inner.y + tab_height + 1,
        width: inner.width,
        height: inner.height.saturating_sub(tab_height + 1),
    };

    let mut tab_state = panel.tab.clone();
    f.render_stateful_widget(TabBar::new(), tab_area, &mut tab_state);

    let lines = match panel.tab.active() {
        STATUS_TAB_COST => build_cost_lines(app),
        STATUS_TAB_CONTEXT => build_context_lines(app),
        _ => vec![],
    };

    f.render_widget(Paragraph::new(Text::from(lines)), content_area);
}

fn build_cost_lines(app: &App) -> Vec<Line<'static>> {
    let tracker = &app.agent.session_token_tracker;
    let mut lines: Vec<Line<'static>> = Vec::new();

    // 会话时长
    let duration_str = match app.agent.session_start_time {
        Some(start) => {
            let s = start.elapsed().as_secs();
            if s >= 3600 {
                format!("{}h{}m{}s", s / 3600, (s % 3600) / 60, s % 60)
            } else if s >= 60 {
                format!("{}m{}s", s / 60, s % 60)
            } else {
                format!("{}s", s)
            }
        }
        None => "N/A".to_string(),
    };
    lines.push(label_value("会话时长", &duration_str));
    lines.push(Line::from(""));

    // Token 消耗
    lines.push(label_value(
        "输入 Tokens",
        &format_number(tracker.total_input_tokens),
    ));
    lines.push(label_value(
        "输出 Tokens",
        &format_number(tracker.total_output_tokens),
    ));
    lines.push(label_value(
        "Cache 创建",
        &format_number(tracker.total_cache_creation_tokens),
    ));
    lines.push(label_value(
        "Cache 读取",
        &format_number(tracker.total_cache_read_tokens),
    ));
    lines.push(Line::from(""));

    // LLM 调用次数
    lines.push(label_value(
        "LLM 调用次数",
        &tracker.llm_call_count.to_string(),
    ));
    lines.push(Line::from(""));

    // 估算费用
    let cost = estimate_cost(app);
    lines.push(label_value("估算费用", &format!("${:.4}", cost)));
    lines.push(Line::from(""));

    // 当前模型
    lines.push(label_value("当前模型", &app.model_name));

    lines
}

fn build_context_lines(app: &App) -> Vec<Line<'static>> {
    let tracker = &app.agent.session_token_tracker;
    let context_window = app.agent.context_window;
    let mut lines: Vec<Line<'static>> = Vec::new();

    // 上下文窗口大小
    lines.push(label_value(
        "上下文窗口",
        &format_number(context_window as u64),
    ));
    lines.push(Line::from(""));

    // 已使用 Token
    let used = tracker.estimated_context_tokens().unwrap_or(0);
    lines.push(label_value("已使用 Token", &format_number(used)));

    // 使用率百分比
    let pct = tracker
        .context_usage_percent(context_window)
        .map(|p| format!("{:.1}%", p))
        .unwrap_or_else(|| "N/A".to_string());
    lines.push(label_value("使用率", &pct));
    lines.push(Line::from(""));

    // 消息数
    let msg_count = app.agent.agent_state_messages.len();
    lines.push(label_value("消息数", &msg_count.to_string()));

    // 工具调用次数
    lines.push(label_value(
        "工具调用次数",
        &app.agent.tool_call_count.to_string(),
    ));
    lines.push(Line::from(""));

    // Autocompact 阈值
    let compact_config = app.get_compact_config();
    let threshold_pct = (compact_config.auto_compact_threshold * 100.0) as u32;
    lines.push(label_value("Autocompact 阈值", &format!("{}%", threshold_pct)));

    lines
}

fn label_value(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<16}", label),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(
            value.to_string(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// 基于模型 alias 的简化费用估算
fn estimate_cost(app: &App) -> f64 {
    let tracker = &app.agent.session_token_tracker;
    let alias = app
        .zen_config
        .as_ref()
        .map(|c| c.config.active_alias.as_str())
        .unwrap_or("sonnet");

    let (input_price, output_price) = match alias {
        "opus" => (15.0, 75.0),
        "haiku" => (0.80, 4.0),
        _ => (3.0, 15.0), // sonnet default
    };

    let input_cost = (tracker.total_input_tokens as f64 / 1_000_000.0) * input_price;
    let output_cost = (tracker.total_output_tokens as f64 / 1_000_000.0) * output_price;
    input_cost + output_cost
}
