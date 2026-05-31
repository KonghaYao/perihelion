use crate::app::App;
use crate::git::status::FileStatus;
use peri_widgets::Theme;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use unicode_width::UnicodeWidthStr;

/// status 面板按钮类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusButton {
    /// [+] 暂存文件/目录（git add）
    Stage,
    /// [-] 取消暂存文件/目录（git restore --staged）
    Unstage,
    /// [x] 删除文件（git rm / rm）
    Delete,
}

/// 记录面板中目录行和按钮行位置（用于点击检测）
#[derive(Default)]
pub struct PanelLayout {
    /// 目录行：(相对行号, 展开标识路径)
    pub dir_rows: Vec<(u16, String)>,
    /// 按钮行：(相对行号, 按钮起始列 x, 按钮类型, 操作路径)
    pub button_rows: Vec<(u16, u16, StatusButton, String)>,
    /// 路径行：(相对行号, 完整的相对路径, 是否为目录)
    pub path_rows: Vec<(u16, String, bool)>,
}

enum TreeNode {
    Dir {
        name: String,
        children: BTreeMap<String, TreeNode>,
    },
    File {
        name: String,
        status: FileStatus,
    },
}

enum FlatNode {
    Dir {
        display_path: String,
        children: Vec<FlatNode>,
    },
    File {
        name: String,
        status: FileStatus,
    },
}

fn build_tree(entries: &[crate::git::status::StatusEntry]) -> Vec<FlatNode> {
    let mut root: BTreeMap<String, TreeNode> = BTreeMap::new();
    for entry in entries {
        let parts: Vec<&str> = entry.path.split('/').collect();
        insert_into_tree(&mut root, &parts, 0, entry.status);
    }
    flatten_and_merge(&root)
}

fn insert_into_tree(
    map: &mut BTreeMap<String, TreeNode>,
    parts: &[&str],
    depth: usize,
    status: FileStatus,
) {
    if depth == parts.len() - 1 {
        let name = parts[depth].to_string();
        map.insert(name.clone(), TreeNode::File { name, status });
    } else {
        let dir_name = parts[depth].to_string();
        let child = map
            .entry(dir_name.clone())
            .or_insert_with(|| TreeNode::Dir {
                name: dir_name,
                children: BTreeMap::new(),
            });
        if let TreeNode::Dir { children, .. } = child {
            insert_into_tree(children, parts, depth + 1, status);
        }
    }
}

fn flatten_and_merge(nodes: &BTreeMap<String, TreeNode>) -> Vec<FlatNode> {
    let mut result = Vec::new();
    for node in nodes.values() {
        match node {
            TreeNode::Dir { name, children } => result.push(compress_dir(name, children)),
            TreeNode::File { name, status } => result.push(FlatNode::File {
                name: name.clone(),
                status: *status,
            }),
        }
    }
    result
}

