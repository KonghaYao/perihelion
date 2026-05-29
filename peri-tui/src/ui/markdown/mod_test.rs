use super::*;

#[test]
fn test_find_last_block_boundary_basic() {
    let text = "paragraph one\n\nparagraph two\n\nparagraph three";
    let prefix_len = "paragraph one\n\nparagraph two\n\n".len();
    let result = find_last_block_boundary(text, prefix_len);
    assert_eq!(result, "paragraph one\n\nparagraph two\n\n".len());
}

#[test]
fn test_find_last_block_boundary_code_fence() {
    // 代码围栏内跳过空行
    let text = "before\n\n```\ncode\n\nmore code\n```\nafter";
    let prefix_len = "before\n\n```\ncode\n\nmore code\n```\n".len();
    let result = find_last_block_boundary(text, prefix_len);
    assert_eq!(result, "before\n\n".len());
}

#[test]
fn test_find_last_block_boundary_unclosed_fence() {
    // 未闭合围栏：边界应在围栏前的 \n\n
    let text = "before\n\n```\nstill open";
    let prefix_len = text.len();
    let result = find_last_block_boundary(text, prefix_len);
    assert_eq!(result, "before\n\n".len());
}

#[test]
fn test_find_last_block_boundary_empty() {
    assert_eq!(find_last_block_boundary("", 0), 0);
    assert_eq!(find_last_block_boundary("hello", 5), 0);
}

#[test]
fn test_find_last_block_boundary_single_paragraph() {
    let text = "just one line";
    assert_eq!(find_last_block_boundary(text, text.len()), 0);
}

#[test]
fn test_find_last_block_boundary_prefix_at_boundary() {
    let text = "aaa\n\nbbb\n\nccc";
    let prefix_len = "aaa\n\nbbb\n\n".len();
    let result = find_last_block_boundary(text, prefix_len);
    assert_eq!(result, "aaa\n\nbbb\n\n".len());
}

#[test]
fn test_find_last_block_boundary_fence_open_close() {
    let text = "para1\n\n```\ncode\n```\n\npara2";
    let prefix_len = text.len();
    let result = find_last_block_boundary(text, prefix_len);
    assert_eq!(result, "para1\n\n```\ncode\n```\n\n".len());
}

/// 辅助：设置 dirty 标志
fn set_dirty(block: &mut ContentBlockView, value: bool) {
    if let ContentBlockView::Text { dirty, .. } = block {
        *dirty = value;
    }
}

/// 辅助：追加文本并标记 dirty
fn append_to_block(block: &mut ContentBlockView, text: &str) {
    if let ContentBlockView::Text { raw, dirty, .. } = block {
        raw.push_str(text);
        *dirty = true;
    }
}

/// 辅助：获取 rendered 行数
fn rendered_line_count(block: &ContentBlockView) -> usize {
    if let ContentBlockView::Text { rendered, .. } = block {
        rendered.lines.len()
    } else {
        0
    }
}

/// 辅助：获取 rendered_prefix_len
fn get_prefix_len(block: &ContentBlockView) -> usize {
    if let ContentBlockView::Text {
        rendered_prefix_len,
        ..
    } = block
    {
        *rendered_prefix_len
    } else {
        0
    }
}

#[test]
fn test_ensure_rendered_incremental_basic() {
    let mut block = ContentBlockView::Text {
        raw: "hello".to_string(),
        rendered: parse_markdown("hello", 80),
        dirty: false,
        rendered_prefix_len: "hello".len(),
        rendered_prefix_lines: 0,
    };
    // 先全量渲染建立基线
    set_dirty(&mut block, true);
    ensure_rendered_incremental(&mut block, 80);
    let baseline_lines = rendered_line_count(&block);

    // 追加新段落，增量解析
    append_to_block(&mut block, "\n\nworld");
    ensure_rendered_incremental(&mut block, 80);

    assert!(rendered_line_count(&block) > baseline_lines, "应该有更多行");
    assert_eq!(get_prefix_len(&block), "hello\n\nworld".len());
}

#[test]
fn test_ensure_rendered_incremental_full_fallback() {
    // rendered_prefix_len==0 且无双换行 → 走全量重解析
    let mut block = ContentBlockView::Text {
        raw: "no boundary".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
    };
    ensure_rendered_incremental(&mut block, 80);

    assert_ne!(rendered_line_count(&block), 0, "应该有渲染输出");
    assert_eq!(get_prefix_len(&block), "no boundary".len());
}

