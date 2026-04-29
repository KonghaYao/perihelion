use once_cell::sync::Lazy;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
pub static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// 对多行代码块进行语法高亮，返回着色后的 Line 列表。
/// 当语言标签未识别时返回 None，调用方应回退到统一颜色渲染。
pub fn highlight_code_block(lang: &str, lines: &[String]) -> Option<Vec<Line<'static>>> {
    let ss = &*SYNTAX_SET;
    let syntax = ss.find_syntax_by_token(lang)?;
    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut result = Vec::with_capacity(lines.len());
    for line_text in lines {
        let mut spans = Vec::new();

        let ranges = highlighter.highlight_line(line_text, ss).ok()?;
        for (style, text) in ranges {
            let color = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
            spans.push(Span::styled(text.to_string(), Style::default().fg(color)));
        }
        result.push(Line::from(spans));
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_code() {
        let result = highlight_code_block("rust", &["fn main() {}".to_string()]);
        assert!(result.is_some(), "rust 代码应被识别");
        let lines = result.unwrap();
        assert_eq!(lines.len(), 1);
        let has_content = lines[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>().contains("fn main");
        assert!(has_content, "应有代码内容");
        let has_syntax_color = lines[0].spans.iter().any(|s| {
            s.style.fg.is_some()
        });
        assert!(has_syntax_color, "应有非前缀颜色的语法着色 span");
    }

    #[test]
    fn highlight_unknown_lang() {
        let result = highlight_code_block("unknown_lang_xyz", &["hello".to_string()]);
        assert!(result.is_none(), "未识别语言应返回 None");
    }

    #[test]
    fn highlight_empty_lang() {
        let result = highlight_code_block("", &["hello".to_string()]);
        assert!(result.is_none(), "空语言标签应返回 None");
    }

    #[test]
    fn highlight_multiline() {
        let lines = vec![
            "fn main() {".to_string(),
            "    println!(\"hello\");".to_string(),
            "}".to_string(),
        ];
        let result = highlight_code_block("rust", &lines);
        assert!(result.is_some(), "多行 rust 代码应被识别");
        assert_eq!(result.unwrap().len(), 3, "输出行数应等于输入行数");
    }
}