fn compress_dir(name: &str, children: &BTreeMap<String, TreeNode>) -> FlatNode {
    let mut display_path = format!("{}/", name);
    let mut cur = children;
    loop {
        let dirs: Vec<_> = cur
            .values()
            .filter(|n| matches!(n, TreeNode::Dir { .. }))
            .collect();
        let files: Vec<_> = cur
            .values()
            .filter(|n| matches!(n, TreeNode::File { .. }))
            .collect();
        if dirs.len() == 1 && files.is_empty() {
            if let Some(TreeNode::Dir {
                name: sub,
                children: sub_ch,
            }) = dirs.into_iter().next()
            {
                display_path = format!("{}{}/", display_path, sub);
                cur = sub_ch;
                continue;
            }
        }
        break;
    }
    FlatNode::Dir {
        display_path,
        children: flatten_and_merge(cur),
    }
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

fn truncate_spans(spans: &mut Vec<Span<'static>>, max: usize) {
    if spans.is_empty() {
        return;
    }
    while spans_width(spans) > max {
        let idx = spans.len() - 1;
        let s = spans[idx].content.clone().into_owned();
        if s.is_empty() {
            break;
        }
        let has_el = s.ends_with('…');
        let rm = if has_el { 2 } else { 1 };
        let chars: Vec<char> = s.chars().collect();
        let nl = chars.len().saturating_sub(rm);
        if nl == 0 {
            break;
        }
        spans[idx].content = Cow::Owned(format!("{}…", chars[..nl].iter().collect::<String>()));
    }
}

/// 按钮区域宽度：空格 + 按钮字符 = 2 列
const BTN_W: u16 = 2;

/// 截断文本、填充空格、追加徽章按钮到行尾，返回按钮起始列列表
fn append_buttons(
    spans: &mut Vec<Span<'static>>,
    buttons: &[StatusButton],
    width: u16,
) -> Vec<(u16, StatusButton)> {
    let total_btn_width = BTN_W * buttons.len() as u16;
    let btn_x = width.saturating_sub(total_btn_width) as usize;
    truncate_spans(spans, btn_x);
    let cur = spans_width(spans);
    let pad = btn_x.saturating_sub(cur);
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    let mut result = Vec::new();
    for btn in buttons {
        spans.push(Span::raw(" "));
        let (ch, style) = match btn {
            StatusButton::Stage => (
                "+",
                Style::default()
                    .fg(ratatui::style::Color::White)
                    .bg(ratatui::style::Color::Rgb(40, 80, 40)),
            ),
            StatusButton::Unstage => (
                "-",
                Style::default()
                    .fg(ratatui::style::Color::White)
                    .bg(ratatui::style::Color::Rgb(140, 110, 20)),
            ),
            StatusButton::Delete => (
                "x",
                Style::default()
                    .fg(ratatui::style::Color::White)
                    .bg(ratatui::style::Color::Rgb(140, 40, 40)),
            ),
        };
        let start_x = spans_width(spans) as u16;
        spans.push(Span::styled(ch.to_string(), style));
        result.push((start_x, *btn));
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn render_tree(
    nodes: &[FlatNode],
    depth: usize,
    prefix: &str,
    collapsed: &HashSet<String>,
    dir_rows: &mut Vec<(u16, String)>,
    button_rows: &mut Vec<(u16, u16, StatusButton, String)>,
    path_rows: &mut Vec<(u16, String, bool)>,
    row: &mut u16,
    lines: &mut Vec<Line<'static>>,
    theme: &crate::theme::GigTheme,
    section: &[StatusButton],
    area_width: u16,
    highlight_path: Option<&str>,
) {
    for node in nodes {
        match node {
            FlatNode::Dir {
                display_path,
                children,
            } => {
                let key = format!("{}{}", prefix, display_path);
                let is_expanded = !collapsed.contains(&key);
                let marker = if is_expanded { "▾" } else { "▸" };
                let mut spans = indent_spans(depth, theme);
                spans.push(Span::styled(
                    format!("{} {}", marker, display_path),
                    Style::default().fg(theme.text()),
                ));
                dir_rows.push((*row, key.clone()));
                path_rows.push((*row, key.trim_end_matches('/').to_string(), true));
                let btns = append_buttons(&mut spans, section, area_width);
                for (bx, btn) in btns {
                    button_rows.push((*row, bx, btn, key.clone()));
                }
                lines.push(Line::from(spans));
                *row += 1;
                if is_expanded {
                    render_tree(
                        children,
                        depth + 1,
                        &key,
                        collapsed,
                        dir_rows,
                        button_rows,
                        path_rows,
                        row,
                        lines,
                        theme,
                        section,
                        area_width,
                        highlight_path,
                    );
                }
            }
            FlatNode::File { name, status } => {
                let (ch, color) = status_style(*status, theme);
                let mut spans = indent_spans(depth, theme);
                spans.push(Span::styled(
                    name.clone(),
                    Style::default().fg(theme.muted()),
                ));
                spans.push(Span::styled(format!(" {}", ch), Style::default().fg(color)));
                let fk = format!("{}{}", prefix, name);
                // 高亮选中的文件
                let should_highlight = highlight_path == Some(fk.as_str());
                if should_highlight {
                    spans = theme.highlight_line(spans);
                }
                path_rows.push((*row, fk.clone(), false));
                let btns = append_buttons(&mut spans, section, area_width);
                for (bx, btn) in btns {
                    button_rows.push((*row, bx, btn, fk.clone()));
                }
                lines.push(Line::from(spans));
                *row += 1;
            }
        }
    }
}

fn indent_spans(depth: usize, theme: &crate::theme::GigTheme) -> Vec<Span<'static>> {
    (0..depth)
        .map(|_| Span::styled("│ ".to_string(), Style::default().fg(theme.dim())))
        .collect()
}

fn status_style(
    status: FileStatus,
    theme: &crate::theme::GigTheme,
) -> (char, ratatui::style::Color) {
    match status {
        FileStatus::New | FileStatus::Renamed => ('A', theme.status_added()),
        FileStatus::Deleted | FileStatus::WorkingDeleted => ('D', theme.status_deleted()),
        FileStatus::Modified | FileStatus::WorkingModified => ('M', theme.status_modified()),
        _ => ('?', theme.dim()),
    }
}

/// 渲染单个面板内容（带边框），返回 (inner Rect, Option<PanelLayout>)
/// `scroll` / `total_lines` / `viewport` 传入面板各自的滚动状态引用
#[allow(clippy::too_many_arguments)]
fn draw_panel(
    f: &mut Frame,
    area: Rect,
    title: &str,
    entries: &[crate::git::status::StatusEntry],
    expanded: bool,
    section: &[StatusButton],
    collapsed: &HashSet<String>,
    theme: &crate::theme::GigTheme,
    scroll: &mut u16,
    total_lines: &mut u16,
    viewport: &mut u16,
    highlight_path: Option<&str>,
) -> (Rect, Option<PanelLayout>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(theme.dim()))
        .border_style(Style::default().fg(theme.border()));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    if inner.height == 0 || entries.is_empty() {
        *total_lines = 0;
        *viewport = inner.height;
        return (inner, None);
    }

    let mut lines: Vec<Line<'_>> = Vec::new();
    let mut layout = PanelLayout::default();
    let mut row: u16 = 0;

    if expanded {
        let tree = build_tree(entries);
        // 为 scrollbar 预留 1 列
        let content_width = inner.width.saturating_sub(1);
        render_tree(
            &tree,
            0,
            "",
            collapsed,
            &mut layout.dir_rows,
            &mut layout.button_rows,
            &mut layout.path_rows,
            &mut row,
            &mut lines,
            theme,
            section,
            content_width,
            highlight_path,
        );
    }

    *total_lines = lines.len() as u16;
    *viewport = inner.height;

    // clamp scroll：防止内容变少后 scroll 超出范围
    let max_scroll = total_lines.saturating_sub(*viewport);
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }

    // 按滚动偏移裁剪
    let s = *scroll as usize;
    let v = inner.height as usize;
    let visible: Vec<Line<'_>> = lines.into_iter().skip(s).take(v).collect();

    // 修正 layout 中的行号（减去 scroll 偏移）
    let scroll_u16 = *scroll;
    for (r, _) in &mut layout.dir_rows {
        *r = r.saturating_sub(scroll_u16);
    }
    for (r, _, _, _) in &mut layout.button_rows {
        *r = r.saturating_sub(scroll_u16);
    }
    for (r, _, _) in &mut layout.path_rows {
        *r = r.saturating_sub(scroll_u16);
    }

    let para = Paragraph::new(visible);
    f.render_widget(para, inner);

    // 渲染 scrollbar
    if *total_lines > *viewport {
        use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
        let scrollbar_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y,
            1,
            inner.height,
        );
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(*scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
    (inner, Some(layout))
}

/// 计算当前需要高亮的文件路径
fn compute_highlight_path(app: &App, sub: crate::app::StatusSubPanel) -> Option<String> {
    // 如果正在预览某文件，高亮该文件
    if let Some((ref path, _)) = app.preview_file {
        return Some(path.clone());
    }
    // 如果焦点在 Status，高亮当前光标位置的文件
    if app.focus == crate::app::Focus::Status && app.status_sub_panel == sub {
        let files = match sub {
            crate::app::StatusSubPanel::Staged => &app.staged_visible_files,
            crate::app::StatusSubPanel::Changes => &app.changes_visible_files,
        };
        return files.get(app.status_file_index).cloned();
    }
    None
}

/// 渲染 Staged 面板
pub fn draw_staged(f: &mut Frame, area: Rect, app: &mut App) -> (Rect, Option<PanelLayout>) {
    let status = &app.git_status;
    let title = format!("Staged ({})", status.staged.len());
    if status.staged.is_empty() {
        let theme = &app.theme;
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", title))
            .title_style(Style::default().fg(theme.dim()))
            .border_style(Style::default().fg(theme.border()));
        f.render_widget(block, area);
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        app.staged_total_lines = 0;
        app.staged_viewport = inner.height;
        app.staged_visible_files.clear();
        return (inner, None);
    }
    let highlight = compute_highlight_path(app, crate::app::StatusSubPanel::Staged);
    let (inner, layout) = draw_panel(
        f,
        area,
        &title,
        &status.staged,
        app.status_staged_expanded,
        &[StatusButton::Unstage],
        &app.status_dir_collapsed,
        &app.theme,
        &mut app.staged_scroll,
        &mut app.staged_total_lines,
        &mut app.staged_viewport,
        highlight.as_deref(),
    );
    // 提取可见文件路径列表
    if let Some(ref l) = layout {
        app.staged_visible_files = l
            .path_rows
            .iter()
            .filter(|(_, _, is_dir)| !is_dir)
            .map(|(_, p, _)| p.clone())
            .collect();
    } else {
        app.staged_visible_files.clear();
    }
    // clamp cursor
    if app.status_sub_panel == crate::app::StatusSubPanel::Staged
        && app.status_file_index >= app.staged_visible_files.len()
        && !app.staged_visible_files.is_empty()
    {
        app.status_file_index = app.staged_visible_files.len().saturating_sub(1);
    }
    (inner, layout)
}

/// 渲染 Changes 面板（unstaged + untracked）
pub fn draw_changes(f: &mut Frame, area: Rect, app: &mut App) -> (Rect, Option<PanelLayout>) {
    let status = &app.git_status;
    let total = status.unstaged.len() + status.untracked.len();
    let title = format!("Changes ({})", total);
    if total == 0 {
        let theme = &app.theme;
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", title))
            .title_style(Style::default().fg(theme.dim()))
            .border_style(Style::default().fg(theme.border()));
        f.render_widget(block, area);
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        app.changes_total_lines = 0;
        app.changes_viewport = inner.height;
        app.changes_visible_files.clear();
        return (inner, None);
    }
    let mut all = status.unstaged.clone();
    all.extend(status.untracked.clone());
    let highlight = compute_highlight_path(app, crate::app::StatusSubPanel::Changes);
    let (inner, layout) = draw_panel(
        f,
        area,
        &title,
        &all,
        app.status_unstaged_expanded,
        &[StatusButton::Stage, StatusButton::Delete],
        &app.status_dir_collapsed,
        &app.theme,
        &mut app.changes_scroll,
        &mut app.changes_total_lines,
        &mut app.changes_viewport,
        highlight.as_deref(),
    );
    // 提取可见文件路径列表
    if let Some(ref l) = layout {
        app.changes_visible_files = l
            .path_rows
            .iter()
            .filter(|(_, _, is_dir)| !is_dir)
            .map(|(_, p, _)| p.clone())
            .collect();
    } else {
        app.changes_visible_files.clear();
    }
    // clamp cursor
    if app.status_sub_panel == crate::app::StatusSubPanel::Changes
        && app.status_file_index >= app.changes_visible_files.len()
        && !app.changes_visible_files.is_empty()
    {
        app.status_file_index = app.changes_visible_files.len().saturating_sub(1);
    }
    (inner, layout)
}