#[test]
fn test_ensure_rendered_incremental_not_dirty() {
    // dirty=false → 直接返回，不触发渲染
    let mut block = ContentBlockView::Text {
        raw: "unchanged".to_string(),
        rendered: Text::raw(""),
        dirty: false,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
    };
    let lines_before = rendered_line_count(&block);
    ensure_rendered_incremental(&mut block, 80);
    assert_eq!(
        rendered_line_count(&block),
        lines_before,
        "不 dirty 时不应修改渲染"
    );
}

#[test]
fn test_ensure_rendered_incremental_no_new_content() {
    // dirty=false 且 raw.len() == rendered_prefix_len → 直接返回
    let mut block = ContentBlockView::Text {
        raw: "hello".to_string(),
        rendered: parse_markdown("hello", 80),
        dirty: false,
        rendered_prefix_len: "hello".len(),
        rendered_prefix_lines: 1,
    };
    let lines_before = rendered_line_count(&block);
    ensure_rendered_incremental(&mut block, 80);
    assert_eq!(
        rendered_line_count(&block),
        lines_before,
        "无新内容时行数不变"
    );
}

#[test]
fn test_ensure_rendered_incremental_code_block_recovery() {
    // 代码块闭合后追加新内容，增量解析应正确工作
    let mut block = ContentBlockView::Text {
        raw: "intro\n\n```\ncode\n```".to_string(),
        rendered: parse_markdown("intro\n\n```\ncode\n```", 80),
        dirty: false,
        rendered_prefix_len: "intro\n\n```\ncode\n```".len(),
        rendered_prefix_lines: 0,
    };
    // 先全量渲染
    set_dirty(&mut block, true);
    ensure_rendered_incremental(&mut block, 80);

    // 追加新内容
    append_to_block(&mut block, "\n\nnew paragraph");
    ensure_rendered_incremental(&mut block, 80);

    assert!(rendered_line_count(&block) > 0);
    assert_eq!(
        get_prefix_len(&block),
        "intro\n\n```\ncode\n```\n\nnew paragraph".len()
    );
}

// ─── Markdown 渲染测试（从 headless_test.rs 迁移）────────────────────────

use ratatui::style::Modifier;

fn all_text(text: &ratatui::text::Text) -> String {
    text.lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn test_md_heading() {
    use peri_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
    let theme = DefaultMarkdownTheme;

    let text = parse_markdown_default("# Hello World");
    let heading_line = &text.lines[1];
    let all_content: String = heading_line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        all_content.contains("Hello World"),
        "H1 应含标题文字，实际: {all_content:?}"
    );
    let has_heading_color = heading_line
        .spans
        .iter()
        .any(|s| s.style.fg == Some(theme.heading()));
    assert!(has_heading_color, "H1 应为 markdown 主题 heading 颜色");
}

#[test]
fn test_md_heading_h2() {
    use peri_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
    let theme = DefaultMarkdownTheme;

    let text = parse_markdown_default("## Section Title");
    let heading_line = &text.lines[1];
    let has_heading_color = heading_line
        .spans
        .iter()
        .any(|s| s.style.fg == Some(theme.heading()));
    assert!(has_heading_color, "H2 应为 markdown 主题 heading 颜色");
}

#[test]
fn test_md_inline_styles() {
    let text = parse_markdown_default("**bold** *italic* ~~strike~~");
    let all = all_text(&text);
    assert!(all.contains("bold"), "应含 bold 文字");
    assert!(all.contains("italic"), "应含 italic 文字");
    assert!(all.contains("strike"), "应含 strike 文字");

    let has_bold = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .any(|s| s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("bold"));
    assert!(has_bold, "bold span 应有 BOLD modifier");

    let has_italic =
        text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
            s.style.add_modifier.contains(Modifier::ITALIC) && s.content.contains("italic")
        });
    assert!(has_italic, "italic span 应有 ITALIC modifier");

    let has_strike = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
        s.style.add_modifier.contains(Modifier::CROSSED_OUT) && s.content.contains("strike")
    });
    assert!(has_strike, "strikethrough span 应有 CROSSED_OUT modifier");
}

#[test]
fn test_md_inline_code() {
    use peri_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
    let theme = DefaultMarkdownTheme;

    let text = parse_markdown_default("`hello`");
    let has_code = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .any(|s| s.style.fg == Some(theme.code()) && s.content.contains("hello"));
    assert!(
        has_code,
        "行内代码应为 markdown 主题 code 颜色，含 hello 文字"
    );
}

