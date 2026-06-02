pub mod cache;
mod render_state;

#[cfg(feature = "markdown-highlight")]
mod highlight;

use pulldown_cmark::{Options, Parser};
use ratatui::{style::Color, text::Text};

use cache::MarkdownCache;
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

/// 将 `Theme` trait 适配为 `MarkdownTheme`
///
/// 语义映射：
/// - heading → warning（标题用警告色，增强可读性）
/// - code → thinking（行内代码用思考色，蓝紫调）
/// - link → success（链接用成功色，绿色标识可点击）
/// - code_prefix → success
/// - quote_prefix → muted
/// - list_bullet → text
/// - separator → muted
pub struct ThemeMarkdownAdapter<'a>(pub &'a dyn crate::theme::Theme);

impl MarkdownTheme for ThemeMarkdownAdapter<'_> {
    fn heading(&self) -> Color {
        self.0.warning()
    }
    fn text(&self) -> Color {
        self.0.text()
    }
    fn muted(&self) -> Color {
        self.0.muted()
    }
    fn code(&self) -> Color {
        self.0.thinking()
    }
    fn link(&self) -> Color {
        self.0.success()
    }
    fn code_prefix(&self) -> Color {
        self.0.success()
    }
    fn quote_prefix(&self) -> Color {
        self.0.muted()
    }
    fn list_bullet(&self) -> Color {
        self.0.text()
    }
    fn separator(&self) -> Color {
        self.0.muted()
    }
}

/// 默认 Markdown 主题——色值与 DarkTheme 一致
#[derive(Debug, Clone)]
pub struct DefaultMarkdownTheme;

impl MarkdownTheme for DefaultMarkdownTheme {
    fn heading(&self) -> Color {
        Color::Rgb(255, 193, 7)
    } // WARNING #FFC107
    fn text(&self) -> Color {
        Color::Rgb(255, 255, 255)
    } // TEXT #FFFFFF
    fn muted(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
    fn code(&self) -> Color {
        Color::Rgb(162, 169, 228)
    } // THINKING #A2A9E4（蓝紫色）
    fn link(&self) -> Color {
        Color::Rgb(78, 186, 101)
    } // SAGE #4EBA65
    fn code_prefix(&self) -> Color {
        Color::Rgb(78, 186, 101)
    } // SAGE #4EBA65
    fn quote_prefix(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
    fn list_bullet(&self) -> Color {
        Color::Rgb(255, 255, 255)
    } // TEXT #FFFFFF
    fn separator(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
}

/// 解析 markdown 文本为 ratatui Text（带 LRU 缓存）
///
/// 缓存 key = (content_hash, max_width)，全局单例共享。
/// 命中时直接返回克隆的 Text<'static>，跳过完整解析。
pub fn parse_markdown(input: &str, theme: &dyn MarkdownTheme, max_width: usize) -> Text<'static> {
    if input.is_empty() {
        return Text::raw("");
    }

    // 检查缓存
    let width_u16 = max_width.min(u16::MAX as usize) as u16;
    let cache = MarkdownCache::global();
    if let Some(cached) = cache.get(input, width_u16) {
        return cached;
    }

    // 缓存未命中，执行完整解析
    let options = Options::all() - Options::ENABLE_SMART_PUNCTUATION;
    let parser = Parser::new_ext(input, options);
    let mut state = RenderState::new(theme).with_max_width(max_width);
    for event in parser {
        state.handle_event(event);
    }
    if !state.current_spans.is_empty() {
        state.flush_line();
    }
    // 裁剪尾部空行，避免最后一个块级元素后多余留白
    while state.lines.last().is_some_and(|l| l.spans.is_empty()) {
        state.lines.pop();
    }
    let result = Text::from(state.lines);

    // 写入缓存
    cache.put(input, width_u16, result.clone());

    result
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

#[cfg(test)]
#[path = "cache_test.rs"]
mod cache_tests;
