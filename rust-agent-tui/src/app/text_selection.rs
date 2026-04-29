/// 文本选区状态
#[derive(Debug, Clone)]
pub struct TextSelection {
    /// 选区起始视觉坐标（相对于消息区域左上角）
    pub start: Option<(u16, u16)>, // (visual_row, visual_col)
    /// 选区结束视觉坐标
    pub end: Option<(u16, u16)>,
    /// 是否正在拖拽中
    pub dragging: bool,
    /// 选区对应的纯文本内容（松开鼠标后计算）
    pub selected_text: Option<String>,
}

impl TextSelection {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            dragging: false,
            selected_text: None,
        }
    }

    /// 开始拖拽：记录起始坐标，清除旧选区
    pub fn start_drag(&mut self, row: u16, col: u16) {
        self.start = Some((row, col));
        self.end = Some((row, col));
        self.dragging = true;
        self.selected_text = None;
    }

    /// 更新拖拽：更新结束坐标
    pub fn update_drag(&mut self, row: u16, col: u16) {
        if self.dragging {
            self.end = Some((row, col));
        }
    }

    /// 结束拖拽：标记拖拽结束，selected_text 由外部计算后通过 set_selected_text 设置
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// 设置提取后的选区文本
    pub fn set_selected_text(&mut self, text: Option<String>) {
        self.selected_text = text;
    }

    /// 清除选区（鼠标点击非拖拽、复制后、resize 后调用）
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.dragging = false;
        self.selected_text = None;
    }

    /// 是否有活跃的选区（正在拖拽或已选中文字）
    pub fn is_active(&self) -> bool {
        self.dragging || self.selected_text.is_some()
    }
}

/// 面板文字选区状态（用于 thread_browser / agent / cron 等列表面板）
#[derive(Debug, Clone)]
pub struct PanelTextSelection {
    /// 选区起始坐标（内容空间：row 已包含 scroll offset）
    pub start: Option<(u16, u16)>, // (content_row, col)
    /// 选区结束坐标
    pub end: Option<(u16, u16)>,
    /// 是否正在拖拽中
    pub dragging: bool,
    /// 选区对应的纯文本内容
    pub selected_text: Option<String>,
}

impl PanelTextSelection {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            dragging: false,
            selected_text: None,
        }
    }

    pub fn start_drag(&mut self, row: u16, col: u16) {
        self.start = Some((row, col));
        self.end = Some((row, col));
        self.dragging = true;
        self.selected_text = None;
    }

    pub fn update_drag(&mut self, row: u16, col: u16) {
        if self.dragging {
            self.end = Some((row, col));
        }
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    pub fn set_selected_text(&mut self, text: Option<String>) {
        self.selected_text = text;
    }

    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.dragging = false;
        self.selected_text = None;
    }

    pub fn is_active(&self) -> bool {
        self.dragging || self.selected_text.is_some()
    }
}

