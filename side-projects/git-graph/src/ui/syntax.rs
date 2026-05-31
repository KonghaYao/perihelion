//! 语法高亮辅助模块。基于 syntect，一次性加载语法定义和主题，
//! 提供 `highlight()` 将文件内容转换为 ratatui Span 列表。

use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

// 全局懒加载语法集和主题
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static SYNTAX_THEME: OnceLock<Theme> = OnceLock::new();

pub fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

pub fn get_theme() -> &'static Theme {
    SYNTAX_THEME.get_or_init(|| {
        let ts = ThemeSet::load_defaults();
        ts.themes["base16-ocean.dark"].clone()
    })
}

/// 根据文件扩展名查找语法定义
pub fn find_syntax(ext: &str) -> Option<&'static SyntaxReference> {
    let ss = get_syntax_set();
    ss.find_syntax_by_extension(ext)
}

/// 将 syntect Style 转换为 ratatui Style（仅保留前景色，丢弃背景色）
pub fn to_ratatui_style(syntect_style: syntect::highlighting::Style) -> Style {
    let fg = Color::Rgb(
        syntect_style.foreground.r,
        syntect_style.foreground.g,
        syntect_style.foreground.b,
    );
    let mut style = Style::default().fg(fg);
    if syntect_style.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if syntect_style.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if syntect_style.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

/// 从文件路径提取扩展名
pub fn extension_from_path(path: &str) -> &str {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}
