use super::*;
use crate::ui::theme;

/// 检查 span 是否有选区背景色
fn has_selection_bg(style: Style) -> bool {
    matches!(style.bg, Some(theme::SELECTION_BG))
}

#[test]
fn test_highlight_line_spans_full_span() {
    let spans = vec![Span::from("Hello"), Span::from("World")];
    let result = highlight_line_spans(spans, 0, 10);
    assert_eq!(result.len(), 2);
    assert!(has_selection_bg(result[0].style));
    assert!(has_selection_bg(result[1].style));
}

#[test]
fn test_highlight_line_spans_partial_start() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 3, 10);
    // 前 3 字符原样，后 2 字符选区背景
    assert_eq!(result.len(), 2);
    assert!(!has_selection_bg(result[0].style));
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[0].content, "Hel");
    assert_eq!(result[1].content, "lo");
}

#[test]
fn test_highlight_line_spans_partial_both() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 1, 4);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].content, "H");
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[1].content, "ell");
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[2].content, "o");
    assert!(!has_selection_bg(result[2].style));
}

#[test]
fn test_highlight_line_spans_multi_span() {
    let spans = vec![Span::from("Hel"), Span::from("lo Wo"), Span::from("rld")];
    let result = highlight_line_spans(spans, 2, 8);
    // 选中范围 char 2..8 = "llo Wo"
    // span0 "Hel": 前 2 原样 + 后 1 选区背景
    // span1 "lo Wo": 全部选区背景
    // span2 "rld": 不在选区（span2 starts at char 8）
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].content, "He");
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[1].content, "l");
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[2].content, "lo Wo");
    assert!(has_selection_bg(result[2].style));
    assert_eq!(result[3].content, "rld");
    assert!(!has_selection_bg(result[3].style));
}

#[test]
fn test_highlight_line_spans_outside() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 10, 15);
    assert_eq!(result.len(), 1);
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[0].content, "Hello");
}