/// 从面板纯文本行中提取选区文本（字符级精度）。
/// start/end 为内容空间坐标 (content_row, col)。
/// 自动处理 start > end 的情况。
pub fn extract_panel_text(
    start: (u16, u16),
    end: (u16, u16),
    plain_lines: &[String],
) -> Option<String> {
    let ((sr, sc), (er, ec)) = if start <= end { (start, end) } else { (end, start) };
    let sr = sr as usize;
    let er = er as usize;
    if sr >= plain_lines.len() {
        return None;
    }
    let er = er.min(plain_lines.len() - 1);

    let mut parts: Vec<String> = Vec::new();
    for i in sr..=er {
        let text = &plain_lines[i];
        if sr == er {
            // 同一行
            let b_start = char_to_byte_idx(text, sc as usize);
            let b_end = char_to_byte_idx(text, ec as usize);
            if b_start >= b_end {
                return None;
            }
            parts.push(text[b_start..b_end].to_string());
        } else if i == sr {
            // 首行：从 sc 到行尾
            let b_start = char_to_byte_idx(text, sc as usize);
            parts.push(text[b_start..].to_string());
        } else if i == er {
            // 末行：从行首到 ec
            let b_end = char_to_byte_idx(text, ec as usize);
            parts.push(text[..b_end].to_string());
        } else {
            // 中间行：整行
            parts.push(text.clone());
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// 在 char_widths 中定位到第 row_in_line 个视觉行，在该视觉行内
/// 累积宽度到 visual_col，返回字符偏移量。
fn char_col_to_offset(
    char_widths: &[u8],
    visual_col: u16,
    row_in_line: usize,
    usable_width: u16,
) -> usize {
    let uw = usable_width as usize;
    if uw == 0 || char_widths.is_empty() {
        return 0;
    }
    // 定位到第 row_in_line 个视觉行的起始字符偏移
    let mut line_start = 0;
    let mut current_row = 0;
    let mut col_in_line: usize = 0;
    for (i, &w) in char_widths.iter().enumerate() {
        let w = w as usize;
        if col_in_line + w > uw {
            current_row += 1;
            col_in_line = w;
            line_start = i;
        } else {
            col_in_line += w;
        }
        if current_row >= row_in_line {
            break;
        }
    }
    // 在当前视觉行内累积到 visual_col
    let target = visual_col as usize;
    let mut accumulated: usize = 0;
    let mut offset = line_start;
    for (i, &w) in char_widths[line_start..].iter().enumerate() {
        let w = w as usize;
        if accumulated + w > target {
            break;
        }
        accumulated += w;
        offset = line_start + i + 1;
        if accumulated >= target {
            break;
        }
    }
    offset
}

/// 将视觉坐标 (visual_row, visual_col) 通过 wrap_map 映射为 (line_idx, char_offset)。
/// `usable_width` 为消息区域可用宽度（右侧留 1 列给滚动条后）。
pub fn visual_to_logical(
    visual_row: u16,
    visual_col: u16,
    wrap_map: &[crate::ui::render_thread::WrappedLineInfo],
    usable_width: u16,
) -> Option<(usize, usize)> {
    let idx = wrap_map.partition_point(|info| info.visual_row_end <= visual_row);
    if idx >= wrap_map.len() {
        return None;
    }
    let info = &wrap_map[idx];
    if visual_row < info.visual_row_start {
        return None;
    }
    let row_in_line = (visual_row - info.visual_row_start) as usize;
    let char_offset = char_col_to_offset(&info.char_widths, visual_col, row_in_line, usable_width);
    Some((info.line_idx, char_offset))
}

/// 将字符索引转换为字节索引，用于安全切割 String。
/// `char_idx` 是 text 中的字符位置（从 0 开始）。
/// 返回对应的 byte 偏移量。如果 char_idx 超出字符数，返回 text.len()。
fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// 根据选区起止坐标从 wrap_map 的 plain_text 提取文本（字符级精度）。
/// 自动处理 start > end 的情况（swap）。
/// 首行从 start_col 对应的字符位置截取，末行到 end_col 对应的字符位置截取，中间行整行。
/// 所有 char offset 通过 char_to_byte_idx 转为 byte 索引后切割，保证 unicode 安全。
pub fn extract_selected_text(
    start: (u16, u16),
    end: (u16, u16),
    wrap_map: &[crate::ui::render_thread::WrappedLineInfo],
    usable_width: u16,
) -> Option<String> {
    let ((start_row, start_col), (end_row, end_col)) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };

    let start_idx = wrap_map.partition_point(|info| info.visual_row_end <= start_row);
    let end_idx = wrap_map.partition_point(|info| info.visual_row_end <= end_row);

    if start_idx >= wrap_map.len() {
        return None;
    }
    let end_idx = end_idx.min(wrap_map.len() - 1);

    let mut parts: Vec<String> = Vec::new();

    for i in start_idx..=end_idx {
        let info = &wrap_map[i];
        let text = &info.plain_text;

        if start_idx == end_idx {
            // 同一逻辑行：截取 [start_char, end_char)
            let row_in_start = (start_row - info.visual_row_start) as usize;
            let row_in_end = (end_row - info.visual_row_start) as usize;
            let c_start = char_col_to_offset(&info.char_widths, start_col, row_in_start, usable_width);
            let c_end = char_col_to_offset(&info.char_widths, end_col, row_in_end, usable_width);
            let b_start = char_to_byte_idx(text, c_start);
            let b_end = char_to_byte_idx(text, c_end);
            if b_start >= b_end {
                return None;
            }
            parts.push(text[b_start..b_end].to_string());
        } else if i == start_idx {
            // 首行：从 start_col 对应的字符位置到行尾
            let row_in_line = (start_row - info.visual_row_start) as usize;
            let c_start = char_col_to_offset(&info.char_widths, start_col, row_in_line, usable_width);
            let b_start = char_to_byte_idx(text, c_start);
            parts.push(text[b_start..].to_string());
        } else if i == end_idx {
            // 末行：从行首到 end_col 对应的字符位置
            let row_in_line = (end_row - info.visual_row_start) as usize;
            let c_end = char_col_to_offset(&info.char_widths, end_col, row_in_line, usable_width);
            let b_end = char_to_byte_idx(text, c_end);
            parts.push(text[..b_end].to_string());
        } else {
            // 中间行：整行
            parts.push(text.to_string());
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_drag_sets_coords() {
        let mut ts = TextSelection::new();
        ts.start_drag(5, 10);
        assert_eq!(ts.start, Some((5, 10)));
        assert_eq!(ts.end, Some((5, 10)));
        assert!(ts.dragging);
        assert!(ts.selected_text.is_none());
    }

    #[test]
    fn test_update_drag_moves_end() {
        let mut ts = TextSelection::new();
        ts.start_drag(0, 0);
        ts.update_drag(3, 8);
        assert_eq!(ts.start, Some((0, 0)));
        assert_eq!(ts.end, Some((3, 8)));
    }

    #[test]
    fn test_end_drag_stops_dragging() {
        let mut ts = TextSelection::new();
        ts.start_drag(1, 2);
        ts.end_drag();
        assert!(!ts.dragging);
        assert_eq!(ts.start, Some((1, 2)));
        assert_eq!(ts.end, Some((1, 2)));
    }

    #[test]
    fn test_clear_resets_all() {
        let mut ts = TextSelection::new();
        ts.start_drag(5, 10);
        ts.update_drag(8, 20);
        ts.end_drag();
        ts.set_selected_text(Some("hello".into()));
        ts.clear();
        assert!(ts.start.is_none());
        assert!(ts.end.is_none());
        assert!(!ts.dragging);
        assert!(ts.selected_text.is_none());
    }

    #[test]
    fn test_is_active() {
        let mut ts = TextSelection::new();
        assert!(!ts.is_active());
        ts.start_drag(0, 0);
        assert!(ts.is_active());
        ts.end_drag();
        assert!(!ts.is_active());
        ts.set_selected_text(Some("x".into()));
        assert!(ts.is_active());
    }

    // --- Task 3: 坐标映射和文本提取测试 ---

    fn make_wrap_map_entry(line_idx: usize, start: u16, end: u16, text: &str) -> crate::ui::render_thread::WrappedLineInfo {
        let char_widths: Vec<u8> = text.chars()
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u8)
            .collect();
        crate::ui::render_thread::WrappedLineInfo {
            line_idx,
            visual_row_start: start,
            visual_row_end: end,
            plain_text: text.to_string(),
            char_widths,
        }
    }

    #[test]
    fn test_visual_to_logical_basic() {
        let wrap_map = vec![
            make_wrap_map_entry(0, 0, 1, "Hello"),
            make_wrap_map_entry(1, 1, 2, "World"),
        ];
        assert_eq!(visual_to_logical(0, 0, &wrap_map, 80), Some((0, 0)));
        assert_eq!(visual_to_logical(1, 0, &wrap_map, 80), Some((1, 0)));
    }

    #[test]
    fn test_visual_to_logical_out_of_range() {
        let wrap_map = vec![
            make_wrap_map_entry(0, 0, 1, "Hello"),
        ];
        assert_eq!(visual_to_logical(99, 0, &wrap_map, 80), None);
    }

    #[test]
    fn test_extract_selected_text_single_line() {
        let wrap_map = vec![
            make_wrap_map_entry(0, 0, 1, "Hello World"),
        ];
        let result = extract_selected_text((0, 2), (0, 8), &wrap_map, 80);
        // char 2..8 of "Hello World" = "llo Wo"
        assert_eq!(result, Some("llo Wo".to_string()));
    }

    #[test]
    fn test_extract_selected_text_multi_line() {
        let wrap_map = vec![
            make_wrap_map_entry(0, 0, 1, "Line0"),
            make_wrap_map_entry(1, 1, 2, "Line1"),
            make_wrap_map_entry(2, 2, 3, "Line2"),
        ];
        let result = extract_selected_text((0, 0), (2, 5), &wrap_map, 80);
        assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
    }

    #[test]
    fn test_extract_selected_text_swapped() {
        let wrap_map = vec![
            make_wrap_map_entry(0, 0, 1, "Line0"),
            make_wrap_map_entry(1, 1, 2, "Line1"),
            make_wrap_map_entry(2, 2, 3, "Line2"),
        ];
        let result = extract_selected_text((2, 5), (0, 0), &wrap_map, 80);
        assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
    }

    #[test]
    fn test_extract_selected_text_partial_first_and_last() {
        let wrap_map = vec![
            make_wrap_map_entry(0, 0, 1, "Hello"),
            make_wrap_map_entry(1, 1, 2, "World"),
        ];
        let result = extract_selected_text((0, 2), (1, 3), &wrap_map, 80);
        assert_eq!(result, Some("llo\nWor".to_string()));
    }

    #[test]
    fn test_char_col_to_offset_ascii() {
        let char_widths = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1]; // "ABCDEFGHIJ"
        let offset = char_col_to_offset(&char_widths, 5, 0, 80);
        assert_eq!(offset, 5);
    }

    #[test]
    fn test_char_col_to_offset_cjk() {
        let char_widths = vec![2, 2, 2, 2]; // "你好世界"
        let offset = char_col_to_offset(&char_widths, 4, 0, 80);
        assert_eq!(offset, 2);
    }

    // --- PanelTextSelection tests ---

    #[test]
    fn test_panel_selection_lifecycle() {
        let mut ps = PanelTextSelection::new();
        assert!(!ps.is_active());
        ps.start_drag(2, 5);
        assert!(ps.is_active());
        assert_eq!(ps.start, Some((2, 5)));
        assert_eq!(ps.end, Some((2, 5)));
        ps.update_drag(4, 10);
        assert_eq!(ps.end, Some((4, 10)));
        ps.end_drag();
        assert!(!ps.dragging);
        ps.set_selected_text(Some("test".into()));
        assert!(ps.is_active());
        ps.clear();
        assert!(!ps.is_active());
    }

    // --- extract_panel_text tests ---

    #[test]
    fn test_extract_panel_text_single_line() {
        let lines = vec!["Hello World".to_string()];
        let result = extract_panel_text((0, 2), (0, 8), &lines);
        assert_eq!(result, Some("llo Wo".to_string()));
    }

    #[test]
    fn test_extract_panel_text_multi_line() {
        let lines = vec!["Line0".to_string(), "Line1".to_string(), "Line2".to_string()];
        let result = extract_panel_text((0, 0), (2, 5), &lines);
        assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
    }

    #[test]
    fn test_extract_panel_text_swapped() {
        let lines = vec!["Line0".to_string(), "Line1".to_string(), "Line2".to_string()];
        let result = extract_panel_text((2, 5), (0, 0), &lines);
        assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
    }

    #[test]
    fn test_extract_panel_text_partial() {
        let lines = vec!["Hello".to_string(), "World".to_string()];
        let result = extract_panel_text((0, 2), (1, 3), &lines);
        assert_eq!(result, Some("llo\nWor".to_string()));
    }

    #[test]
    fn test_extract_panel_text_out_of_range() {
        let lines = vec!["Hello".to_string()];
        let result = extract_panel_text((5, 0), (5, 3), &lines);
        assert_eq!(result, None);
    }
}
