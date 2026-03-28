use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span, Text},
};
use super::message_view::ContentBlockView;
use super::theme;

// ── 辅助类型 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum ListType {
    Ordered(u64),
    Unordered,
}

#[derive(Debug, Clone)]
struct ListState {
    list_type: ListType,
}

// ── 渲染状态机 ────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct RenderState {
    /// 已完成的行
    lines: Vec<Line<'static>>,
    /// 当前行待写入的 Span 列表
    current_spans: Vec<Span<'static>>,
    /// 当前累积的行内样式（Strong / Emphasis / Strikethrough / Link 叠加）
    inline_style: Style,
    /// 嵌套列表栈（每层记录类型和当前编号）
    list_stack: Vec<ListState>,
    /// 引用块嵌套深度
    quote_depth: u32,
    /// 是否在代码块内
    in_code_block: bool,
    /// 代码块语言标识
    code_block_lang: String,
}

impl RenderState {
    /// 将 current_spans 封装为 Line 并 push 到 lines，清空 current_spans
    fn flush_line(&mut self) {
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
    fn push_span(&mut self, text: String, extra: Style) {
        let style = self.inline_style.patch(extra);
        self.current_spans.push(Span::styled(text, style));
    }

    /// 处理单个 pulldown-cmark 事件
    fn handle_event(&mut self, event: Event<'_>) {
        match event {
            // ── 标题 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let (color, prefix) = match level {
                    HeadingLevel::H1 => (theme::ACCENT, Some("── ")),
                    HeadingLevel::H2 => (theme::ACCENT, None),
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
                } else {
                    self.push_span(text_str, Style::default());
                }
            }

            // ── 行内代码 ──────────────────────────────────────────────────────
            Event::Code(text) => {
                let style = Style::default().fg(theme::ACCENT);
                self.current_spans
                    .push(Span::styled(text.into_string(), style));
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

// ── 公共接口（保持不变） ───────────────────────────────────────────────────────

/// 解析 markdown 文本为 ratatui Text
pub fn parse_markdown(input: &str) -> Text<'static> {
    if input.is_empty() {
        return Text::raw("");
    }
    // 禁用智能引号，保持原始撇号字符
    let options = Options::all() - Options::ENABLE_SMART_PUNCTUATION;
    let parser = Parser::new_ext(input, options);
    let mut state = RenderState::default();
    for event in parser {
        state.handle_event(event);
    }
    // 收尾：确保最后一行被 flush
    if !state.current_spans.is_empty() {
        state.flush_line();
    }
    Text::from(state.lines)
}

/// 确保 block 已渲染（dirty 为 true 时重新解析）
pub fn ensure_rendered(block: &mut ContentBlockView) {
    if let ContentBlockView::Text { raw, rendered, dirty } = block {
        if *dirty {
            *rendered = parse_markdown(raw);
            *dirty = false;
        }
    }
}
