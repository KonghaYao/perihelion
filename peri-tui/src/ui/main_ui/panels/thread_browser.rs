use chrono::Utc;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::App,
    thread::ThreadBrowser,
    ui::{main_ui::highlight_line_spans, theme},
};

/// 搜索框 + 空行占用的固定高度
const SEARCH_OVERHEAD: u16 = 4; // 3 行搜索框 + 1 行空行

// Keep ThreadBrowser import for render function signature

fn truncate_display(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return s.to_string();
    }
    let target = max_width.saturating_sub(1);
    let mut cum = 0;
    for (i, c) in s.char_indices() {
        let cw = c.width().unwrap_or(0);
        if cum + cw > target {
            return format!("{}…", &s[..i]);
        }
        cum += cw;
    }
    s.to_string()
}

/// 格式化内容大小为人类可读字符串
fn format_content_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else if bytes > 0 {
        format!("{}B", bytes)
    } else {
        String::new()
    }
}

/// 格式化相对时间
fn format_relative_time(dt: &chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);
    let secs = diff.num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!(
            "{} minute{} ago",
            secs / 60,
            if secs / 60 > 1 { "s" } else { "" }
        )
    } else if secs < 86400 {
        format!(
            "{} hour{} ago",
            secs / 3600,
            if secs / 3600 > 1 { "s" } else { "" }
        )
    } else {
        let days = secs / 86400;
        format!("{} day{} ago", days, if days > 1 { "s" } else { "" })
    }
}

/// 渲染搜索框到固定区域（不参与滚动）
fn render_search_box(f: &mut Frame, browser: &ThreadBrowser, area: Rect) {
    if area.width < 4 || area.height < 3 {
        return;
    }

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if browser.search_focused {
            theme::ACCENT
        } else {
            theme::DIM
        }));

    let search_inner = search_block.inner(area);

    let query_val = browser.search_query.value();
    let content_line = if query_val.is_empty() && !browser.search_focused {
        Line::from(vec![
            Span::styled(" ⌕ ", Style::default().fg(theme::MUTED)),
            Span::styled("Search…", Style::default().fg(theme::DIM)),
        ])
    } else {
        let mut spans = vec![
            Span::styled(" ⌕ ", Style::default().fg(theme::MUTED)),
            Span::styled(
                browser.search_query.display_text('•'),
                Style::default().fg(theme::TEXT),
            ),
        ];
        if browser.search_focused {
            spans.push(Span::styled("█", Style::default().fg(theme::TEXT)));
        }
        Line::from(spans)
    };

    let search_para = Paragraph::new(content_line);
    f.render_widget(search_block, area);
    f.render_widget(search_para, search_inner);
}

/// Thread 浏览面板（底部展开区）
pub(crate) fn render_thread_browser(
    f: &mut Frame,
    browser: &ThreadBrowser,
    app: &mut App,
    area: Rect,
) {
    let current_thread_id = app.session_mgr.current_mut().current_thread_id.clone();

    let popup_area = area;

    // BorderedPanel 标题
    let total = browser.total();
    let total_all = browser.total_all();
    let cursor_display = if total == 0 { 0 } else { browser.cursor + 1 };
    let title_text = format!(" Resume Session ({}/{}) ", cursor_display, total_all);

    let inner = BorderedPanel::new(Span::styled(
        title_text,
        Style::default()
            .fg(theme::SELECTED_FG)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::MUTED))
    .render(f, popup_area);

    // ── 布局拆分：搜索框（固定） + 列表区域（可滚动） ──
    let search_area = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: 3,
    };
    let list_area = Rect {
        x: inner.x,
        y: inner.y + SEARCH_OVERHEAD,
        width: inner.width,
        height: inner.height.saturating_sub(SEARCH_OVERHEAD),
    };

    // ── 1. 渲染搜索框（固定位置，不滚动） ──
    render_search_box(f, browser, search_area);

    // ── 2. 构建列表内容（纯 thread 列表 + 快捷键，无搜索框占位） ──
    let mut lines: Vec<Line> = Vec::new();

    let filtered = browser.filtered_threads();
    let max_title_width = list_area.width.saturating_sub(6) as usize;

    if filtered.is_empty() {
        if browser.search_query.value().is_empty() {
            lines.push(Line::from(Span::styled(
                "  (No conversations yet)",
                Style::default().fg(theme::MUTED),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  (No matching conversations)",
                Style::default().fg(theme::MUTED),
            )));
        }
        lines.push(Line::from(""));
    }

    let current_thread_id = current_thread_id.as_ref();
    for (i, meta) in filtered.iter().enumerate() {
        let is_cursor = i == browser.cursor;
        let is_current = current_thread_id == Some(&meta.id);
        let title = meta.title.as_deref().unwrap_or("(untitled)");
        let label = truncate_display(title, max_title_width);

        // 第一行：cursor indicator + 标题
        let cursor_span = Span::styled(
            if is_cursor { "❯ " } else { "  " },
            Style::default().fg(if is_cursor {
                theme::SELECTED_FG
            } else {
                theme::MUTED
            }),
        );

        let current_tag = if is_current { "✓ " } else { "" };

        let title_style = if is_cursor {
            Style::default()
                .fg(theme::SELECTED_FG)
                .add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default()
                .fg(theme::SELECTED_FG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let mut first_line_spans = vec![cursor_span];
        if is_current {
            first_line_spans.push(Span::styled(
                current_tag.to_string(),
                Style::default().fg(theme::SAGE),
            ));
        }
        first_line_spans.push(Span::styled(label, title_style));
        lines.push(Line::from(first_line_spans));

        // 第二行：metadata（relative time · branch · size）
        let relative_time = format_relative_time(&meta.updated_at);
        let size_str = format_content_size(meta.content_size);

        let mut meta_parts = vec![Span::styled(
            format!("   {}", relative_time),
            Style::default().fg(theme::MUTED),
        )];

        if let Some(branch) = &browser.branch {
            meta_parts.push(Span::styled(
                format!(" · {}", branch),
                Style::default().fg(theme::MUTED),
            ));
        }

        if !size_str.is_empty() {
            meta_parts.push(Span::styled(
                format!(" · {}", size_str),
                Style::default().fg(theme::MUTED),
            ));
        }

        lines.push(Line::from(meta_parts));

        // 空行分隔
        lines.push(Line::from(""));
    }

    // 存储面板元数据供鼠标选区使用（仅列表区域）
    let scroll_offset = browser.scroll_offset;
    let panel_selection_active = app.session_mgr.current_mut().ui.panel_selection.is_active();
    let panel_selection = app.session_mgr.current_mut().ui.panel_selection.clone();

    app.session_mgr.current_mut().ui.panel_area = Some(list_area);
    app.session_mgr.current_mut().ui.panel_scroll_offset = scroll_offset;
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    // 应用面板选区高亮
    if panel_selection_active {
        let sel = &panel_selection;
        if let (Some(start), Some(end)) = (sel.start, sel.end) {
            let ((sr, sc), (er, ec)) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let scroll = app.session_mgr.current_mut().ui.panel_scroll_offset as usize;
            let visible_start = scroll;
            let visible_end = scroll + list_area.height as usize;
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

    // ── 3. 渲染列表区域（可滚动） ──
    let mut scroll_state = ScrollState::with_offset(browser.scroll_offset);
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, list_area, &mut scroll_state);
}
