//! 文本编辑器核心类型与 TextEditor 结构体。
//!
//! 提供基于 [`Rope`](ropey::Rope) 的文件编辑能力，包括光标定位、选区管理、
//! undo/redo 栈、滚动偏移等。不涉及渲染逻辑，仅负责数据层。

pub mod input;
pub mod render;

use anyhow::{Context, Result};
use ratatui::style::Style;
use ropey::Rope;
use std::fs;
use std::path::PathBuf;
use unicode_width::UnicodeWidthChar;

/// 单行最大显示列数（防止超长行拖慢渲染）
pub(crate) const MAX_DISPLAY_COLS: usize = 2000;

/// Tab 显示宽度
const TAB_WIDTH: usize = 4;

/// 统一的字符显示宽度计算。
/// Tab 按 TAB_WIDTH 计算，其余按 unicode-width 库。
pub fn char_width(ch: char) -> usize {
    if ch == '\t' {
        TAB_WIDTH
    } else {
        UnicodeWidthChar::width(ch).unwrap_or(0)
    }
}

// ── CursorPos ──

/// 光标位置（行号和列号，均为字符索引，0-based）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub line: usize,
    pub col: usize,
}

impl CursorPos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl PartialOrd for CursorPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CursorPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line.cmp(&other.line).then(self.col.cmp(&other.col))
    }
}

// ── EditAction ──

/// 编辑操作记录（用于 undo/redo）。
#[derive(Debug, Clone)]
enum EditAction {
    /// 在 `pos` 位置插入了 `text`。
    Insert { pos: CursorPos, text: String },
    /// 在 `pos` 位置删除了 `text`。
    Delete { pos: CursorPos, text: String },
}

// ── TextEditor ──

/// 基于 Rope 的文本编辑器数据结构。
///
/// 管理 [ropey::Rope] 文本缓冲区、光标、选区、undo/redo 栈和滚动状态。
/// 不持有任何终端/渲染状态。
pub struct TextEditor {
    rope: Rope,
    path: PathBuf,
    cursor: CursorPos,
    selection_anchor: Option<CursorPos>,
    scroll_y: usize,
    scroll_x: usize,
    modified: bool,
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,

    // 语法高亮
    /// 每行的高亮结果：None = 未高亮，Some = 已高亮
    highlight_cache: Vec<Option<Vec<(Style, String)>>>,
    /// 标记缓存是否需要重建（编辑后置 true，下次渲染前同步重建）
    highlight_dirty: bool,
}

impl TextEditor {
    // ── 文件 I/O ──

