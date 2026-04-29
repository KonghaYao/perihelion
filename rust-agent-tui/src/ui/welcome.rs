//! Welcome Card — 空消息时显示品牌 Logo + 功能提示

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::ui::theme;

/// ASCII Art Logo（"PERIHELION"，6 行，宽度约 46 字符）
const LOGO: &[&str] = &[
    "███╗   ██╗███████╗██╗  ██╗",
    "████╗  ██║██╔════╝╚██╗██╔╝",
    "██╔██╗ ██║█████╗   ╚███╔╝ ",
    "██║╚██╗██║██╔══╝   ██╔██╗ ",
    "██║ ╚████║███████╗██╔╝ ██╗",
    "╚═╝  ╚═══╝╚══════╝╚═╝  ╚═╝",
];

/// 窄屏阈值：低于此宽度跳过 ASCII Art Logo
const NARROW_THRESHOLD: u16 = 50;

/// 渲染 Welcome Card（空消息时替代聊天区内容）
pub(crate) fn render_welcome(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let narrow = area.width < NARROW_THRESHOLD;

    // ── Logo 区域 ────────────────────────────────────────────────────────
    if narrow {
        // 窄屏：单行文字标题
        lines.push(Line::from(Span::styled(
            "Perihelion",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )));
    } else {
        // 宽屏：ASCII Art Logo
        lines.push(Line::from(""));
        for row in LOGO {
            lines.push(Line::from(Span::styled(
                row.to_string(),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )));
        }
    }

    // ── 副标题 ──────────────────────────────────────────────────────────
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Perihelion Agent Framework",
        Style::default().fg(theme::MUTED),
    )));

    // ── 分隔线 ──────────────────────────────────────────────────────────
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "────── What can I do? ──────",
        Style::default().fg(theme::DIM),
    )));

    // ── 功能亮点 ────────────────────────────────────────────────────────
    lines.push(Line::from(""));

    let features = [
        "Ask me to code, debug, or refactor",
        "Manage files and run terminal commands",
        "Delegate tasks to specialized sub-agents",
    ];

    for feat in &features {
        lines.push(Line::from(vec![
            Span::styled(" • ", Style::default().fg(theme::ACCENT)),
            Span::styled(*feat, Style::default().fg(theme::TEXT)),
        ]));
    }

    // ── 命令提示 ────────────────────────────────────────────────────────
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" /model", Style::default().fg(theme::WARNING)),
        Span::styled("  ", Style::default().fg(theme::MUTED)),
        Span::styled("/history", Style::default().fg(theme::WARNING)),
        Span::styled("  ", Style::default().fg(theme::MUTED)),
        Span::styled("/help", Style::default().fg(theme::WARNING)),
        Span::styled("  ", Style::default().fg(theme::MUTED)),
        Span::styled("/agents", Style::default().fg(theme::WARNING)),
    ]));

    // ── 首次使用引导（未配置 Provider 时显示）───────────────────────────
    let has_provider = app.zen_config.as_ref()
        .map(|c| !c.config.providers.is_empty())
        .unwrap_or(false);
    if !has_provider {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" ▶ ", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled("请输入 ", Style::default().fg(theme::TEXT)),
            Span::styled("/login", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(" 配置 API Key 开始使用", Style::default().fg(theme::TEXT)),
        ]));
    }

    // ── 快捷键提示 ──────────────────────────────────────────────────────
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" Esc", Style::default().fg(theme::DIM)),
        Span::styled(":Quit  ", Style::default().fg(theme::DIM)),
        Span::styled("Ctrl+C", Style::default().fg(theme::DIM)),
        Span::styled(":Stop  ", Style::default().fg(theme::DIM)),
        Span::styled("Alt+Enter", Style::default().fg(theme::DIM)),
        Span::styled(":NewLine  ", Style::default().fg(theme::DIM)),
        Span::styled("Shift+Tab", Style::default().fg(theme::DIM)),
        Span::styled(":Mode", Style::default().fg(theme::DIM)),
    ]));

    // ── 动态信息 ────────────────────────────────────────────────────────
    let skills_count = app.core.skills.len();
    if skills_count > 0 {
        lines.push(Line::from(vec![
            Span::styled(" #", Style::default().fg(theme::WARNING)),
            Span::styled(
                format!("{} skills available", skills_count),
                Style::default().fg(theme::TEXT),
            ),
        ]));
    }

    // ── 居中渲染 ────────────────────────────────────────────────────────
    let content_height = lines.len() as u16;
    let padding_top = area.height.saturating_sub(content_height) / 2;

    // 所有行水平居中
    let centered_lines: Vec<Line<'static>> = lines.into_iter().map(|l| l.centered()).collect();

    // 垂直居中：顶部填充空行
    let mut padded_lines: Vec<Line<'static>> = (0..padding_top)
        .map(|_| Line::from(""))
        .collect();
    padded_lines.extend(centered_lines);

    let paragraph = Paragraph::new(Text::from(padded_lines));

    f.render_widget(paragraph, area);
}
