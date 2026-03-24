use ratatui::text::Text;

use super::message_view::ContentBlockView;

/// 解析 markdown 文本为 ratatui Text
/// 目前简单返回原始文本（后续可替换为更复杂的 markdown 渲染）
pub fn parse_markdown(input: &str) -> Text<'static> {
    if input.is_empty() {
        return Text::raw("");
    }
    // TODO: 实现更完整的 markdown 渲染
    Text::from(input.to_string())
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
