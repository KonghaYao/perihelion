use peri_middlewares::ask_user::AskUserQuestionData;

/// 精确计算 AskUser 弹窗内容行数，与 render_ask_user_popup 的 lines 结构 1:1 对齐。
///
/// 结构：问题文本 + 空行 + [选项行 + 描述行 + 选项间空行]×N + 空行 + 自定义输入行
/// 返回值包含 header(1) + 分隔线(1) + BorderedPanel 上下边框(2) 的开销。
pub(crate) fn ask_user_content_height(q: &AskUserQuestionData, panel_width: usize) -> u16 {
    let w = panel_width.max(1) as u16;
    let mut lines: u16 = 0;

    // 问题文本（考虑自动换行）
    for line in q.question.lines() {
        let line_w = unicode_width::UnicodeWidthStr::width(line) as u16;
        lines += line_w.div_ceil(w);
    }
    // 问题后空行
    lines += 1;

    let multi = q.multi_select;
    let option_count = q.options.len();

    for (i, opt) in q.options.iter().enumerate() {
        // 选项 label 行
        // 单选前缀: "❯ N. " → ❯(2) + " "(1) + digits + ". "(2) ≈ 6
        // 多选前缀: "❯ ○ N. " → ❯(2) + " "(1) + ○(1) + " "(1) + digits + ". "(2) ≈ 8-10
        let prefix_w: u16 = if multi { 10 } else { 6 };
        let label_w = unicode_width::UnicodeWidthStr::width(opt.label.as_str()) as u16 + prefix_w;
        lines += label_w.div_ceil(w);

        // 选项 description 行（若有）
        if let Some(ref desc) = opt.description {
            if !desc.is_empty() {
                // 多选缩进 "       " (7)，单选缩进 "     " (5)
                let indent_w: u16 = if multi { 7 } else { 5 };
                let desc_w = unicode_width::UnicodeWidthStr::width(desc.as_str()) as u16 + indent_w;
                lines += desc_w.div_ceil(w);
            }
        }

        // 选项之间空行（最后一个选项不加）
        if i < option_count - 1 {
            lines += 1;
        }
    }

    // 自定义输入前空行 + 自定义输入行
    lines += 2;

    // header tab 行 + 分隔线 + BorderedPanel 上下边框 = 4
    lines + 4
}
