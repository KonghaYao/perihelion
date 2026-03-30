use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use super::super::theme;

// ── 辅助类型 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) enum ListType {
    Ordered(u64),
    Unordered,
}

#[derive(Debug, Clone)]
pub(crate) struct ListState {
    pub list_type: ListType,
}

// ── 表格类型 ──────────────────────────────────────────────────────────────────

/// 一个单元格的渲染结果：多个 Span 组成
type CellContent = Vec<Span<'static>>;

/// 表格累积状态
#[derive(Debug, Default)]
struct TableBuilder {
    /// 列对齐方式
    alignments: Vec<Alignment>,
    /// 所有行：head + body
    rows: Vec<Vec<CellContent>>,
    /// 当前行的单元格
    current_row: Vec<CellContent>,
    /// 当前单元格的 span 累积
    current_cell: CellContent,
    /// 是否在 head 中
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

    /// 渲染表格为多行 Lines
    fn render(self) -> Vec<Line<'static>> {
        if self.rows.is_empty() {
            return vec![];
        }

        let num_cols = self.rows[0].len();
        if num_cols == 0 {
            return vec![];
        }

        // 计算每列最大宽度（字符数）
        let mut col_widths = vec![0usize; num_cols];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let w: usize = cell.iter().map(|s| s.content.chars().count()).sum();
                    col_widths[i] = col_widths[i].max(w);
                }
            }
        }

        let mut lines = Vec::new();

        // 顶边框: ┌─────┬─────┐
        lines.push(Line::from(make_border(
            &col_widths, '┌', '┬', '┐', '─',
        )));

        // 渲染每行
        for (row_idx, row) in self.rows.iter().enumerate() {
            // 数据行: │ cell │ cell │
            lines.push(make_data_line(&col_widths, row, &self.alignments));

            if row_idx == 0 {
                // head 后的分隔线: ├─────┼─────┤
                lines.push(Line::from(make_border(
                    &col_widths, '├', '┼', '┤', '─',
                )));
            }
        }

        // 底边框: └─────┴─────┘
        lines.push(Line::from(make_border(
            &col_widths, '└', '┴', '┘', '─',
        )));

        lines
    }
}

/// 生成边框行（如 ┌─────┬─────┐）
fn make_border(
    col_widths: &[usize],
    left: char,
    mid: char,
    right: char,
    fill: char,
) -> Span<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, &w) in col_widths.iter().enumerate() {
        // 单元格内左右各 1 空格 padding
        for _ in 0..w + 2 {
            s.push(fill);
        }
        if i < col_widths.len() - 1 {
            s.push(mid);
        }
    }
    s.push(right);
    Span::styled(s, Style::default().fg(theme::MUTED))
}

/// 生成数据行（如 │ Name │ Value │）
fn make_data_line(
    col_widths: &[usize],
    row: &[CellContent],
    alignments: &[Alignment],
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    spans.push(Span::styled("│".to_string(), Style::default().fg(theme::MUTED)));

    for (i, col_w) in col_widths.iter().enumerate() {
        spans.push(Span::raw(" ")); // left padding

        let cell_spans = row.get(i).cloned().unwrap_or_default();
        let cell_char_count: usize = cell_spans.iter().map(|s| s.content.chars().count()).sum();
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

        spans.push(Span::raw(" ")); // right padding
        spans.push(Span::styled("│".to_string(), Style::default().fg(theme::MUTED)));
    }

    Line::from(spans)
}

// ── 渲染状态机 ────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct RenderState {
    /// 已完成的行
    pub lines: Vec<Line<'static>>,
    /// 当前行待写入的 Span 列表
    pub current_spans: Vec<Span<'static>>,
    /// 当前累积的行内样式（Strong / Emphasis / Strikethrough / Link 叠加）
    pub inline_style: Style,
    /// 嵌套列表栈（每层记录类型和当前编号）
    pub list_stack: Vec<ListState>,
    /// 引用块嵌套深度
    pub quote_depth: u32,
    /// 是否在代码块内
    pub in_code_block: bool,
    /// 代码块语言标识
    pub code_block_lang: String,
    /// 表格构建器（进入表格时 Some）
    table: Option<TableBuilder>,
}

impl RenderState {
    /// 将 current_spans 封装为 Line 并 push 到 lines，清空 current_spans
    pub fn flush_line(&mut self) {
        let mut spans = std::mem::take(&mut self.current_spans);

        // 引用块前缀：每层加一个 ▍
        if self.quote_depth > 0 && !spans.is_empty() {
            let prefix = "▍ ".repeat(self.quote_depth as usize);
            spans.insert(0, Span::styled(prefix, Style::default().fg(theme::MUTED)));
        }

        if spans.is_empty() {
            self.lines.push(Line::default());
        } else {
            self.lines.push(Line::from(spans));
        }
    }

    /// 将 text 以 inline_style 合并 extra 后作为 Span 追加到 current_spans
    pub fn push_span(&mut self, text: String, extra: Style) {
        let style = self.inline_style.patch(extra);
        self.current_spans.push(Span::styled(text, style));
    }

    /// 处理单个 pulldown-cmark 事件
    pub fn handle_event(&mut self, event: Event<'_>) {
        match event {
            // ── 标题 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let (color, prefix): (Color, Option<&str>) = match level {
                    HeadingLevel::H1 => (theme::WARNING, None),
                    HeadingLevel::H2 => (theme::WARNING, None),
                    HeadingLevel::H3 => (theme::WARNING, None),
                    _ => (theme::MUTED, None),
                };
                self.inline_style =
                    Style::default().fg(color).add_modifier(Modifier::BOLD);
                if let Some(p) = prefix {
                    self.current_spans.push(Span::styled(
                        p.to_string(),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                self.inline_style = Style::default();
                self.flush_line();
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
                if !self.code_block_lang.is_empty() {
                    let tag = format!("[{}]", self.code_block_lang);
                    self.current_spans.push(Span::styled(
                        tag,
                        Style::default().fg(theme::MUTED),
                    ));
                    self.flush_line();
                }
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
                    .push(Span::styled(bullet, Style::default().fg(theme::TEXT)));
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
                    .push(Span::styled(rule, Style::default().fg(theme::MUTED)));
                self.flush_line();
            }

            // ── 文本（含代码块内容） ───────────────────────────────────────────
            Event::Text(text) => {
                let text_str = text.into_string();
                if self.in_code_block {
                    // 代码块：按换行分割，每行加 │ 前缀
                    let code_lines: Vec<&str> = text_str.split('\n').collect();
                    for (i, line_text) in code_lines.iter().enumerate() {
                        // 最后一个 \n 产生的空行跳过
                        if i == code_lines.len() - 1 && line_text.is_empty() {
                            continue;
                        }
                        self.current_spans.push(Span::styled(
                            "│ ".to_string(),
                            Style::default().fg(theme::SAGE),
                        ));
                        self.current_spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(theme::TEXT),
                        ));
                        self.flush_line();
                    }
                } else if self.table.is_some() {
                    // 表格内文本 → 追加到 current_cell
                    let style = self.inline_style;
                    self.table.as_mut().unwrap().current_cell
                        .push(Span::styled(text_str, style));
                } else {
                    self.push_span(text_str, Style::default());
                }
            }

            // ── 行内代码 ──────────────────────────────────────────────────────
            Event::Code(text) => {
                let style = Style::default().fg(theme::WARNING);
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
                    .fg(theme::SAGE)
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
                    let table_lines = tb.render();
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
                    // push last cell of head
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
