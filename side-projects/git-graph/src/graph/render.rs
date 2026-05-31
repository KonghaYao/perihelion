use super::color::BranchColors;
use super::layout::{CellType, GraphRow};
use crate::theme::GigTheme;
use git2::Oid;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

pub fn render_graph_row(
    row: &GraphRow,
    graph_width: u16,
    is_selected: bool,
    head_oid: Option<Oid>,
    colors: &BranchColors,
    theme: &GigTheme,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // 渲染 graph cells（每个 cell 统一 2 字符宽度，保持对齐）
    for cell in &row.cells {
        let (ch, color) = cell_to_char(cell);
        let (ch2, c2) = cell_second_char(cell, color);
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        spans.push(Span::styled(ch2.to_string(), Style::default().fg(c2)));
    }

    // Commit 信息紧跟 cells 后面（不再先填充）
    if let Some(oid) = row.oid {
        spans.push(Span::raw(" "));

        // branch 标记（背景色与分支颜色一致）
        if !row.branches.is_empty() {
            for branch in &row.branches {
                let bg = colors.get(branch).unwrap_or(Color::Magenta);
                spans.push(Span::styled(
                    format!(" {} ", branch),
                    Style::default()
                        .fg(Color::White)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
            }
        }

        // remote branch 标记（灰色小字）
        if !row.remote_branches.is_empty() {
            for remote_branch in &row.remote_branches {
                spans.push(Span::styled(
                    format!(" {} ", remote_branch),
                    Style::default()
                        .fg(Color::Rgb(160, 160, 160))
                        .bg(Color::Rgb(40, 40, 50)),
                ));
                spans.push(Span::raw(" "));
            }
        }

        // tag 标记
        if !row.tags.is_empty() {
            for tag in &row.tags {
                spans.push(Span::styled(
                    format!(" {} ", tag),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
            }
        }

        // HEAD 标记
        if head_oid == Some(oid) {
            spans.push(Span::styled(
                "HEAD→ ".to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // stash 标记
        if row.has_stash {
            spans.push(Span::styled(
                "📦 ".to_string(),
                Style::default().fg(Color::Yellow),
            ));
        }

        // commit message（单行）
        if !row.message_short.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                row.message_short.clone(),
                Style::default().fg(Color::White),
            ));
        }
    }

    // 最后填充到 graph_width（保证整行背景色一致）
    let used: usize = spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    if used < graph_width as usize {
        spans.push(Span::raw(" ".repeat(graph_width as usize - used)));
    }

    if is_selected {
        let bg = theme.selected_bg();
        spans = spans
            .into_iter()
            .map(|span| {
                let new_style = span.style.patch(Style::default().bg(bg));
                Span::styled(span.content, new_style)
            })
            .collect();
    }

    Line::from(spans)
}

fn cell_to_char(cell: &CellType) -> (char, Color) {
    match cell {
        CellType::Empty => (' ', Color::Reset),
        CellType::Pipe(c) => ('│', *c),
        CellType::Commit(c) => ('◉', *c),
        CellType::BranchRight(c) => ('╭', *c),
        CellType::BranchLeft(c) => ('╮', *c),
        CellType::MergeRight(c) => ('╰', *c),
        CellType::MergeLeft(c) => ('╯', *c),
        CellType::Horizontal(c) => ('─', *c),
        CellType::TeeRight(c) => ('├', *c),
        CellType::TeeLeft(c) => ('┤', *c),
    }
}

/// 每个 cell 的第二个字符，保证统一 2 字符宽度对齐
fn cell_second_char(cell: &CellType, color: Color) -> (char, Color) {
    match cell {
        CellType::Empty => (' ', Color::Reset),
        CellType::Pipe(_) => (' ', color),
        CellType::Commit(_) => (' ', color),
        CellType::Horizontal(_) => ('─', color),
        // 转角和 T 字右侧补水平线，保持连接连续
        CellType::BranchRight(_) | CellType::MergeRight(_) | CellType::TeeRight(_) => ('─', color),
        // 转角和 T 字左侧已经是连接终点，补空格
        CellType::BranchLeft(_) | CellType::MergeLeft(_) | CellType::TeeLeft(_) => (' ', color),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::layout::CellType;
    use git2::Oid;

    fn oid(n: u8) -> Oid {
        let mut bytes = [0u8; 20];
        bytes[0] = n;
        Oid::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn test_render_commit_row() {
        let row = GraphRow {
            oid: Some(oid(1)),
            lane: 0,
            cells: vec![CellType::Commit(Color::Green)],
            branch: Some("main".to_string()),
            branches: vec!["main".to_string()],
            message_short: "initial commit".to_string(),
            has_stash: false,
            tags: Vec::new(),
            remote_branches: Vec::new(),
        };
        let theme = GigTheme::new();
        let line = render_graph_row(&row, 40, false, Some(oid(1)), &BranchColors::new(), &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains('◉'));
        assert!(text.contains("main"));
        assert!(text.contains("HEAD→"));
        assert!(text.contains("initial commit"));
        assert!(!text.contains("00000001"), "不应显示 hash");
    }

    #[test]
    fn test_render_selected_row() {
        let row = GraphRow {
            oid: Some(oid(1)),
            lane: 0,
            cells: vec![CellType::Commit(Color::Green)],
            branch: None,
            branches: Vec::new(),
            message_short: "test".to_string(),
            has_stash: false,
            tags: Vec::new(),
            remote_branches: Vec::new(),
        };
        let theme = GigTheme::new();
        let line = render_graph_row(&row, 20, true, None, &BranchColors::new(), &theme);
        assert!(line.spans.iter().all(|s| s.style.bg.is_some()));
    }

    #[test]
    fn test_render_connector_row() {
        let row = GraphRow {
            oid: None,
            lane: 0,
            cells: vec![
                CellType::TeeRight(Color::Red),
                CellType::Horizontal(Color::Red),
                CellType::MergeLeft(Color::Red),
            ],
            branch: None,
            branches: Vec::new(),
            message_short: String::new(),
            has_stash: false,
            tags: Vec::new(),
            remote_branches: Vec::new(),
        };
        let theme = GigTheme::new();
        let line = render_graph_row(&row, 20, false, None, &BranchColors::new(), &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains('├'));
        assert!(text.contains('─'));
        assert!(text.contains('╯'));
    }
}
