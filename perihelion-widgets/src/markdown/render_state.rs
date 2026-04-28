use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::MarkdownTheme;

// ── 辅助类型 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum ListType {
    Ordered(u64),
    Unordered,
}

#[derive(Debug, Clone)]
pub(crate) struct ListState {
    list_type: ListType,
}

// ── 表格类型 ──────────────────────────────────────────────────────────────────

type CellContent = Vec<Span<'static>>;

#[derive(Debug, Default)]
struct TableBuilder {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<CellContent>>,
    current_row: Vec<CellContent>,
    current_cell: CellContent,
    in_head: bool,
}

impl TableBuilder {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            ..Default::default()
        }
    }

    fn push_cell(&mut self) {
        let cell = std::mem::take(&mut self.current_cell);
        self.current_row.push(cell);
    }

    fn push_row(&mut self) {
        if !self.current_row.is_empty() {
            let row = std::mem::take(&mut self.current_row);
            self.rows.push(row);
        }
    }

    fn render(self, theme: &dyn MarkdownTheme) -> Vec<Line<'static>> {
        if self.rows.is_empty() {
            return vec![];
        }

        let num_cols = self.rows[0].len();
        if num_cols == 0 {
            return vec![];
        }

        let mut col_widths = vec![0usize; num_cols];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let w: usize = cell.iter().map(|s| s.content.width()).sum();
                    col_widths[i] = col_widths[i].max(w);
                }
            }
        }

        let mut lines = Vec::new();

        lines.push(Line::from(make_border(
            &col_widths, '┌', '┬', '┐', '─', theme,
        )));

        for (row_idx, row) in self.rows.iter().enumerate() {
            lines.push(make_data_line(&col_widths, row, &self.alignments, theme));

            if row_idx == 0 {
                lines.push(Line::from(make_border(
                    &col_widths, '├', '┼', '┤', '─', theme,
                )));
            }
        }

        lines.push(Line::from(make_border(
            &col_widths, '└', '┴', '┘', '─', theme,
        )));

        lines
    }

    /// 包装单元格文本以适应最大宽度
    fn wrap_cells(&self, max_width: usize, _theme: &dyn MarkdownTheme) -> Vec<Vec<Vec<Vec<Span<'static>>>>> {
        let num_cols = self.rows.get(0).map(|r| r.len()).unwrap_or(0);
        if num_cols == 0 {
            return vec![];
        }

        // 计算可用宽度（减去边框和间距）
        let border_width = num_cols + 1; // 每列两边的空格和边框
        let available_width = max_width.saturating_sub(border_width);

        // 计算每列的最小和理想宽度
        let _min_col_widths = self.calculate_min_col_widths(num_cols);
        let ideal_col_widths = self.calculate_ideal_col_widths(num_cols);

        // 如果总宽度超过可用宽度，按比例缩小
        let total_ideal: usize = ideal_col_widths.iter().sum();
        let col_widths = if total_ideal > available_width {
            self.scale_col_widths(&ideal_col_widths, available_width)
        } else {
            ideal_col_widths
        };

        // 包装每个单元格的文本
        let mut wrapped_rows = Vec::new();
        for row in &self.rows {
            let mut wrapped_row = Vec::new();
            for (col_idx, cell) in row.iter().enumerate() {
                let col_width = col_widths.get(col_idx).copied().unwrap_or(0);
                let wrapped = self.wrap_cell_text(cell, col_width);
                wrapped_row.push(wrapped);
            }
            wrapped_rows.push(wrapped_row);
        }

        wrapped_rows
    }

    /// 计算每列的最小宽度（基于最短内容）
    fn calculate_min_col_widths(&self, num_cols: usize) -> Vec<usize> {
        let mut min_widths = vec![0usize; num_cols];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let w: usize = cell.iter().map(|s| s.content.width()).sum();
                    min_widths[i] = min_widths[i].max(w.min(10)); // 最小宽度至少为10
                }
            }
        }
        min_widths
    }

    /// 计算每列的理想宽度（基于内容长度）
    fn calculate_ideal_col_widths(&self, num_cols: usize) -> Vec<usize> {
        let mut ideal_widths = vec![0usize; num_cols];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let w: usize = cell.iter().map(|s| s.content.width()).sum();
                    ideal_widths[i] = ideal_widths[i].max(w);
                }
            }
        }
        ideal_widths
    }

    /// 按比例缩放列宽度以适应可用宽度
    fn scale_col_widths(&self, ideal_widths: &[usize], available_width: usize) -> Vec<usize> {
        let total: usize = ideal_widths.iter().sum();
        if total == 0 {
            return ideal_widths.to_vec();
        }

        let mut scaled = Vec::with_capacity(ideal_widths.len());
        let mut remaining = available_width;

        for (i, &ideal) in ideal_widths.iter().enumerate() {
            if i == ideal_widths.len() - 1 {
                // 最后一列取剩余宽度
                scaled.push(remaining);
            } else {
                let scaled_width = (ideal * available_width) / total;
                scaled.push(scaled_width.max(1)); // 至少为1
                remaining = remaining.saturating_sub(scaled_width.max(1));
            }
        }

        scaled
    }

    /// 渲染表格，支持自动换行
    fn render_with_wrap(self, max_width: usize, theme: &dyn MarkdownTheme) -> Vec<Line<'static>> {
        let wrapped_rows = self.wrap_cells(max_width, theme);
        if wrapped_rows.is_empty() {
            return vec![];
        }

        // 计算每列的最大宽度（考虑换行后的每行）
        let num_cols = wrapped_rows[0].len();
        let mut col_widths = vec![0usize; num_cols];

        for row in &wrapped_rows {
            for (col_idx, cell_lines) in row.iter().enumerate() {
                if col_idx < num_cols {
                    for line in cell_lines {
                        let line_width: usize = line.iter().map(|s| s.content.width()).sum();
                        col_widths[col_idx] = col_widths[col_idx].max(line_width);
                    }
                }
            }
        }

        let mut lines = Vec::new();

        // 顶部边框
        lines.push(Line::from(make_border(
            &col_widths, '┌', '┬', '┐', '─', theme,
        )));

        // 渲染每一行
        for (row_idx, row) in wrapped_rows.iter().enumerate() {
            // 计算这一行需要的行数（基于最高的单元格）
            let max_lines = row.iter().map(|cell_lines| cell_lines.len()).max().unwrap_or(1);

            for line_idx in 0..max_lines {
                let mut spans = Vec::new();
                spans.push(Span::styled("│".to_string(), Style::default().fg(theme.muted())));

                for (col_idx, cell_lines) in row.iter().enumerate() {
                    let col_w = col_widths.get(col_idx).copied().unwrap_or(0);
                    spans.push(Span::raw(" "));

                    if line_idx < cell_lines.len() {
                        // 获取这一行的内容
                        let line_spans = &cell_lines[line_idx];
                        let content_width: usize = line_spans.iter().map(|s| s.content.width()).sum();
                        let padding = col_w.saturating_sub(content_width);

                        let align = self.alignments.get(col_idx).copied().unwrap_or(Alignment::None);
                        match align {
                            Alignment::Center => {
                                let left_pad = padding / 2;
                                let right_pad = padding - left_pad;
                                if left_pad > 0 {
                                    spans.push(Span::raw(" ".repeat(left_pad)));
                                }
                                spans.extend(line_spans.iter().cloned());
                                if right_pad > 0 {
                                    spans.push(Span::raw(" ".repeat(right_pad)));
                                }
                            }
                            Alignment::Right => {
                                if padding > 0 {
                                    spans.push(Span::raw(" ".repeat(padding)));
                                }
                                spans.extend(line_spans.iter().cloned());
                            }
                            Alignment::None | Alignment::Left => {
                                spans.extend(line_spans.iter().cloned());
                                if padding > 0 {
                                    spans.push(Span::raw(" ".repeat(padding)));
                                }
                            }
                        }
                    } else {
                        // 这一行的这个单元格没有内容，填充空格
                        spans.push(Span::raw(" ".repeat(col_w)));
                    }

                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("│".to_string(), Style::default().fg(theme.muted())));
                }

                lines.push(Line::from(spans));
            }

            // 在第一行后添加分隔线
            if row_idx == 0 {
                lines.push(Line::from(make_border(
                    &col_widths, '├', '┼', '┤', '─', theme,
                )));
            }
        }

        // 底部边框
        lines.push(Line::from(make_border(
            &col_widths, '└', '┴', '┘', '─', theme,
        )));

        lines
    }

    /// 包装单个单元格的文本
    fn wrap_cell_text(&self, cell: &[Span], max_width: usize) -> Vec<Vec<Span<'static>>> {
        if max_width == 0 || cell.is_empty() {
            return vec![vec![]];
        }

        // 合并单元格中的所有文本
        let full_text: String = cell.iter().map(|s| s.content.as_ref()).collect();
        let base_style = cell.first().map(|s| s.style).unwrap_or_default();

        if full_text.width() <= max_width {
            // 不需要换行，将所有 Span 转换为 'static
            let static_spans: Vec<Span<'static>> = cell.iter().map(|s| Span::styled(s.content.as_ref().to_string(), s.style)).collect();
            return vec![static_spans];
        }

        // 需要换行
        let mut lines = Vec::new();
        let mut current_pos = 0;
        let chars: Vec<char> = full_text.chars().collect();

        while current_pos < chars.len() {
            let remaining = chars.len() - current_pos;
            let take_len = remaining.min(max_width);

            // 尽量在空格处换行
            let mut break_pos = current_pos + take_len;
            if break_pos < chars.len() {
                // 向前查找最近的空格
                for i in (current_pos..break_pos).rev() {
                    if chars[i].is_whitespace() {
                        break_pos = i;
                        break;
                    }
                }
            }

            let line_text: String = chars[current_pos..break_pos].iter().collect();
            let trimmed = line_text.trim();
            if !trimmed.is_empty() {
                lines.push(vec![Span::styled(trimmed.to_string(), base_style)]);
            }

            current_pos = break_pos;
            // 跳过空格
            while current_pos < chars.len() && chars[current_pos].is_whitespace() {
                current_pos += 1;
            }
        }

        if lines.is_empty() {
            lines.push(vec![]);
        }

        lines
    }
}