    /// 从磁盘加载文件到 Rope 缓冲区。
    ///
    /// 文件不存在时创建空缓冲区。读取失败返回错误。
    pub fn open(path: PathBuf) -> Result<Self> {
        let rope = if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("无法读取文件: {}", path.display()))?;
            Rope::from(content)
        } else {
            Rope::new()
        };
        Ok(Self {
            rope,
            path: path.clone(),
            cursor: CursorPos::new(0, 0),
            selection_anchor: None,
            scroll_y: 0,
            scroll_x: 0,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            highlight_cache: Vec::new(),
            highlight_dirty: true,
        })
    }

    /// 同步重建全部语法高亮。文件 ≤ 5000 行时每次编辑后调用（< 1ms）。
    /// 超过限制则跳过，降级为纯文本。
    pub fn rehighlight_all(&mut self) {
        let ext = crate::ui::syntax::extension_from_path(self.path.to_str().unwrap_or(""));
        let syntax = match crate::ui::syntax::find_syntax(ext) {
            Some(s) => s,
            None => {
                let total = self.rope.len_lines();
                self.highlight_cache = vec![None; total];
                self.highlight_dirty = false;
                return;
            }
        };

        let total = self.rope.len_lines();
        // 超长文件跳过高亮（降级为纯文本）
        if total > 5000 {
            self.highlight_cache = vec![None; total];
            self.highlight_dirty = false;
            return;
        }

        let theme = crate::ui::syntax::get_theme();
        let ss = crate::ui::syntax::get_syntax_set();
        let mut h = syntect::easy::HighlightLines::new(syntax, theme);

        self.highlight_cache.clear();
        self.highlight_cache.reserve(total);

        for i in 0..total {
            let line = self.line_text(i);
            let spans = match h.highlight_line(&line, ss) {
                Ok(segments) => segments
                    .into_iter()
                    .map(|(s, t)| (crate::ui::syntax::to_ratatui_style(s), t.to_string()))
                    .collect(),
                Err(_) => vec![(Style::default(), line)],
            };
            self.highlight_cache.push(Some(spans));
        }
        self.highlight_dirty = false;
    }

    /// 主循环调用：缓存 dirty 时同步重建高亮。
    /// 只处理到可视区域末尾，避免大文件全量重建。
    pub fn sync_highlight_visible(&mut self, scroll_y: usize, viewport_height: usize) -> bool {
        if !self.highlight_dirty {
            return false;
        }

        let ext = crate::ui::syntax::extension_from_path(self.path.to_str().unwrap_or(""));
        let syntax = match crate::ui::syntax::find_syntax(ext) {
            Some(s) => s,
            None => {
                let total = self.rope.len_lines();
                self.highlight_cache.resize(total, None);
                self.highlight_dirty = false;
                return true;
            }
        };

        let total = self.rope.len_lines();
        self.highlight_cache.resize(total, None);

        let end = (scroll_y + viewport_height).min(total);

        let theme = crate::ui::syntax::get_theme();
        let ss = crate::ui::syntax::get_syntax_set();
        let mut h = syntect::easy::HighlightLines::new(syntax, theme);

        for i in 0..end {
            let line = self.line_text(i);
            let spans = match h.highlight_line(&line, ss) {
                Ok(segments) => segments
                    .into_iter()
                    .map(|(s, t)| (crate::ui::syntax::to_ratatui_style(s), t.to_string()))
                    .collect(),
                Err(_) => vec![(Style::default(), line)],
            };
            self.highlight_cache[i] = Some(spans);
        }
        self.highlight_dirty = false;
        true
    }

    /// 兼容旧调用
    pub fn sync_highlight(&mut self) -> bool {
        if !self.highlight_dirty {
            return false;
        }
        self.rehighlight_all();
        true
    }

    /// 获取指定行的高亮 spans（如果已缓存）
    pub fn line_highlights(&self, line: usize) -> Option<&[(Style, String)]> {
        self.highlight_cache
            .get(line)
            .and_then(|opt| opt.as_deref())
    }

    /// 编辑后标记缓存失效
    fn invalidate_highlight(&mut self) {
        self.highlight_dirty = true;
    }

    /// 将缓冲区内容写入磁盘。成功后清除 modified 标记。
    pub fn save(&mut self) -> Result<()> {
        let content = self.rope.to_string();
        fs::write(&self.path, &content)
            .with_context(|| format!("无法写入文件: {}", self.path.display()))?;
        self.modified = false;
        Ok(())
    }

    // ── 只读访问器 ──

    /// 文件是否被修改过（相对于打开时）。
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// 关联的文件路径。
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// 缓冲区总行数。
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// 当前光标位置。
    pub fn cursor(&self) -> CursorPos {
        self.cursor
    }

    /// 选区锚点（如果存在）。
    pub fn selection(&self) -> Option<CursorPos> {
        self.selection_anchor
    }

    /// 是否存在选区。
    pub fn has_selection(&self) -> bool {
        self.selection_anchor.is_some()
    }

    /// 返回有序选区范围 `(start, end)`，保证 start <= end。
    /// 无选区时返回 None。
    pub fn selection_range(&self) -> Option<(CursorPos, CursorPos)> {
        self.selection_anchor.map(|anchor| {
            let start = anchor.min(self.cursor);
            let end = anchor.max(self.cursor);
            (start, end)
        })
    }

    /// 获取选区文本。无选区时返回空字符串。
    pub fn selected_text(&self) -> String {
        match self.selection_anchor {
            Some(anchor) => {
                let start = anchor.min(self.cursor);
                let end = anchor.max(self.cursor);
                let start_idx = self.pos_to_char(start);
                let end_idx = self.pos_to_char(end);
                self.rope.slice(start_idx..end_idx).to_string()
            }
            None => String::new(),
        }
    }

    /// 清除选区。
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// 全选（锚点放在文件起始，光标放在文件末尾）。
    pub fn select_all(&mut self) {
        self.selection_anchor = Some(CursorPos::new(0, 0));
        let last_line = self.line_count().saturating_sub(1);
        let last_col = self.line_content_len(last_line);
        self.cursor = CursorPos::new(last_line, last_col);
    }

    // ── 行/位置工具 ──

    /// 指定行的内容长度（不含换行符）。
    pub fn line_content_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let rope_line = self.rope.line(line);
        // ropey 行末含 \n，len_chars() 包含它；最后非空行可能不含 \n
        let len = rope_line.len_chars();
        if len > 0 && rope_line.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    /// 获取指定行文本（不含换行符）。
    pub fn line_text(&self, line: usize) -> String {
        if line >= self.rope.len_lines() {
            return String::new();
        }
        let rope_line = self.rope.line(line);
        let len = rope_line.len_chars();
        if len > 0 && rope_line.char(len - 1) == '\n' {
            rope_line.slice(0..len - 1).to_string()
        } else {
            rope_line.to_string()
        }
    }

    /// 计算指定行的显示宽度（所有字符的 display width 之和）。
    pub fn line_display_width(&self, line: usize) -> usize {
        let text = self.line_text(line);
        text.chars().map(|ch| char_width(ch)).sum()
    }

    /// [`CursorPos`] 转换为 Rope 绝对字符索引。
    pub fn pos_to_char(&self, pos: CursorPos) -> usize {
        if pos.line == 0 {
            pos.col
        } else {
            // ropey line(i) 从行起始开始；行索引 + 列偏移
            let line_start = self.rope.line_to_char(pos.line);
            line_start + pos.col
        }
    }

    /// 将位置钳位到有效范围。
    pub fn clamp_pos(&self, pos: CursorPos) -> CursorPos {
        let line = pos.line.min(self.line_count().saturating_sub(1));
        let col = pos.col.min(self.line_content_len(line));
        CursorPos::new(line, col)
    }

    /// 将当前光标钳位到有效范围。
    pub fn clamp_cursor(&mut self) {
        self.cursor = self.clamp_pos(self.cursor);
    }

    // ── 编辑操作 ──

    /// 在光标位置插入单个字符。若存在选区则先删除。
    pub fn insert_char(&mut self, ch: char) {
        self.delete_selection_if_any();
        let pos = self.cursor;
        let char_idx = self.pos_to_char(pos);
        self.rope.insert_char(char_idx, ch);
        // 更新光标
        if ch == '\n' {
            self.cursor = CursorPos::new(pos.line + 1, 0);
        } else {
            self.cursor = CursorPos::new(pos.line, pos.col + 1);
        }
        self.push_undo(EditAction::Insert {
            pos,
            text: ch.to_string(),
        });
        self.modified = true;
    }

    /// 在光标位置插入字符串（用于粘贴）。处理多行文本。
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection_if_any();
        let pos = self.cursor;
        let char_idx = self.pos_to_char(pos);
        self.rope.insert(char_idx, text);
        // 计算插入后光标位置
        let newline_count = text.chars().filter(|&c| c == '\n').count();
        if newline_count == 0 {
            self.cursor = CursorPos::new(pos.line, pos.col + text.chars().count());
        } else {
            let last_newline_offset = text.rfind('\n').unwrap();
            let last_line_col = text[last_newline_offset + 1..].chars().count();
            self.cursor = CursorPos::new(pos.line + newline_count, last_line_col);
        }
        self.push_undo(EditAction::Insert {
            pos,
            text: text.to_string(),
        });
        self.modified = true;
    }

    /// 退格删除：选区存在时删除选区，否则删除光标前一个字符或合并行。
    pub fn delete_backward(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let pos = self.cursor;
        if pos.col > 0 {
            // 删除光标前一个字符
            let char_idx = self.pos_to_char(pos);
            let removed = self.rope.char(char_idx - 1);
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor = CursorPos::new(pos.line, pos.col - 1);
            self.push_undo(EditAction::Delete {
                pos: CursorPos::new(pos.line, pos.col - 1),
                text: removed.to_string(),
            });
            self.modified = true;
        } else if pos.line > 0 {
            // 合并到上一行
            let prev_line_len = self.line_content_len(pos.line - 1);
            let char_idx = self.pos_to_char(pos);
            // 删除换行符（前一行末尾的 \n）
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor = CursorPos::new(pos.line - 1, prev_line_len);
            self.push_undo(EditAction::Delete {
                pos: CursorPos::new(pos.line - 1, prev_line_len),
                text: "\n".to_string(),
            });
            self.modified = true;
        }
    }

    /// 正向删除：选区存在时删除选区，否则删除光标处字符或合并下一行。
    pub fn delete_forward(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let pos = self.cursor;
        let line_len = self.line_content_len(pos.line);
        if pos.col < line_len {
            // 删除光标处字符
            let char_idx = self.pos_to_char(pos);
            let removed = self.rope.char(char_idx);
            self.rope.remove(char_idx..char_idx + 1);
            self.push_undo(EditAction::Delete {
                pos,
                text: removed.to_string(),
            });
            self.modified = true;
        } else if pos.line + 1 < self.line_count() {
            // 合并下一行（删除行末换行符）
            let char_idx = self.pos_to_char(pos);
            self.rope.remove(char_idx..char_idx + 1);
            self.push_undo(EditAction::Delete {
                pos,
                text: "\n".to_string(),
            });
            self.modified = true;
        }
    }

    /// 删除选区文本并返回被删除的内容。无选区时返回 None。
    pub fn delete_selection(&mut self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let start_idx = self.pos_to_char(start);
        let end_idx = self.pos_to_char(end);
        let text = self.rope.slice(start_idx..end_idx).to_string();
        self.rope.remove(start_idx..end_idx);
        self.cursor = start;
        self.selection_anchor = None;
        self.push_undo(EditAction::Delete {
            pos: start,
            text: text.clone(),
        });
        self.modified = true;
        Some(text)
    }

    /// 内部辅助：如果存在选区则删除之。
    pub fn delete_selection_if_any(&mut self) {
        self.delete_selection();
    }

    /// 撤销上一步操作。
    pub fn undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            return;
        };
        self.apply_inverse(&action);
        self.redo_stack.push(action);
    }

    /// 重做上一步操作。
    pub fn redo(&mut self) {
        let Some(action) = self.redo_stack.pop() else {
            return;
        };
        self.apply_forward(&action);
        self.undo_stack.push(action);
    }

    /// 将编辑操作推入 undo 栈。合并同行相邻位置的单字符插入。
    pub fn push_undo(&mut self, action: EditAction) {
        // 尝试合并：连续单字符（非换行）插入且在同一行相邻位置
        if let EditAction::Insert {
            pos: new_pos,
            ref text,
        } = action
        {
            if text.chars().count() == 1 && !text.contains('\n') {
                if let Some(EditAction::Insert {
                    pos: prev_pos,
                    text: ref mut prev_text,
                }) = self.undo_stack.last_mut()
                {
                    if prev_pos.line == new_pos.line
                        && prev_pos.col + prev_text.chars().count() == new_pos.col
                    {
                        prev_text.push_str(&text);
                        // 合并成功，不压入新 action
                        self.redo_stack.clear();
                        return;
                    }
                }
            }
        }
        // 未合并，正常压栈
        self.undo_stack.push(action);
        self.redo_stack.clear();
        // 栈容量上限
        if self.undo_stack.len() > 10000 {
            self.undo_stack.remove(0);
        }
        // 编辑后使高亮失效
        self.invalidate_highlight();
    }

    /// 反向应用编辑操作（用于 undo）。
    fn apply_inverse(&mut self, action: &EditAction) {
        match action {
            EditAction::Insert { pos, text } => {
                let start_idx = self.pos_to_char(*pos);
                let end_idx = start_idx + text.chars().count();
                self.rope.remove(start_idx..end_idx);
                self.cursor = *pos;
            }
            EditAction::Delete { pos, text } => {
                let char_idx = self.pos_to_char(*pos);
                self.rope.insert(char_idx, text);
                self.cursor = *pos;
            }
        }
        self.clamp_cursor();
    }

    /// 正向应用编辑操作（用于 redo）。
    fn apply_forward(&mut self, action: &EditAction) {
        match action {
            EditAction::Insert { pos, text } => {
                let char_idx = self.pos_to_char(*pos);
                self.rope.insert(char_idx, text);
                // 光标移到插入文本末尾
                let newline_count = text.chars().filter(|&c| c == '\n').count();
                if newline_count == 0 {
                    self.cursor = CursorPos::new(pos.line, pos.col + text.chars().count());
                } else {
                    let last_nl = text.rfind('\n').unwrap();
                    let last_col = text[last_nl + 1..].chars().count();
                    self.cursor = CursorPos::new(pos.line + newline_count, last_col);
                }
            }
            EditAction::Delete { pos, text } => {
                let char_idx = self.pos_to_char(*pos);
                self.rope.remove(char_idx..char_idx + text.chars().count());
                self.cursor = *pos;
            }
        }
        self.clamp_cursor();
    }

    // ── 光标移动 ──

    /// 光标上移一行，列钳位到目标行内容长度。清除选区。
    pub fn move_up(&mut self) {
        self.clear_selection();
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.cursor.col.min(self.line_content_len(self.cursor.line));
        }
    }

    /// 光标下移一行，列钳位到目标行内容长度。清除选区。
    pub fn move_down(&mut self) {
        self.clear_selection();
        if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = self.cursor.col.min(self.line_content_len(self.cursor.line));
        }
    }

    /// 光标左移一字符，到行首时跳到上一行末尾。清除选区。
    pub fn move_left(&mut self) {
        self.clear_selection();
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_content_len(self.cursor.line);
        }
    }

    /// 光标右移一字符，到行末时跳到下一行行首。清除选区。
    pub fn move_right(&mut self) {
        self.clear_selection();
        let line_len = self.line_content_len(self.cursor.line);
        if self.cursor.col < line_len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    /// 光标移到当前行行首（col = 0）。清除选区。
    pub fn move_home(&mut self) {
        self.clear_selection();
        self.cursor.col = 0;
    }

    /// 光标移到当前行行末（col = line_content_len）。清除选区。
    pub fn move_end(&mut self) {
        self.clear_selection();
        self.cursor.col = self.line_content_len(self.cursor.line);
    }

    // ── 鼠标交互 ──

    /// 鼠标点击：钳位位置后设置光标，清除选区。
    pub fn click(&mut self, line: usize, col: usize) {
        let pos = self.clamp_pos(CursorPos::new(line, col));
        self.cursor = pos;
        self.clear_selection();
    }

    /// 鼠标拖拽：钳位位置，若无锚点则先设置锚点，然后移动光标。
    pub fn drag(&mut self, line: usize, col: usize) {
        let pos = self.clamp_pos(CursorPos::new(line, col));
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.cursor = pos;
    }

    /// 调整 scroll_y 使光标行在视口内可见。
    pub fn scroll_to_cursor(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        let line = self.cursor.line;
        if line < self.scroll_y {
            self.scroll_y = line;
        } else if line >= self.scroll_y + viewport_height {
            self.scroll_y = line - viewport_height + 1;
        }
    }

    /// 调整 scroll_x 使光标的显示列在视口内可见。
    /// `content_width` 是内容区域的宽度（已减去 gutter、分隔符、滚动条）。
    pub fn scroll_to_cursor_x(&mut self, content_width: usize) {
        if content_width == 0 {
            return;
        }
        let display_col = self.char_idx_to_display_col(self.cursor.line, self.cursor.col);
        // 内容可见范围：[scroll_x, scroll_x + content_width)
        let visible_end = self.scroll_x + content_width;
        if display_col < self.scroll_x {
            // 光标在可见区域左侧，向左滚动（留 2 列边距）
            self.scroll_x = display_col.saturating_sub(2);
        } else if display_col >= visible_end {
            // 光标在可见区域右侧，向右滚动（留 2 列边距）
            self.scroll_x = display_col - content_width + 3;
        }
    }

    // ── 显示列转换 ──

    /// 将字符索引转换为显示列（累加显示宽度）。
    pub fn char_idx_to_display_col(&self, line: usize, char_idx: usize) -> usize {
        let text = self.line_text(line);
        let mut col = 0;
        for (i, ch) in text.chars().enumerate() {
            if i >= char_idx {
                break;
            }
            col += char_width(ch);
        }
        col
    }

    /// 将显示列转换为字符索引（在给定文本中）。
    ///
    /// 从文本开头累加显示宽度，返回不超过 target_display_col 的最大字符索引。
    pub fn display_col_to_char_idx(text: &str, target_display_col: usize) -> usize {
        let mut display_col = 0;
        for (idx, ch) in text.chars().enumerate() {
            if display_col >= target_display_col {
                return idx;
            }
            display_col += char_width(ch);
        }
        text.chars().count()
    }

    /// 将屏幕坐标转换为光标位置（考虑软折行）。
    ///
    /// - `rel_row`：视觉行索引（0 = 视口顶部）
    /// - `rel_col`：内容区内的显示列（0 = 内容区左边界，不含 gutter/sep）
    /// - `content_width`：内容区显示宽度
    pub fn screen_to_cursor(
        &self,
        rel_row: usize,
        rel_col: usize,
        content_width: usize,
    ) -> CursorPos {
        if content_width == 0 || self.rope.len_lines() == 0 {
            return CursorPos::new(0, 0);
        }

        let max_line = self.rope.len_lines().saturating_sub(1);
        let mut visual_row = 0;

        for logical_line in self.scroll_y..=max_line {
            let line_width = self.line_display_width(logical_line);
            let wrap_count = if line_width == 0 {
                1
            } else {
                (line_width + content_width - 1) / content_width
            };

            for wrap_idx in 0..wrap_count {
                if visual_row == rel_row {
                    // 找到了对应的视觉行
                    let col_offset = wrap_idx * content_width;
                    let actual_display_col = col_offset + rel_col;
                    let text = self.line_text(logical_line);
                    let char_col = Self::display_col_to_char_idx(&text, actual_display_col);
                    return CursorPos::new(logical_line, char_col);
                }
                visual_row += 1;
            }
        }

        // 超出视口 → 文件末尾
        CursorPos::new(max_line, self.line_content_len(max_line))
    }

    // ── 滚动访问器 ──

    /// 垂直滚动偏移（行号）。
    pub fn scroll_y(&self) -> usize {
        self.scroll_y
    }

    /// 水平滚动偏移（列号）。
    pub fn scroll_x(&self) -> usize {
        self.scroll_x
    }

    /// 设置垂直滚动偏移，钳位到文件末尾。
    pub fn set_scroll_y(&mut self, y: usize) {
        let max_y = self.rope.len_lines().saturating_sub(1);
        self.scroll_y = y.min(max_y);
    }

    /// 设置水平滚动偏移。
    pub fn set_scroll_x(&mut self, x: usize) {
        self.scroll_x = x;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

    /// 测试打开文件并读取内容。
    #[test]
    fn test_open_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        {
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"hello\nworld\n").unwrap();
        }

        let editor = TextEditor::open(file_path.clone()).unwrap();
        assert_eq!(editor.line_count(), 3, "应有 3 行（含末尾空行）");
        assert_eq!(editor.line_text(0), "hello", "第一行内容");
        assert_eq!(editor.line_text(1), "world", "第二行内容");
        assert!(!editor.is_modified(), "刚打开不应标记为已修改");
    }

    /// 测试 line_text 处理无换行末尾行和越界行。
    #[test]
    fn test_line_text() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        {
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"foo\nbar").unwrap();
        }

        let editor = TextEditor::open(file_path).unwrap();
        assert_eq!(editor.line_text(0), "foo", "第一行应不含换行符");
        assert_eq!(editor.line_text(1), "bar", "最后一行无换行符");
        assert_eq!(editor.line_text(99), "", "越界行应返回空字符串");
    }

    // === 编辑操作测试 ===

    /// 辅助：创建内容为 text 的编辑器，光标在 (0,0)。
    fn make_editor(text: &str) -> TextEditor {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, text).unwrap();
        TextEditor::open(file_path).unwrap()
    }

    #[test]
    fn test_insert_char() {
        let mut ed = make_editor("ab");
        ed.cursor = CursorPos::new(0, 0);
        ed.insert_char('x');
        assert_eq!(ed.line_text(0), "xab", "插入后应为 xab");
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "光标应在 col 1");
    }

    #[test]
    fn test_insert_char_newline() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 1);
        ed.insert_char('\n');
        assert_eq!(ed.line_text(0), "a", "第一行应为 a");
        assert_eq!(ed.line_text(1), "bc", "第二行应为 bc");
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "光标应在第二行行首");
    }

    #[test]
    fn test_delete_backward() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 2);
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "ac", "删除后应为 ac");
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "光标应在 col 1");
    }

    #[test]
    fn test_delete_backward_merge_lines() {
        let mut ed = make_editor("ab\ncd");
        ed.cursor = CursorPos::new(1, 0);
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "abcd", "合并后应为 abcd");
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "光标应在 col 2");
    }

    #[test]
    fn test_delete_forward() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 0);
        ed.delete_forward();
        assert_eq!(ed.line_text(0), "bc", "删除后应为 bc");
        assert_eq!(ed.cursor, CursorPos::new(0, 0), "光标应在 col 0");
    }

    #[test]
    fn test_selection_and_delete() {
        let mut ed = make_editor("hello world");
        ed.cursor = CursorPos::new(0, 2);
        ed.selection_anchor = Some(CursorPos::new(0, 7));
        let deleted = ed.delete_selection();
        assert_eq!(deleted, Some("llo w".to_string()), "应删除 llo w");
        assert_eq!(ed.line_text(0), "heorld", "删除后应为 heorld");
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "光标应在选区起始位置");
        assert!(!ed.has_selection(), "选区应已清除");
    }

    #[test]
    fn test_undo_redo() {
        let mut ed = make_editor("ab");
        ed.cursor = CursorPos::new(0, 0);
        ed.insert_char('x');
        assert_eq!(ed.line_text(0), "xab");
        ed.insert_char('y');
        assert_eq!(ed.line_text(0), "xyab");
        // 两个连续单字符插入应合并
        ed.undo();
        assert_eq!(ed.line_text(0), "ab", "undo 合并操作后应恢复为 ab");
        assert_eq!(ed.cursor, CursorPos::new(0, 0), "光标应回到 col 0");
        ed.redo();
        assert_eq!(ed.line_text(0), "xyab", "redo 后应为 xyab");
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "光标应在 col 2");
    }

    #[test]
    fn test_insert_text_multiline() {
        let mut ed = make_editor("ab");
        ed.cursor = CursorPos::new(0, 1);
        ed.insert_text("x\ny");
        assert_eq!(ed.line_text(0), "ax", "第一行应为 ax");
        assert_eq!(ed.line_text(1), "yb", "第二行应为 yb");
        assert_eq!(ed.cursor, CursorPos::new(1, 1), "光标应在 (1,1)");
    }

    #[test]
    fn test_select_all() {
        let mut ed = make_editor("abc\ndef");
        ed.select_all();
        assert_eq!(ed.selected_text(), "abc\ndef", "全选应选中全部文本");
    }

    // === 光标移动与鼠标交互测试 ===

    #[test]
    fn test_cursor_movement() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(1, 1);
        ed.move_up();
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "上移到第 0 行 col 1");
        ed.move_end();
        assert_eq!(ed.cursor, CursorPos::new(0, 3), "行末 col 3");
        ed.move_right();
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "行末右移跳到下一行行首");
        ed.move_home();
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "行首 col 0");
        ed.move_left();
        assert_eq!(ed.cursor, CursorPos::new(0, 3), "行首左移跳到上一行行末");
        ed.move_down();
        assert_eq!(ed.cursor, CursorPos::new(1, 3), "下移 col 钳位到行长度");
        assert_eq!(ed.cursor.col, 3, "def 长度 3，col 钳位到 3");
    }

    #[test]
    fn test_click_and_drag() {
        let mut ed = make_editor("hello world");
        ed.click(0, 2);
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "点击设置光标");
        assert!(!ed.has_selection(), "点击后无选区");
        ed.drag(0, 7);
        assert_eq!(ed.cursor, CursorPos::new(0, 7), "拖拽移动光标");
        assert_eq!(
            ed.selection(),
            Some(CursorPos::new(0, 2)),
            "拖拽设置锚点为点击位置"
        );
        assert_eq!(ed.selected_text(), "llo w", "选区文本应为 llo w");
    }

    #[test]
    fn test_display_col_conversion() {
        // ASCII 字符宽度各 1
        assert_eq!(
            TextEditor::display_col_to_char_idx("abc", 2),
            2,
            "ASCII target 2 → char_idx 2"
        );
        assert_eq!(
            TextEditor::display_col_to_char_idx("abc", 5),
            3,
            "ASCII target 超出 → char_idx 3（文本长度）"
        );
        // CJK 字符宽度各 2
        assert_eq!(
            TextEditor::display_col_to_char_idx("你好", 2),
            1,
            "CJK target 2 → char_idx 1（display_col=0+2=2, 2>=2, return 1）"
        );
        assert_eq!(
            TextEditor::display_col_to_char_idx("你好", 3),
            2,
            "CJK target 3 → char_idx 2（两个 CJK 共 4 列，超出文本长度）"
        );
        assert_eq!(
            TextEditor::display_col_to_char_idx("你好", 4),
            2,
            "CJK target 4 → char_idx 2（两个 CJK 共 4 显示列，超出）"
        );
        // Tab 测试
        assert_eq!(
            TextEditor::display_col_to_char_idx("a\tb", 4),
            2,
            "Tab target 4 → char_idx 2（a=1, \\t=4, 1+4=5>=4, return 2 即 'b'）"
        );
    }

    /// char_idx_to_display_col 在 CJK 下正确累加宽度
    #[test]
    fn test_char_idx_to_display_col() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "你好abc").unwrap();
        let ed = TextEditor::open(path).unwrap();
        // char_idx=0 → display 0, char_idx=1 → display 2, char_idx=2 → display 4
        assert_eq!(ed.char_idx_to_display_col(0, 0), 0);
        assert_eq!(ed.char_idx_to_display_col(0, 1), 2);
        assert_eq!(ed.char_idx_to_display_col(0, 2), 4);
        assert_eq!(ed.char_idx_to_display_col(0, 3), 5);
    }
}