#[test]
fn test_md_code_block() {
    let text = parse_markdown_default("```rust\nfn main() {}\n```");
    let all_lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    assert_eq!(
        all_lines.len(),
        1,
        "单行代码块应只产生一行，got: {all_lines:#?}"
    );
    assert!(
        !all_lines[0].contains("[rust]"),
        "单行代码块不应含 [lang] 标签"
    );
    assert!(!all_lines[0].contains('│'), "单行代码块不应含 │ 前缀");
    assert!(all_lines[0].contains("fn main"), "应包含代码内容");
}

#[test]
fn test_md_unordered_list() {
    let text = parse_markdown_default("- item1\n- item2");
    let all_lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    let bullet_lines: Vec<&String> = all_lines.iter().filter(|l| l.contains('•')).collect();
    assert_eq!(
        bullet_lines.len(),
        2,
        "无序列表应有 2 行含 • ，实际:{all_lines:#?}"
    );
}

#[test]
fn test_md_ordered_list() {
    let text = parse_markdown_default("1. first\n2. second");
    let all_lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    let has_one = all_lines.iter().any(|l| l.contains("1."));
    let has_two = all_lines.iter().any(|l| l.contains("2."));
    assert!(has_one, "有序列表应含 1. 前缀，实际:{all_lines:#?}");
    assert!(has_two, "有序列表应含 2. 前缀，实际:{all_lines:#?}");
}

#[test]
fn test_md_blockquote() {
    let text = parse_markdown_default("> quoted text");
    let has_prefix = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .any(|s| s.content.contains('▍'));
    assert!(has_prefix, "引用块应含 ▍ 前缀");
}

#[test]
fn test_md_rule() {
    let text = parse_markdown_default("---");
    let has_rule = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .any(|s| s.content.matches('─').count() >= 10);
    assert!(has_rule, "水平线应含多个 ─ 字符");
}

#[test]
fn test_md_incomplete_does_not_panic() {
    let text = parse_markdown_default("**unclosed bold");
    let all = all_text(&text);
    assert!(
        all.contains("unclosed bold"),
        "不完整 Markdown 应降级为纯文本，实际: {all:?}"
    );
}

#[test]
fn test_md_table_basic() {
    let md = "| Name  | Value |\n|-------|-------|\n| foo   | 123   |\n| bar   | 456   |";
    let text = parse_markdown_default(md);
    let all = all_text(&text);
    assert!(
        all.contains("Name"),
        "Table should contain header 'Name', got: {all:?}"
    );
    assert!(
        all.contains("foo"),
        "Table should contain data 'foo', got: {all:?}"
    );
    assert!(
        all.contains("456"),
        "Table should contain data '456', got: {all:?}"
    );
    assert!(
        all.contains("│"),
        "Table should have vertical borders, got: {all:?}"
    );
    assert!(
        all.contains("┌"),
        "Table should have top-left corner, got: {all:?}"
    );
    assert!(
        all.contains("└"),
        "Table should have bottom-left corner, got: {all:?}"
    );
    assert!(
        all.contains("┼"),
        "Table should have header separator, got: {all:?}"
    );
}

#[test]
fn test_md_table_cell_count() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |";
    let text = parse_markdown_default(md);
    assert_eq!(
        text.lines.len(),
        5,
        "2-col table should produce 5 lines, got: {}",
        text.lines.len()
    );
}

#[test]
fn test_md_table_border_alignment() {
    let md = "| Name | Value |\n|------|-------|\n| foo  | 123   |";
    let text = parse_markdown_default(md);
    for (i, line) in text.lines.iter().enumerate() {
        let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        eprintln!(
            "line {}: {:?} (chars={})",
            i,
            content,
            content.chars().count()
        );
    }
    let widths: Vec<usize> = text
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|s| s.content.chars().count())
                .sum::<usize>()
        })
        .collect();
    let unique_widths: std::collections::HashSet<usize> = widths.iter().copied().collect();
    assert!(
        unique_widths.len() == 1,
        "All table lines should have same visual width, got: {:?}",
        widths
    );
}

#[test]
fn test_md_table_alignment() {
    let md = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a    | b      | c     |";
    let text = parse_markdown_default(md);
    let all = all_text(&text);
    assert!(
        all.contains("Left"),
        "Should contain 'Left' header, got: {all:?}"
    );
    assert!(all.contains("a"), "Should contain data 'a', got: {all:?}");
}

#[test]
fn test_md_table_with_inline_code() {
    let md = "| Command |\n|---------|\n| `ls`    |";
    let text = parse_markdown_default(md);
    let all = all_text(&text);
    assert!(
        all.contains("ls"),
        "Should contain inline code content, got: {all:?}"
    );
}
