use ratatui::text::Text;

use perihelion_widgets::DefaultMarkdownTheme;

use super::message_view::ContentBlockView;

static THEME: DefaultMarkdownTheme = DefaultMarkdownTheme;

/// 解析 markdown 文本为 ratatui Text
pub fn parse_markdown(input: &str, max_width: usize) -> Text<'static> {
    perihelion_widgets::markdown::parse_markdown(input, &THEME, max_width)
}

/// 解析 markdown 文本为 ratatui Text（使用默认宽度 80）
pub fn parse_markdown_default(input: &str) -> Text<'static> {
    parse_markdown(input, 80)
}

/// 确保 block 已渲染（dirty 为 true 时重新解析）
pub fn ensure_rendered(block: &mut ContentBlockView, max_width: usize) {
    if let ContentBlockView::Text { raw, rendered, dirty } = block {
        if *dirty {
            *rendered = parse_markdown(raw, max_width);
            *dirty = false;
        }
    }
}
