mod render_state;

use pulldown_cmark::{Options, Parser};
use ratatui::text::Text;

use super::message_view::ContentBlockView;

use render_state::RenderState;

// ── 公共接口 ─────────────────────────────────────────────────────────────────

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
