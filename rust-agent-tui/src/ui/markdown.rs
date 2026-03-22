use ratatui::text::Text;

use super::message_view::ContentBlockView;

/// 解析 markdown 文本为 ratatui Text
pub fn parse_markdown(input: &str) -> Text<'static> {
    if input.is_empty() {
        return Text::raw("");
    }
    let rendered = tui_markdown::from_str(input);
    // 转换为 'static 生命周期（克隆文本内容）
    Text::from(rendered.to_string())
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