fn make_border(
    col_widths: &[usize],
    left: char,
    mid: char,
    right: char,
    fill: char,
    theme: &dyn MarkdownTheme,
) -> Span<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, &w) in col_widths.iter().enumerate() {
        for _ in 0..w + 2 {
            s.push(fill);
        }
        if i < col_widths.len() - 1 {
            s.push(mid);
        }
    }
    s.push(right);
    Span::styled(s, Style::default().fg(theme.muted()))
}

fn make_data_line(
    col_widths: &[usize],
    row: &[CellContent],
    alignments: &[Alignment],
    theme: &dyn MarkdownTheme,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    spans.push(Span::styled("│".to_string(), Style::default().fg(theme.muted())));

    for (i, col_w) in col_widths.iter().enumerate() {
        spans.push(Span::raw(" "));

        let cell_spans = row.get(i).cloned().unwrap_or_default();
        let cell_char_count: usize = cell_spans.iter().map(|s| s.content.width()).sum();
        let padding = col_w.saturating_sub(cell_char_count);

        let align = alignments.get(i).copied().unwrap_or(Alignment::None);

        match align {
            Alignment::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                if left_pad > 0 {
                    spans.push(Span::raw(" ".repeat(left_pad)));
                }
                spans.extend(cell_spans);
                if right_pad > 0 {
                    spans.push(Span::raw(" ".repeat(right_pad)));
                }
            }
            Alignment::Right => {
                if padding > 0 {
                    spans.push(Span::raw(" ".repeat(padding)));
                }
                spans.extend(cell_spans);
            }
            Alignment::None | Alignment::Left => {
                spans.extend(cell_spans);
                if padding > 0 {
                    spans.push(Span::raw(" ".repeat(padding)));
                }
            }
        }

        spans.push(Span::raw(" "));
        spans.push(Span::styled("│".to_string(), Style::default().fg(theme.muted())));
    }

    Line::from(spans)
}

