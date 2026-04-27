mod render_state;

use pulldown_cmark::{Options, Parser};
use ratatui::style::Color;
use ratatui::text::Text;

use render_state::RenderState;

// ── MarkdownTheme trait ──────────────────────────────────────

/// Markdown 渲染颜色主题——将 render_state.rs 中的硬编码颜色参数化
pub trait MarkdownTheme {
    /// 标题颜色（H1-H3，对应原 theme::WARNING）
    fn heading(&self) -> Color;
    /// 主文字颜色（列表前缀、代码内容，对应原 theme::TEXT）
    fn text(&self) -> Color;
    /// 弱化文字颜色（边框、分隔线、代码标签，对应原 theme::MUTED）
    fn muted(&self) -> Color;
    /// 行内代码颜色（对应原 theme::WARNING，与 heading 共用）
    fn code(&self) -> Color;
    /// 链接颜色（对应原 theme::SAGE）
    fn link(&self) -> Color;
    /// 代码块行前缀颜色（`│`，对应原 theme::SAGE）
    fn code_prefix(&self) -> Color;
    /// 引用块前缀颜色（`▍`，对应原 theme::MUTED）
    fn quote_prefix(&self) -> Color;
    /// 列表项目符号颜色（`•`，对应原 theme::TEXT）
    fn list_bullet(&self) -> Color;
    /// 水平线颜色（`─`，对应原 theme::MUTED）
    fn separator(&self) -> Color;
}

/// 默认 Markdown 主题——色值与 DarkTheme 一致
#[derive(Debug, Clone)]
pub struct DefaultMarkdownTheme;

impl MarkdownTheme for DefaultMarkdownTheme {
    fn heading(&self) -> Color { Color::Rgb(176, 152, 120) }    // WARNING
    fn text(&self) -> Color { Color::Rgb(218, 206, 208) }       // TEXT
    fn muted(&self) -> Color { Color::Rgb(140, 125, 120) }      // MUTED
    fn code(&self) -> Color { Color::Rgb(176, 152, 120) }       // WARNING
    fn link(&self) -> Color { Color::Rgb(110, 181, 106) }       // SAGE
    fn code_prefix(&self) -> Color { Color::Rgb(110, 181, 106) } // SAGE
    fn quote_prefix(&self) -> Color { Color::Rgb(140, 125, 120) } // MUTED
    fn list_bullet(&self) -> Color { Color::Rgb(218, 206, 208) } // TEXT
    fn separator(&self) -> Color { Color::Rgb(140, 125, 120) }  // MUTED
}

/// 解析 markdown 文本为 ratatui Text
pub fn parse_markdown(input: &str, theme: &dyn MarkdownTheme) -> Text<'static> {
    if input.is_empty() {
        return Text::raw("");
    }
    let options = Options::all() - Options::ENABLE_SMART_PUNCTUATION;
    let parser = Parser::new_ext(input, options);
    let mut state = RenderState::new(theme);
    for event in parser {
        state.handle_event(event);
    }
    if !state.current_spans.is_empty() {
        state.flush_line();
    }
    Text::from(state.lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    fn default_theme() -> DefaultMarkdownTheme {
        DefaultMarkdownTheme
    }

    #[test]
    fn parse_empty_input() {
        let text = parse_markdown("", &default_theme());
        // Empty input may produce an empty line
        assert!(text.lines.len() <= 1, "Expected at most 1 line for empty input, got {}", text.lines.len());
    }

    #[test]
    fn parse_heading() {
        let text = parse_markdown("# Hello", &default_theme());
        assert!(text.lines.len() >= 1);
        let line = &text.lines[0];
        let heading_found = line.spans.iter().any(|s| s.content.contains("Hello"));
        assert!(heading_found, "Expected 'Hello' in heading output");
        let has_bold = line.spans.iter().any(|s| s.style.add_modifier == Modifier::BOLD);
        assert!(has_bold, "Expected BOLD modifier on heading");
    }

    #[test]
    fn parse_code_block() {
        let text = parse_markdown("```rust\nfn main() {}\n```", &default_theme());
        assert!(text.lines.len() >= 2);
        let has_tag = text.lines.iter().any(|l| {
            let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
            line_str.contains("[rust]")
        });
        assert!(has_tag, "Expected [rust] tag");
        let has_code = text.lines.iter().any(|l| {
            let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
            line_str.contains("│") && line_str.contains("fn main")
        });
        assert!(has_code, "Expected code line with │ prefix");
    }

    #[test]
    fn parse_inline_code() {
        let text = parse_markdown("`hello`", &default_theme());
        assert!(text.lines.len() >= 1);
        let has_code = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("hello") && s.style.fg == Some(default_theme().code())));
        assert!(has_code, "Expected inline code with code color");
    }

    #[test]
    fn parse_bold_italic() {
        let text = parse_markdown("**bold** *italic*", &default_theme());
        assert!(text.lines.len() >= 1);
        let line = &text.lines[0];
        let has_bold = line.spans.iter().any(|s| s.style.add_modifier == Modifier::BOLD);
        assert!(has_bold, "Expected BOLD modifier");
        let has_italic = line.spans.iter().any(|s| s.style.add_modifier == Modifier::ITALIC);
        assert!(has_italic, "Expected ITALIC modifier");
    }

    #[test]
    fn parse_link() {
        let text = parse_markdown("[text](url)", &default_theme());
        assert!(text.lines.len() >= 1);
        let has_link = text.lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                s.content.contains("text") && s.style.fg == Some(default_theme().link())
            })
        });
        assert!(has_link, "Expected link text with link color");
    }

    #[test]
    fn parse_unordered_list() {
        let text = parse_markdown("- item1\n- item2", &default_theme());
        assert!(text.lines.len() >= 2);
        let has_bullet1 = text.lines.iter().any(|l| {
            let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
            line_str.contains("•") && line_str.contains("item1")
        });
        assert!(has_bullet1, "Expected bullet • and item1");
        let has_bullet2 = text.lines.iter().any(|l| {
            let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
            line_str.contains("item2")
        });
        assert!(has_bullet2, "Expected item2");
    }

    #[test]
    fn parse_ordered_list() {
        let text = parse_markdown("1. first\n2. second", &default_theme());
        assert!(text.lines.len() >= 2);
        let has_1 = text.lines.iter().any(|l| {
            let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
            line_str.contains("1.") && line_str.contains("first")
        });
        assert!(has_1, "Expected '1. first'");
        let has_2 = text.lines.iter().any(|l| {
            let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
            line_str.contains("2.") && line_str.contains("second")
        });
        assert!(has_2, "Expected '2. second'");
    }

    #[test]
    fn parse_blockquote() {
        let text = parse_markdown("> quoted", &default_theme());
        assert!(text.lines.len() >= 1);
        let has_prefix = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("▍")));
        assert!(has_prefix, "Expected blockquote prefix ▍");
    }

    #[test]
    fn parse_horizontal_rule() {
        let text = parse_markdown("---", &default_theme());
        assert!(text.lines.len() >= 1);
        let has_rule = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("─")));
        assert!(has_rule, "Expected horizontal rule ─");
    }

    #[test]
    fn parse_table() {
        let text = parse_markdown("| H1 | H2 |\n| --- | --- |\n| A | B |", &default_theme());
        assert!(text.lines.len() >= 3);
        let has_border = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("┌") || s.content.contains("├") || s.content.contains("└")));
        assert!(has_border, "Expected table box-drawing borders");
    }
}
