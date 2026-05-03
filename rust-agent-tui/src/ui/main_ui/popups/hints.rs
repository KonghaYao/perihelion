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

/// 统一候选项：命令或 Skill，按名称排序后扁平展示
enum HintItem<'a> {
    Cmd { name: &'a str, desc: &'a str },
    Skill { name: &'a str, desc: &'a str },
}

const MAX_VIEWPORT: usize = 10;

/// 统一提示浮层：输入 / 前缀时展示命令和 Skills 候选（按字母顺序排列）
pub(crate) fn render_unified_hint(f: &mut Frame, app: &App, input_area: Rect) {
    let first_line = app.sessions[app.active]
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
    let cmd_candidates: Vec<(&str, &str)> = app.sessions[app.active].core.command_registry.match_prefix(prefix);
    let skill_candidates: Vec<_> = app.sessions[app.active]
        .core
        .skills
        .iter()
        .filter(|s| prefix.is_empty() || s.name.contains(prefix))
        .collect();

    // 合并并按名称排序（全量，不截断）
    let mut items: Vec<HintItem<'_>> = Vec::new();
    for (name, desc) in &cmd_candidates {
        items.push(HintItem::Cmd { name, desc });
    }
    for skill in &skill_candidates {
        items.push(HintItem::Skill {
            name: &skill.name,
            desc: &skill.description,
        });
    }
    items.sort_by(|a, b| a.name().cmp(b.name()));

    if items.is_empty() {
        return;
    }

    let total = items.len();
    let cursor = app.sessions[app.active].core.hint_cursor;

    // 计算视口：根据光标位置确定滚动偏移
    let viewport = MAX_VIEWPORT.min(total);
    let scroll_offset = if let Some(cur) = cursor {
        // 确保光标在视口内
        if cur < viewport {
            0
        } else {
            cur - viewport + 1
        }
    } else {
        0
    };
    let visible_items = &items[scroll_offset..scroll_offset + viewport];

    let hint_height = viewport as u16 + 2; // 视口行数 + 边框
    let y = input_area.y.saturating_sub(hint_height);
    let hint_area = Rect {
        x: input_area.x,
        y,
        width: input_area.width,
        height: hint_height,
    };

    let inner = BorderedPanel::new(Span::styled("", Style::default()))
        .border_style(Style::default().fg(theme::BORDER))
        .render(f, hint_area);

    let max_name_width = visible_items
        .iter()
        .map(|it| unicode_width::UnicodeWidthStr::width(it.name()))
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line> = Vec::new();

    for (vi, item) in visible_items.iter().enumerate() {
        let global_idx = scroll_offset + vi;
        let is_selected = cursor == Some(global_idx);
        let name = item.name();

        // 高亮匹配前缀部分
        let highlight = if !prefix.is_empty() {
            name.find(prefix).map(|pos| {
                let before = &name[..pos];
                let matched = &name[pos..pos + prefix.len()];
                let after = &name[pos + prefix.len()..];
                (before, Some(matched), after)
            })
        } else {
            None
        };

        let (before, matched, after) = match highlight {
            Some(bma) => bma,
            None => (name, None, ""),
        };

        let mut spans = vec![Span::styled(
            if is_selected { "❯ /" } else { "  /" },
            Style::default().fg(theme::THINKING),
        )];

        if let Some(m) = matched {
            spans.push(Span::styled(
                before.to_string(),
                Style::default().fg(if is_selected {
                    theme::THINKING
                } else {
                    theme::TEXT
                }),
            ));
            spans.push(Span::styled(
                m.to_string(),
                Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                after.to_string(),
                Style::default().fg(if is_selected {
                    theme::THINKING
                } else {
                    theme::TEXT
                }),
            ));
        } else {
            spans.push(Span::styled(
                name.to_string(),
                Style::default().fg(if is_selected {
                    theme::THINKING
                } else {
                    theme::TEXT
                }),
            ));
        }

        let name_display_width = unicode_width::UnicodeWidthStr::width(name);
        let padding = max_name_width - name_display_width + 2;
        spans.push(Span::styled(" ".repeat(padding), Style::default()));
        spans.push(Span::styled(
            item.desc().to_string(),
            Style::default().fg(theme::MUTED),
        ));

        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

impl<'a> HintItem<'a> {
    fn name(&self) -> &'a str {
        match self {
            HintItem::Cmd { name, .. } => name,
            HintItem::Skill { name, .. } => name,
        }
    }

    fn desc(&self) -> &'a str {
        match self {
            HintItem::Cmd { desc, .. } => desc,
            HintItem::Skill { desc, .. } => desc,
        }
    }
}