// ── 渲染状态机 ────────────────────────────────────────────────────────────────

pub(super) struct RenderState<'a> {
    pub lines: Vec<Line<'static>>,
    pub current_spans: Vec<Span<'static>>,
    pub inline_style: Style,
    pub list_stack: Vec<ListState>,
    pub quote_depth: u32,
    pub in_code_block: bool,
    pub code_block_lang: String,
    table: Option<TableBuilder>,
    theme: &'a dyn MarkdownTheme,
    max_width: usize,
}

impl<'a> RenderState<'a> {
    pub fn new(theme: &'a dyn MarkdownTheme) -> Self {
        Self {
            lines: Vec::new(),
            current_spans: Vec::new(),
            inline_style: Style::default(),
            list_stack: Vec::new(),
            quote_depth: 0,
            in_code_block: false,
            code_block_lang: String::new(),
            table: None,
            theme,
            max_width: 80, // 默认宽度
        }
    }

    pub fn with_max_width(mut self, width: usize) -> Self {
        self.max_width = width;
        self
    }

    pub fn flush_line(&mut self) {
        let mut spans = std::mem::take(&mut self.current_spans);

        if self.quote_depth > 0 && !spans.is_empty() {
            let prefix = "▍ ".repeat(self.quote_depth as usize);
            spans.insert(0, Span::styled(prefix, Style::default().fg(self.theme.quote_prefix())));
        }

        if spans.is_empty() {
            self.lines.push(Line::default());
        } else {
            self.lines.push(Line::from(spans));
        }
    }

