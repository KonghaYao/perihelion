//! 编辑器渲染：直接写入 ratatui Buffer，支持软折行（soft wrap）。
//!
//! 布局：`| gutter | │ | content area | scrollbar |`
//! 长行在 content area 右边界自动折行，一行逻辑行可占多个视觉行。
//! 光标最后绘制（反色叠加），确保始终可见。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{char_width, TextEditor, MAX_DISPLAY_COLS};

// ── 配色常量 ──

const GUTTER_FG: Color = Color::Rgb(100, 100, 100);
const GUTTER_BG: Color = Color::Rgb(30, 30, 40);
const SEPARATOR_FG: Color = Color::Rgb(60, 60, 70);
const CURRENT_LINE_BG: Color = Color::Rgb(30, 30, 45);
const SELECTION_BG: Color = Color::Rgb(40, 60, 100);
const CURSOR_BG: Color = Color::Rgb(200, 200, 200);
const STATUS_FG: Color = Color::Rgb(100, 100, 110);
const DEFAULT_BG: Color = Color::Rgb(25, 25, 32);

// ── Visual Row 映射 ──

/// 一个视觉行的信息。
struct VisualRow {
    /// 逻辑行号（0-based）
    logical_line: usize,
    /// 本视觉行在该逻辑行中的显示列起始偏移
    col_offset: usize,
    /// 是否是逻辑行的第一个视觉行（用于显示行号）
    is_first: bool,
}

/// 构建可见视觉行映射。
///
/// 从 `scroll_y`（逻辑行）开始，将每个逻辑行按 `content_width` 拆分为多个视觉行，
/// 直到填满 `max_rows` 行。
fn build_visual_rows(
    editor: &TextEditor,
    scroll_y: usize,
    max_rows: usize,
    content_width: usize,
) -> Vec<VisualRow> {
    let mut rows = Vec::with_capacity(max_rows);
    let total_lines = editor.line_count();

    for logical_line in scroll_y..total_lines {
        if rows.len() >= max_rows {
            break;
        }
        let line_width = editor.line_display_width(logical_line);
        if line_width == 0 {
            // 空行占一个视觉行
            rows.push(VisualRow {
                logical_line,
                col_offset: 0,
                is_first: true,
            });
        } else {
            let wrap_count = line_width.div_ceil(content_width);
            for wrap_idx in 0..wrap_count {
                if rows.len() >= max_rows {
                    break;
                }
                rows.push(VisualRow {
                    logical_line,
                    col_offset: wrap_idx * content_width,
                    is_first: wrap_idx == 0,
                });
            }
        }
    }
    rows
}

// ── 公共 API ──

/// 根据行数计算 gutter 宽度（行号位数 + 1 空格，最小 3）。
pub fn gutter_width(line_count: usize) -> u16 {
    let digits = if line_count <= 1 {
        1
    } else {
        line_count.ilog10() as u16 + 1
    };
    (digits + 1).max(3)
}

/// 主渲染入口：将编辑器内容写入 Buffer 的指定区域。
///
/// area 最后一行预留给 status bar，其余给编辑内容。
pub fn render_to_buffer(editor: &TextEditor, buf: &mut Buffer, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }

    let gw = gutter_width(editor.line_count());
    let content_width = area.width.saturating_sub(gw + 1 + 1); // gutter + sep + scrollbar
    let content_height = area.height.saturating_sub(1); // status bar
    let content_area = Rect {
        x: area.x + gw + 1,
        y: area.y,
        width: content_width,
        height: content_height,
    };
    let status_y = area.y + area.height.saturating_sub(1);

    // 构建视觉行映射
    let visual_rows = build_visual_rows(
        editor,
        editor.scroll_y(),
        content_height as usize,
        content_width as usize,
    );

    render_gutter(editor, buf, area, gw, &visual_rows);
    render_separator(buf, area, gw, content_height);
    render_content(editor, buf, content_area, &visual_rows);
    render_scrollbar(editor, buf, area, content_height, &visual_rows);
    render_cursor(editor, buf, content_area, &visual_rows);
    render_status_bar(editor, buf, area.x, status_y, area.width);
}

// ── Gutter ──

