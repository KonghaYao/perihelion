use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::{app::App, ui::theme};

/// 统一候选项：命令、Skill 或 Agent 命令，按名称排序后扁平展示
enum HintItem<'a> {
    Cmd { name: &'a str, desc: &'a str },
    Skill { name: &'a str, desc: &'a str },
    AgentCmd { name: &'a str },
}

const MAX_VIEWPORT: usize = 10;

/// 统一提示浮层：输入 / 前缀时展示命令和 Skills 候选（前缀匹配优先，再按字母顺序排列）
pub(crate) fn render_unified_hint(f: &mut Frame, app: &App, input_area: Rect) {
    let first_line = app
        .session_mgr
        .current()
        .ui
        .textarea
        .lines()
        .first()
        .map(|s| s.as_str())
        .unwrap_or("");
    if !first_line.starts_with('/') {
        return;
    }

    let prefix = first_line.trim_start_matches('/');
    let cmd_candidates: Vec<(String, String)> = app
        .session_mgr
        .current()
        .commands
        .command_registry
        .match_prefix(prefix, &app.services.lc);
    let skill_candidates: Vec<_> = app
        .session_mgr
        .current()
        .commands
        .skills
        .iter()
        .filter(|s| prefix.is_empty() || s.name.contains(prefix))
        .collect();
    let agent_cmd_candidates: Vec<_> = app
        .session_mgr
        .current()
        .commands
        .agent_commands
        .iter()
        .filter(|n| prefix.is_empty() || n.contains(prefix))
        .collect();

    // 合并排序：前缀匹配优先，再按名称字母序
    let mut items: Vec<HintItem<'_>> = Vec::new();
    for (name, desc) in &cmd_candidates {
        items.push(HintItem::Cmd {
            name: name.as_str(),
            desc: desc.as_str(),
        });
    }
    for skill in &skill_candidates {
        items.push(HintItem::Skill {
            name: &skill.name,
            desc: &skill.description,
        });
    }
    for name in &agent_cmd_candidates {
        items.push(HintItem::AgentCmd { name });
    }
    items.sort_by(|a, b| {
        let a_starts = a.name().starts_with(prefix) as u8;
        let b_starts = b.name().starts_with(prefix) as u8;
        // 前缀匹配优先 > 命令 > Skill > AgentCmd > 字母序
        b_starts
            .cmp(&a_starts)
            .then_with(|| b.rank().cmp(&a.rank()))
            .then_with(|| a.name().cmp(b.name()))
    });

    if items.is_empty() {
        return;
    }

    let total = items.len();
    let cursor = app.session_mgr.current().ui.hint_cursor;

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
                // 字符级切片防止 CJK panic
                let char_indices: Vec<(usize, char)> = name.char_indices().collect();
                let before_end = char_indices
                    .iter()
                    .position(|(i, _)| *i == pos)
                    .map(|p| char_indices[p].0)
                    .unwrap_or(pos);
                let matched_end = char_indices
                    .iter()
                    .skip_while(|(i, _)| *i < pos)
                    .take(prefix.chars().count())
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(pos + prefix.len());
                (
                    &name[..before_end],
                    Some(&name[pos..matched_end]),
                    &name[matched_end..],
                )
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
            HintItem::AgentCmd { name } => name,
        }
    }

    fn desc(&self) -> &'a str {
        match self {
            HintItem::Cmd { desc, .. } => desc,
            HintItem::Skill { desc, .. } => desc,
            HintItem::AgentCmd { .. } => "",
        }
    }

    /// 排序优先级：命令 > Skill > AgentCmd
    fn rank(&self) -> u8 {
        match self {
            HintItem::Cmd { .. } => 2,
            HintItem::Skill { .. } => 1,
            HintItem::AgentCmd { .. } => 0,
        }
    }
}