    pub fn push_span(&mut self, text: String, extra: Style) {
        let style = self.inline_style.patch(extra);
        self.current_spans.push(Span::styled(text, style));
    }

    pub fn handle_event(&mut self, event: Event<'_>) {
        match event {
            // ── 标题 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let color = match level {
                    HeadingLevel::H1 | HeadingLevel::H2 | HeadingLevel::H3 => self.theme.heading(),
                    _ => self.theme.muted(),
                };
                self.inline_style =
                    Style::default().fg(color).add_modifier(Modifier::BOLD);
                // 标题前添加空行分隔
                self.flush_line();
            }
            Event::End(TagEnd::Heading(_)) => {
                self.inline_style = Style::default();
                self.flush_line();
                // 标题后添加空行分隔
                self.lines.push(Line::default());
            }

            // ── 段落 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                self.flush_line();
            }

            // ── 代码块 ────────────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                self.in_code_block = true;
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                // 不再立即输出标签，而是在第一行代码时显示
            }
            Event::End(TagEnd::CodeBlock) => {
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
                self.in_code_block = false;
                self.code_block_lang.clear();
            }

            // ── 列表 ─────────────────────────────────────────────────────────
            Event::Start(Tag::List(start)) => {
                let list_type = match start {
                    Some(n) => ListType::Ordered(n),
                    None => ListType::Unordered,
                };
                self.list_stack.push(ListState { list_type });
            }
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let bullet = if let Some(state) = self.list_stack.last_mut() {
                    match &mut state.list_type {
                        ListType::Unordered => format!("{}• ", indent),
                        ListType::Ordered(n) => {
                            let s = format!("{}{}. ", indent, n);
                            *n += 1;
                            s
                        }
                    }
                } else {
                    format!("{}• ", indent)
                };
                self.current_spans
                    .push(Span::styled(bullet, Style::default().fg(self.theme.list_bullet())));
            }
            Event::End(TagEnd::Item) => {
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
            }

            // ── 引用块 ────────────────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                self.quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if self.quote_depth > 0 {
                    self.quote_depth -= 1;
                }
            }

            // ── 水平线 ────────────────────────────────────────────────────────
            Event::Rule => {
                let rule = "─".repeat(60);
                self.current_spans
                    .push(Span::styled(rule, Style::default().fg(self.theme.separator())));
                self.flush_line();
            }

            // ── 文本（含代码块内容） ───────────────────────────────────────────
            Event::Text(text) => {
                let text_str = text.into_string();
                if self.in_code_block {
                    let code_lines: Vec<&str> = text_str.split('\n').collect();
                    for (i, line_text) in code_lines.iter().enumerate() {
                        if i == code_lines.len() - 1 && line_text.is_empty() {
                            continue;
                        }

                        // 在第一行添加语言标签
                        if i == 0 && !self.code_block_lang.is_empty() {
                            let tag = format!("[{}] ", self.code_block_lang);
                            self.current_spans.push(Span::styled(
                                tag,
                                Style::default()
                                    .fg(self.theme.code())
                                    .add_modifier(Modifier::BOLD),
                            ));
                        }

                        self.current_spans.push(Span::styled(
                            "│ ".to_string(),
                            Style::default().fg(self.theme.code_prefix()),
                        ));
                        self.current_spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(self.theme.text()),
                        ));
                        self.flush_line();
                    }
                } else if self.table.is_some() {
                    let style = self.inline_style;
                    self.table.as_mut().unwrap().current_cell
                        .push(Span::styled(text_str, style));
                } else {
                    self.push_span(text_str, Style::default());
                }
            }

            // ── 行内代码 ──────────────────────────────────────────────────────
            Event::Code(text) => {
                let style = Style::default().fg(self.theme.code());
                let span = Span::styled(text.into_string(), style);
                if self.table.is_some() {
                    self.table.as_mut().unwrap().current_cell.push(span);
                } else {
                    self.current_spans.push(span);
                }
            }

            // ── Strong / Emphasis / Strikethrough ────────────────────────────
            Event::Start(Tag::Strong) => {
                self.inline_style = self.inline_style.add_modifier(Modifier::BOLD);
            }
            Event::End(TagEnd::Strong) => {
                self.inline_style = self.inline_style.remove_modifier(Modifier::BOLD);
            }
            Event::Start(Tag::Emphasis) => {
                self.inline_style = self.inline_style.add_modifier(Modifier::ITALIC);
            }
            Event::End(TagEnd::Emphasis) => {
                self.inline_style = self.inline_style.remove_modifier(Modifier::ITALIC);
            }
            Event::Start(Tag::Strikethrough) => {
                self.inline_style =
                    self.inline_style.add_modifier(Modifier::CROSSED_OUT);
            }
            Event::End(TagEnd::Strikethrough) => {
                self.inline_style =
                    self.inline_style.remove_modifier(Modifier::CROSSED_OUT);
            }

            // ── 链接 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Link { .. }) => {
                self.inline_style = self.inline_style
                    .fg(self.theme.link())
                    .add_modifier(Modifier::UNDERLINED);
            }
            Event::End(TagEnd::Link) => {
                self.inline_style = Style::default();
            }

            // ── 表格 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Table(alignments)) => {
                self.table = Some(TableBuilder::new(alignments));
            }
            Event::End(TagEnd::Table) => {
                if let Some(tb) = self.table.take() {
                    let table_lines = tb.render_with_wrap(self.max_width, self.theme);
                    self.lines.extend(table_lines);
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(tb) = self.table.as_mut() {
                    tb.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(tb) = self.table.as_mut() {
                    tb.push_cell();
                    tb.push_row();
                    tb.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if let Some(tb) = self.table.as_mut() {
                    tb.push_cell();
                    tb.push_row();
                }
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                if let Some(tb) = self.table.as_mut() {
                    tb.push_cell();
                }
            }

            // ── 换行 ─────────────────────────────────────────────────────────
            Event::SoftBreak => {
                self.push_span(" ".to_string(), Style::default());
            }
            Event::HardBreak => {
                self.flush_line();
            }

            _ => {}
        }
    }
}