/// 渲染行号区域：只在每个逻辑行的第一个视觉行显示行号。
fn render_gutter(
    editor: &TextEditor,
    buf: &mut Buffer,
    area: Rect,
    gw: u16,
    visual_rows: &[VisualRow],
) {
    let cursor_line = editor.cursor().line;

    for (row_idx, vr) in visual_rows.iter().enumerate() {
        let y = area.y + row_idx as u16;

        // 整行填充 gutter 背景
        for x in 0..gw {
            set_cell(
                buf,
                area.x + x,
                y,
                ' ',
                Style::default().fg(GUTTER_FG).bg(GUTTER_BG),
            );
        }

        if vr.is_first {
            let line_num = vr.logical_line + 1;
            let num_str = line_num.to_string();
            let start_x = area.x + gw - 1 - num_str.len() as u16;
            let is_current = vr.logical_line == cursor_line;
            let style = if is_current {
                Style::default().fg(Color::Rgb(180, 180, 180)).bg(GUTTER_BG)
            } else {
                Style::default().fg(GUTTER_FG).bg(GUTTER_BG)
            };
            for (i, ch) in num_str.chars().enumerate() {
                set_cell(buf, start_x + i as u16, y, ch, style);
            }
        }
    }
}

// ── Separator ──

fn render_separator(buf: &mut Buffer, area: Rect, gw: u16, height: u16) {
    let sep_x = area.x + gw;
    let style = Style::default().fg(SEPARATOR_FG);
    for row in 0..height {
        set_cell(buf, sep_x, area.y + row, '│', style);
    }
}

// ── Content ──

/// 渲染内容区：按视觉行映射逐行渲染，每行用 col_offset 做水平裁剪。
fn render_content(editor: &TextEditor, buf: &mut Buffer, area: Rect, visual_rows: &[VisualRow]) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let cursor_line = editor.cursor().line;
    let selection = editor.selection_range();

    for (row_idx, vr) in visual_rows.iter().enumerate() {
        let y = area.y + row_idx as u16;
        let is_current = vr.logical_line == cursor_line;
        let default_style = if is_current {
            Style::default().bg(CURRENT_LINE_BG)
        } else {
            Style::default().bg(DEFAULT_BG)
        };

        clear_row(buf, area.x, y, area.width, default_style);

        let scroll_x = vr.col_offset;
        if let Some(spans) = editor.line_highlights(vr.logical_line) {
            render_chars(
                buf,
                area,
                y,
                &spans_to_chars(spans),
                scroll_x,
                vr.logical_line,
                &selection,
                default_style,
            );
        } else {
            let text = editor.line_text(vr.logical_line);
            let chars: Vec<(char, Option<Style>)> = text.chars().map(|ch| (ch, None)).collect();
            render_chars(
                buf,
                area,
                y,
                &chars,
                scroll_x,
                vr.logical_line,
                &selection,
                default_style,
            );
        }
    }
}

/// 将高亮 spans 转为统一的 char + style 对列表。
fn spans_to_chars(spans: &[(Style, String)]) -> Vec<(char, Option<Style>)> {
    let mut result = Vec::new();
    for (style, text) in spans {
        for ch in text.chars() {
            result.push((ch, Some(*style)));
        }
    }
    result
}

/// 通用的字符渲染：逐字符累加 display_col，跳过 scroll_x 之前的部分，
/// 超出 area.width 时停止（严格视口裁剪）。
#[allow(clippy::too_many_arguments)]
fn render_chars(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    chars: &[(char, Option<Style>)],
    scroll_x: usize,
    file_line: usize,
    selection: &Option<(super::CursorPos, super::CursorPos)>,
    default_style: Style,
) {
    let mut display_col: usize = 0;
    let mut char_idx: usize = 0;
    let max_col = (area.width as usize).min(MAX_DISPLAY_COLS);
    let scroll_end = scroll_x + max_col;

    for &(ch, span_style) in chars {
        let cw = char_width(ch);
        let char_start = display_col;
        let char_end = display_col + cw;

        // 完全在视口左侧 → 跳过
        if char_end <= scroll_x {
            display_col = char_end;
            char_idx += 1;
            continue;
        }
        // 超出视口右侧 → 停止
        if char_start >= scroll_end {
            break;
        }
        // CJK 跨越左边界 → 整体跳过（不渲染半字）
        if char_start < scroll_x && char_end > scroll_x {
            display_col = char_end;
            char_idx += 1;
            continue;
        }

        let relative = char_start - scroll_x;

        // 选中判断
        let selected = is_char_selected(file_line, char_idx, selection);
        let style = if selected {
            Style::default().bg(SELECTION_BG)
        } else if let Some(ss) = span_style {
            let bg = default_style.bg.unwrap_or(DEFAULT_BG);
            ss.patch(Style::default().bg(bg))
        } else {
            default_style
        };

        // 写入字符（严格在视口内）
        if relative < max_col {
            set_cell(buf, area.x + relative as u16, y, ch, style);
        }
        // CJK 宽度填充
        for extra in 1..cw {
            let col = relative + extra;
            if col >= max_col {
                break;
            }
            set_cell(buf, area.x + col as u16, y, ' ', style);
        }

        display_col = char_end;
        char_idx += 1;
    }
}

// ── Scrollbar ──

fn render_scrollbar(
    editor: &TextEditor,
    buf: &mut Buffer,
    area: Rect,
    height: u16,
    _visual_rows: &[VisualRow],
) {
    let sb_x = area.x + area.width.saturating_sub(1);
    let total_lines = editor.line_count();

    if total_lines <= height as usize {
        return;
    }

    let scroll_y = editor.scroll_y();
    let thumb_h = ((height as usize * height as usize) / total_lines).max(1) as u16;
    let thumb_start = ((scroll_y as u64 * (height as u64 - thumb_h as u64))
        / (total_lines as u64 - height as u64).max(1)) as u16;

    for row in 0..height {
        let is_thumb = row >= thumb_start && row < thumb_start + thumb_h;
        set_cell(
            buf,
            sb_x,
            area.y + row,
            if is_thumb { '█' } else { '┊' },
            if is_thumb {
                Style::default().fg(Color::Rgb(100, 100, 110))
            } else {
                Style::default().fg(Color::Rgb(40, 40, 50))
            },
        );
    }
}

// ── Cursor ──

fn render_cursor(
    editor: &TextEditor,
    buf: &mut Buffer,
    content_area: Rect,
    visual_rows: &[VisualRow],
) {
    if content_area.width == 0 {
        return;
    }

    let cursor = editor.cursor();
    let display_col = editor.char_idx_to_display_col(cursor.line, cursor.col);
    let cw = content_area.width as usize;

    // 找到光标所在的视觉行
    for (row_idx, vr) in visual_rows.iter().enumerate() {
        if vr.logical_line != cursor.line {
            continue;
        }
        let seg_start = vr.col_offset;
        let seg_end = seg_start + cw;
        if display_col >= seg_start && display_col < seg_end {
            let y = content_area.y + row_idx as u16;
            let relative = display_col - seg_start;
            let x = content_area.x + relative as u16;
            if let Some(cell) = buf.cell_mut((x, y)) {
                let existing_fg = cell.fg;
                cell.set_style(Style::default().fg(existing_fg).bg(CURSOR_BG));
            }
            return;
        }
    }
}

// ── Status Bar ──

fn render_status_bar(editor: &TextEditor, buf: &mut Buffer, x: u16, y: u16, width: u16) {
    let cursor = editor.cursor();
    let line_num = cursor.line + 1;
    let total = editor.line_count();
    let col = cursor.col + 1;

    let mut text = format!(" L{}/{}, C{}", line_num, total, col);
    if editor.is_modified() {
        text.push_str(" ●");
    }

    let style = Style::default().fg(STATUS_FG).bg(DEFAULT_BG);
    clear_row(buf, x, y, width, style);

    for (i, ch) in text.chars().enumerate() {
        if i as u16 >= width {
            break;
        }
        set_cell(buf, x + i as u16, y, ch, style);
    }
}

// ── 辅助函数 ──

fn is_char_selected(
    line: usize,
    char_idx: usize,
    selection: &Option<(super::CursorPos, super::CursorPos)>,
) -> bool {
    let Some((start, end)) = selection else {
        return false;
    };
    if line < start.line || line > end.line {
        return false;
    }
    if line == start.line && line == end.line {
        return char_idx >= start.col && char_idx < end.col;
    }
    if line == start.line {
        return char_idx >= start.col;
    }
    if line == end.line {
        return char_idx < end.col;
    }
    true
}

fn set_cell(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}

fn clear_row(buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
    for col in 0..width {
        set_cell(buf, x + col, y, ' ', style);
    }
}
