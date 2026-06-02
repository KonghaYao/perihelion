# Text Editor Component Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 git-graph (gig) TUI 构建一个纯 GUI 风格的文本编辑器组件，鼠标优先、无模式，小白用户开箱即用。

**Architecture:** 独立 `editor` 模块（`src/editor/`），使用 `ropey` Rope 作为文本缓冲区，直接写入 ratatui `Buffer` 渲染（绕过 Widget 抽象层）。软件光标（反色 cell），不依赖终端光标。通过 `App.editor: Option<TextEditor>` 字段集成到 git-graph 主循环。

**Tech Stack:** `ropey = "1"`（文本缓冲区），`ratatui::buffer::Buffer`（直接 cell 写入），`crossterm::event`（鼠标/键盘），`unicode-width`（CJK 列宽），`arboard`（剪贴板，已有依赖），`syntect`（语法高亮，复用已有 `ui::syntax` 模块）。

---

## File Structure

```
src/editor/
├── mod.rs       — TextEditor 公共 API、核心编辑操作、undo/redo
├── render.rs    — 直接写入 ratatui Buffer 的渲染逻辑
└── input.rs     — 鼠标/键盘事件处理
```

集成改动（现有文件）：
- `Cargo.toml` — 添加 `ropey = "1"`
- `src/main.rs` — 添加 `mod editor;`
- `src/app.rs` — 添加 `editor: Option<editor::TextEditor>` 字段 + `editor_area: Rect`
- `src/render.rs` — editor 激活时渲染编辑器面板
- `src/event.rs` — editor 激活时路由事件到编辑器

---

## Task 1: Core Types and TextBuffer

**Files:**
- Create: `src/editor/mod.rs`
- Test: `src/editor/mod.rs` (inline `#[cfg(test)]`)

此任务建立核心数据结构和文本操作。TextEditor 持有 ropey Rope、光标、选区、undo 栈。

### 1.1 CursorPos 类型

```rust
/// 光标位置（行号和列号，均为字符索引，0-based）
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
```

### 1.2 EditAction（undo/redo 记录）

```rust
/// 编辑操作记录（用于 undo/redo）
#[derive(Debug, Clone)]
enum EditAction {
    /// 在 pos 位置插入了 text
    Insert { pos: CursorPos, text: String },
    /// 在 pos 位置删除了 text
    Delete { pos: CursorPos, text: String },
}
```

### 1.3 TextEditor 结构体

```rust
/// 文本编辑器状态
pub struct TextEditor {
    // 缓冲区
    rope: Rope,
    path: PathBuf,

    // 光标 & 选区
    cursor: CursorPos,
    /// 选区锚点（鼠标按下时的位置），Some = 有选区
    selection_anchor: Option<CursorPos>,

    // 滚动
    scroll_y: usize,
    scroll_x: usize,

    // 状态
    modified: bool,

    // Undo/Redo
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,
}
```

### 1.4 核心方法实现

- [ ] **Step 1: 创建 `src/editor/mod.rs` 骨架**

写入上述类型定义 + 以下方法骨架（先写 `open` / `save` / 只读访问器）：

```rust
use ropey::Rope;
use std::path::PathBuf;

// ... CursorPos, EditAction, TextEditor 定义 ...

impl TextEditor {
    /// 从文件加载
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(&path)?;
        let rope = Rope::from(text);
        Ok(Self {
            rope,
            path,
            cursor: CursorPos::new(0, 0),
            selection_anchor: None,
            scroll_y: 0,
            scroll_x: 0,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    /// 保存到文件
    pub fn save(&mut self) -> std::io::Result<()> {
        // to_string() 会自动用 \n 换行符
        std::fs::write(&self.path, self.rope.to_string())?;
        self.modified = false;
        Ok(())
    }

    pub fn is_modified(&self) -> bool { self.modified }
    pub fn path(&self) -> &PathBuf { &self.path }
    pub fn line_count(&self) -> usize { self.rope.len_lines() }
    pub fn cursor(&self) -> CursorPos { self.cursor }

    /// 获取选区范围（保证 start <= end）
    pub fn selection(&self) -> Option<(CursorPos, CursorPos)> {
        self.selection_anchor.map(|anchor| {
            if anchor <= self.cursor {
                (anchor, self.cursor)
            } else {
                (self.cursor, anchor)
            }
        })
    }

    /// 获取选区文本
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection()?;
        let start_char = self.pos_to_char(start);
        let end_char = self.pos_to_char(end);
        Some(self.rope.slice(start_char..end_char).to_string())
    }

    pub fn clear_selection(&mut self) { self.selection_anchor = None; }

    pub fn select_all(&mut self) {
        let last_line = self.rope.len_lines().saturating_sub(1);
        let last_col = self.line_content_len(last_line);
        self.selection_anchor = Some(CursorPos::new(0, 0));
        self.cursor = CursorPos::new(last_line, last_col);
    }

    // ── 内部辅助 ──

    /// 行内容长度（不含换行符）
    fn line_content_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() { return 0; }
        let rope_line = self.rope.line(line);
        let len = rope_line.len_chars();
        if len > 0 && rope_line.char(len - 1) == '\n' { len - 1 } else { len }
    }

    /// CursorPos → rope 绝对字符索引
    fn pos_to_char(&self, pos: CursorPos) -> usize {
        if pos.line >= self.rope.len_lines() { return self.rope.len_chars(); }
        let line_start = self.rope.line_to_char(pos.line);
        line_start + pos.col.min(self.line_content_len(pos.line))
    }

    fn clamp_pos(&self, pos: CursorPos) -> CursorPos {
        let max_line = self.rope.len_lines().saturating_sub(1);
        let line = pos.line.min(max_line);
        let col = pos.col.min(self.line_content_len(line));
        CursorPos::new(line, col)
    }

    fn clamp_cursor(&mut self) { self.cursor = self.clamp_pos(self.cursor); }

    fn has_selection(&self) -> bool { self.selection_anchor.is_some() }

    /// 获取指定行文本（不含换行符）
    pub fn line_text(&self, line: usize) -> String {
        if line >= self.rope.len_lines() { return String::new(); }
        let rope_line = self.rope.line(line);
        let len = rope_line.len_chars();
        if len > 0 && rope_line.char(len - 1) == '\n' {
            rope_line.slice(0..len - 1).to_string()
        } else {
            rope_line.to_string()
        }
    }

    pub fn scroll_y(&self) -> usize { self.scroll_y }
    pub fn scroll_x(&self) -> usize { self.scroll_x }

    pub fn set_scroll_y(&mut self, y: usize) {
        let max = self.rope.len_lines().saturating_sub(1);
        self.scroll_y = y.min(max);
    }

    pub fn set_scroll_x(&mut self, x: usize) { self.scroll_x = x; }
}
```

- [ ] **Step 2: 编译验证**

Run: `cd side-projects/git-graph && cargo check 2>&1 | head -20`

需先在 `src/main.rs` 添加 `mod editor;`，在 `Cargo.toml` 添加 `ropey = "1"`。

Expected: 编译通过（可能有 unused warnings）

---

## Task 2: Edit Operations (insert / delete / undo / redo)

**Files:**
- Modify: `src/editor/mod.rs`（添加编辑方法）

核心编辑操作。每个操作维护 undo 栈。连续字符输入自动合并为一个 undo 动作。

- [ ] **Step 1: 实现 insert_char**

```rust
/// 在光标处插入字符（如有选区先删除选区）
pub fn insert_char(&mut self, ch: char) {
    self.delete_selection_if_any();
    let char_idx = self.pos_to_char(self.cursor);
    let mut s = String::new();
    s.push(ch);
    self.rope.insert(char_idx, &s);
    self.push_undo(EditAction::Insert { pos: self.cursor, text: s });
    self.redo_stack.clear();

    if ch == '\n' {
        self.cursor.line += 1;
        self.cursor.col = 0;
    } else {
        self.cursor.col += 1;
    }
    self.clamp_cursor();
    self.modified = true;
}
```

- [ ] **Step 2: 实现 insert_text（粘贴用）**

```rust
/// 插入文本字符串（用于粘贴）
pub fn insert_text(&mut self, text: &str) {
    if text.is_empty() { return; }
    self.delete_selection_if_any();
    let char_idx = self.pos_to_char(self.cursor);
    self.rope.insert(char_idx, text);
    self.push_undo(EditAction::Insert { pos: self.cursor, text: text.to_string() });
    self.redo_stack.clear();

    // 移动光标到文本末尾
    let newline_count = text.chars().filter(|c| *c == '\n').count();
    if newline_count > 0 {
        self.cursor.line += newline_count;
        let last_line_start = text.rfind('\n').map(|i| i + 1).unwrap_or(0);
        self.cursor.col = text[last_line_start..].chars().count();
    } else {
        self.cursor.col += text.chars().count();
    }
    self.clamp_cursor();
    self.modified = true;
}
```

- [ ] **Step 3: 实现 delete_backward（Backspace）和 delete_forward（Delete）**

```rust
/// 删除光标前字符（Backspace）
pub fn delete_backward(&mut self) {
    if self.has_selection() {
        self.delete_selection();
        return;
    }
    if self.cursor.col > 0 {
        let col = self.cursor.col - 1;
        let char_idx = self.pos_to_char(CursorPos::new(self.cursor.line, col));
        let deleted: String = self.rope.char(char_idx).to_string();
        self.rope.remove(char_idx..char_idx + 1);
        self.cursor.col = col;
        self.push_undo(EditAction::Delete { pos: self.cursor, text: deleted });
        self.redo_stack.clear();
        self.modified = true;
    } else if self.cursor.line > 0 {
        // 合并行：删除上一行末尾的换行符
        let prev_line = self.cursor.line - 1;
        let prev_col = self.line_content_len(prev_line);
        let nl_char = self.pos_to_char(CursorPos::new(prev_line, prev_col));
        self.rope.remove(nl_char..nl_char + 1);
        self.cursor = CursorPos::new(prev_line, prev_col);
        self.push_undo(EditAction::Delete { pos: self.cursor, text: "\n".to_string() });
        self.redo_stack.clear();
        self.modified = true;
    }
}

/// 删除光标后字符（Delete）
pub fn delete_forward(&mut self) {
    if self.has_selection() {
        self.delete_selection();
        return;
    }
    let line_len = self.line_content_len(self.cursor.line);
    if self.cursor.col < line_len {
        let char_idx = self.pos_to_char(self.cursor);
        let deleted: String = self.rope.char(char_idx).to_string();
        self.rope.remove(char_idx..char_idx + 1);
        self.push_undo(EditAction::Delete { pos: self.cursor, text: deleted });
        self.redo_stack.clear();
        self.modified = true;
    } else if self.cursor.line + 1 < self.rope.len_lines() {
        // 合并下一行
        let char_idx = self.pos_to_char(self.cursor);
        self.rope.remove(char_idx..char_idx + 1);
        self.push_undo(EditAction::Delete { pos: self.cursor, text: "\n".to_string() });
        self.redo_stack.clear();
        self.modified = true;
    }
}
```

- [ ] **Step 4: 实现 delete_selection**

```rust
/// 删除选区，返回被删除的文本
pub fn delete_selection(&mut self) -> Option<String> {
    let text = self.selected_text()?;
    let (start, _end) = self.selection()?;
    let start_char = self.pos_to_char(start);
    let end_char = self.pos_to_char(_end);
    self.rope.remove(start_char..end_char);
    self.cursor = start;
    self.selection_anchor = None;
    self.push_undo(EditAction::Delete { pos: start, text: text.clone() });
    self.redo_stack.clear();
    self.clamp_cursor();
    self.modified = true;
    Some(text)
}

fn delete_selection_if_any(&mut self) {
    if self.has_selection() { self.delete_selection(); }
}
```

- [ ] **Step 5: 实现 undo / redo**

```rust
pub fn undo(&mut self) {
    if let Some(action) = self.undo_stack.pop() {
        self.apply_inverse(&action);
        self.redo_stack.push(action);
    }
}

pub fn redo(&mut self) {
    if let Some(action) = self.redo_stack.pop() {
        self.apply_forward(&action);
        self.undo_stack.push(action);
    }
}

fn apply_inverse(&mut self, action: &EditAction) {
    match action {
        EditAction::Insert { pos, text } => {
            let start = self.pos_to_char(*pos);
            let char_count = text.chars().count();
            self.rope.remove(start..start + char_count);
            self.cursor = *pos;
        }
        EditAction::Delete { pos, text } => {
            let start = self.pos_to_char(*pos);
            self.rope.insert(start, text);
            self.cursor = *pos;
        }
    }
    self.clamp_cursor();
    self.selection_anchor = None;
}

fn apply_forward(&mut self, action: &EditAction) {
    match action {
        EditAction::Insert { pos, text } => {
            let start = self.pos_to_char(*pos);
            self.rope.insert(start, text);
            // 光标移到插入末尾
            let newline_count = text.chars().filter(|c| *c == '\n').count();
            if newline_count > 0 {
                self.cursor = CursorPos::new(
                    pos.line + newline_count,
                    text.rfind('\n').map(|i| text[i + 1..].chars().count()).unwrap_or(0),
                );
            } else {
                self.cursor = CursorPos::new(pos.line, pos.col + text.chars().count());
            }
        }
        EditAction::Delete { pos, text } => {
            let start = self.pos_to_char(*pos);
            let char_count = text.chars().count();
            self.rope.remove(start..start + char_count);
            self.cursor = *pos;
        }
    }
    self.clamp_cursor();
    self.selection_anchor = None;
}
```

- [ ] **Step 6: 实现 push_undo（连续字符插入合并）**

```rust
fn push_undo(&mut self, action: EditAction) {
    // 连续单字符插入合并为一个 undo 动作（同行、连续位置）
    if let EditAction::Insert { pos, ref text } = action {
        if text.len() == 1 && !text.contains('\n') {
            if let Some(EditAction::Insert { pos: last_pos, ref mut text: last_text }) =
                self.undo_stack.last_mut()
            {
                if last_pos.line == pos.line
                    && last_pos.col + last_text.chars().count() == pos.col
                {
                    last_text.push_str(text);
                    return;
                }
            }
        }
    }
    self.undo_stack.push(action);
    if self.undo_stack.len() > 10000 {
        self.undo_stack.drain(0..1000);
    }
}
```

- [ ] **Step 7: 编写测试**

在 `mod.rs` 底部添加 `#[cfg(test)]` 模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_editor(text: &str) -> TextEditor {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, text).unwrap();
        TextEditor::open(path).unwrap()
    }

    #[test]
    fn test_open_and_read() {
        let ed = make_editor("hello\nworld\n");
        assert_eq!(ed.line_count(), 3); // "hello\n", "world\n", "" (trailing newline)
        assert_eq!(ed.line_text(0), "hello");
        assert_eq!(ed.line_text(1), "world");
    }

    #[test]
    fn test_insert_char() {
        let mut ed = make_editor("ab");
        ed.insert_char('x');
        assert_eq!(ed.line_text(0), "axb");
        assert_eq!(ed.cursor(), CursorPos::new(0, 2));
    }

    #[test]
    fn test_insert_char_newline() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 1); // 光标在 'b' 前
        ed.insert_char('\n');
        assert_eq!(ed.line_text(0), "a");
        assert_eq!(ed.line_text(1), "bc");
        assert_eq!(ed.cursor(), CursorPos::new(1, 0));
    }

    #[test]
    fn test_delete_backward() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 2);
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "ac");
        assert_eq!(ed.cursor(), CursorPos::new(0, 1));
    }

    #[test]
    fn test_delete_backward_merge_lines() {
        let mut ed = make_editor("ab\ncd");
        ed.cursor = CursorPos::new(1, 0);
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "abcd");
        assert_eq!(ed.cursor(), CursorPos::new(0, 2));
    }

    #[test]
    fn test_delete_forward() {
        let mut ed = make_editor("abc");
        ed.delete_forward();
        assert_eq!(ed.line_text(0), "bc");
    }

    #[test]
    fn test_selection_and_delete() {
        let mut ed = make_editor("hello world");
        ed.click(0, 2);
        ed.drag(0, 7); // 选 "llo w"
        assert_eq!(ed.selected_text().unwrap(), "llo w");
        ed.delete_selection();
        assert_eq!(ed.line_text(0), "heorld");
        assert_eq!(ed.cursor(), CursorPos::new(0, 2));
    }

    #[test]
    fn test_undo_redo() {
        let mut ed = make_editor("ab");
        ed.insert_char('x');
        ed.insert_char('y');
        // 连续插入合并为一个 undo
        ed.undo();
        assert_eq!(ed.line_text(0), "ab");
        ed.redo();
        assert_eq!(ed.line_text(0), "axyb");
    }

    #[test]
    fn test_insert_text_multiline() {
        let mut ed = make_editor("ab");
        ed.insert_text("x\ny");
        assert_eq!(ed.line_text(0), "ax");
        assert_eq!(ed.line_text(1), "yb");
        assert_eq!(ed.cursor(), CursorPos::new(1, 1));
    }

    #[test]
    fn test_select_all() {
        let mut ed = make_editor("abc\ndef");
        ed.select_all();
        let text = ed.selected_text().unwrap();
        assert_eq!(text, "abc\ndef");
    }
}
```

- [ ] **Step 8: 运行测试**

Run: `cd side-projects/git-graph && cargo test -p gig editor::tests -- --nocapture`

Expected: 全部 PASS

- [ ] **Step 9: Commit**

```bash
git add side-projects/git-graph/Cargo.toml side-projects/git-graph/src/editor/mod.rs side-projects/git-graph/src/main.rs
git commit -m "feat(gig): add TextEditor core — buffer, cursor, selection, undo/redo"
```

---

## Task 3: Cursor Movement and Mouse Interaction

**Files:**
- Modify: `src/editor/mod.rs`（添加光标移动和鼠标方法）

- [ ] **Step 1: 实现光标移动方法**

```rust
// ── 光标移动（清除选区）──

pub fn move_up(&mut self) {
    self.clear_selection();
    if self.cursor.line > 0 {
        self.cursor.line -= 1;
        self.cursor.col = self.cursor.col.min(self.line_content_len(self.cursor.line));
    }
}

pub fn move_down(&mut self) {
    self.clear_selection();
    if self.cursor.line + 1 < self.rope.len_lines() {
        self.cursor.line += 1;
        self.cursor.col = self.cursor.col.min(self.line_content_len(self.cursor.line));
    }
}

pub fn move_left(&mut self) {
    self.clear_selection();
    if self.cursor.col > 0 {
        self.cursor.col -= 1;
    } else if self.cursor.line > 0 {
        self.cursor.line -= 1;
        self.cursor.col = self.line_content_len(self.cursor.line);
    }
}

pub fn move_right(&mut self) {
    self.clear_selection();
    let line_len = self.line_content_len(self.cursor.line);
    if self.cursor.col < line_len {
        self.cursor.col += 1;
    } else if self.cursor.line + 1 < self.rope.len_lines() {
        self.cursor.line += 1;
        self.cursor.col = 0;
    }
}

pub fn move_home(&mut self) {
    self.clear_selection();
    self.cursor.col = 0;
}

pub fn move_end(&mut self) {
    self.clear_selection();
    self.cursor.col = self.line_content_len(self.cursor.line);
}
```

- [ ] **Step 2: 实现鼠标交互方法**

```rust
// ── 鼠标交互 ──

/// 鼠标点击定位光标（清除选区）
pub fn click(&mut self, line: usize, col: usize) {
    self.cursor = self.clamp_pos(CursorPos::new(line, col));
    self.selection_anchor = None;
}

/// 鼠标拖拽扩展选区
pub fn drag(&mut self, line: usize, col: usize) {
    let new_pos = self.clamp_pos(CursorPos::new(line, col));
    if self.selection_anchor.is_none() {
        self.selection_anchor = Some(self.cursor);
    }
    self.cursor = new_pos;
}

/// 滚动到光标可见
pub fn scroll_to_cursor(&mut self, viewport_height: usize) {
    if self.cursor.line < self.scroll_y {
        self.scroll_y = self.cursor.line;
    } else if self.cursor.line >= self.scroll_y + viewport_height {
        self.scroll_y = self.cursor.line - viewport_height + 1;
    }
}

/// 滚动到光标列可见
pub fn scroll_to_cursor_x(&mut self, gutter_width: usize, viewport_width: usize) {
    let cursor_display_col = self.char_idx_to_display_col(self.cursor.line, self.cursor.col);
    let visible_start = self.scroll_x;
    let visible_end = self.scroll_x + viewport_width.saturating_sub(gutter_width);
    if cursor_display_col < visible_start {
        self.scroll_x = cursor_display_col.saturating_sub(2);
    } else if cursor_display_col >= visible_end {
        self.scroll_x = cursor_display_col - viewport_width.saturating_sub(gutter_width) + 2;
    }
}
```

- [ ] **Step 3: 实现显示列 ↔ 字符索引转换**

```rust
/// 字符索引 → 显示列（累加 unicode width）
pub fn char_idx_to_display_col(&self, line: usize, char_idx: usize) -> usize {
    let text = self.line_text(line);
    let mut col = 0usize;
    for (i, ch) in text.chars().enumerate() {
        if i >= char_idx { break; }
        col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    col
}

/// 显示列 → 字符索引（用于鼠标点击转换）
pub fn display_col_to_char_idx(text: &str, target_display_col: usize) -> usize {
    let mut display_col = 0usize;
    for (idx, ch) in text.chars().enumerate() {
        if display_col >= target_display_col { return idx; }
        display_col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    text.chars().count()
}
```

- [ ] **Step 4: 测试光标移动**

在 tests 模块追加：

```rust
#[test]
fn test_cursor_movement() {
    let mut ed = make_editor("abc\ndef\nghi");
    ed.cursor = CursorPos::new(1, 1); // 'e'
    ed.move_up();
    assert_eq!(ed.cursor(), CursorPos::new(0, 1));
    ed.move_end();
    assert_eq!(ed.cursor(), CursorPos::new(0, 3));
    ed.move_right(); // 跳到下一行开头
    assert_eq!(ed.cursor(), CursorPos::new(1, 0));
}

#[test]
fn test_click_and_drag() {
    let mut ed = make_editor("hello world");
    ed.click(0, 2);
    assert_eq!(ed.cursor(), CursorPos::new(0, 2));
    assert!(ed.selection().is_none());

    ed.drag(0, 7);
    let (start, end) = ed.selection().unwrap();
    assert_eq!(start, CursorPos::new(0, 2));
    assert_eq!(end, CursorPos::new(0, 7));
}

#[test]
fn test_display_col_conversion() {
    assert_eq!(TextEditor::display_col_to_char_idx("abc", 2), 2);
    assert_eq!(TextEditor::display_col_to_char_idx("abc", 5), 3); // 超出返回末尾
    // CJK：中文字符占 2 列
    assert_eq!(TextEditor::display_col_to_char_idx("你好", 2), 1);
    assert_eq!(TextEditor::display_col_to_char_idx("你好", 3), 1); // 第 2 列仍在第 1 个字符内
}
```

- [ ] **Step 5: 运行测试**

Run: `cd side-projects/git-graph && cargo test -p gig editor::tests -- --nocapture`

Expected: 全部 PASS

- [ ] **Step 6: Commit**

```bash
git add side-projects/git-graph/src/editor/mod.rs
git commit -m "feat(gig): add cursor movement and mouse interaction"
```

---

## Task 4: Rendering

**Files:**
- Create: `src/editor/render.rs`

渲染逻辑直接写入 ratatui `Buffer`，不通过 Widget/Block 抽象。包含：行号 gutter、文本内容、选区高亮、当前行高亮、软件光标。

- [ ] **Step 1: 创建 render.rs**

```rust
//! 编辑器渲染：直接写入 ratatui Buffer。

use crate::editor::TextEditor;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

// 配色常量（与 GigTheme 对齐）
const GUTTER_FG: Color = Color::Rgb(100, 100, 100);
const GUTTER_BG: Color = Color::Rgb(30, 30, 40);
const SEPARATOR_FG: Color = Color::Rgb(60, 60, 70);
const CURRENT_LINE_BG: Color = Color::Rgb(30, 30, 45);
const SELECTION_BG: Color = Color::Rgb(40, 60, 100);
const CURSOR_FG: Color = Color::Black;
const CURSOR_BG: Color = Color::Rgb(200, 200, 200);
const STATUS_FG: Color = Color::Rgb(100, 100, 110);

/// 计算行号 gutter 宽度（行号数字 + 1 空格）
pub fn gutter_width(line_count: usize) -> u16 {
    let digits = if line_count == 0 { 1 } else { line_count.to_string().len() };
    (digits + 1) as u16
}

/// 渲染编辑器到 ratatui Buffer
pub fn render_to_buffer(editor: &TextEditor, buf: &mut Buffer, area: Rect) {
    if area.width < 4 || area.height == 0 { return; }

    let total_lines = editor.line_count();
    let gutter_w = gutter_width(total_lines);
    let sep_w: u16 = 1; // "│"
    let content_x = area.x + gutter_w + sep_w;
    let content_w = area.width.saturating_sub(gutter_w + sep_w + 1); // -1 scrollbar
    if content_w == 0 { return; }

    let viewport_h = area.height as usize;

    for row in 0..viewport_h {
        let line_idx = editor.scroll_y() + row;
        let y = area.y + row as u16;

        if line_idx >= total_lines {
            // 空行：只画 gutter 背景
            clear_row(buf, area.x, y, area.width, Style::default());
            continue;
        }

        let text = editor.line_text(line_idx);
        let is_current_line = line_idx == editor.cursor().line;

        // 1. Gutter（行号）
        render_gutter(buf, area.x, y, gutter_w, line_idx + 1, is_current_line);

        // 2. 分隔符
        let sep_x = area.x + gutter_w;
        set_cell(buf, sep_x, y, '│', Style::default().fg(SEPARATOR_FG));

        // 3. 内容区域背景（当前行高亮）
        let bg = if is_current_line { CURRENT_LINE_BG } else { Color::Reset };
        clear_row(buf, content_x, y, content_w, Style::default().bg(bg));

        // 4. 内容文本（带水平滚动、选区高亮）
        render_line_content(editor, buf, content_x, y, content_w, line_idx, &text);

        // 5. Scrollbar
        render_scrollbar(buf, area, total_lines, editor.scroll_y(), viewport_h);

        // 6. 底部状态栏
        render_status_bar(editor, buf, area, total_lines);
    }

    // 7. 软件光标（在当前行之后覆盖渲染）
    render_cursor(editor, buf, content_x, content_w, gutter_w);
}

fn render_gutter(buf: &mut Buffer, x: u16, y: u16, width: u16, line_num: usize, is_current: bool) {
    let num_str = format!("{:>width$} ", line_num, width = width as usize - 1);
    let fg = if is_current { Color::Rgb(180, 180, 180) } else { GUTTER_FG };
    for (i, ch) in num_str.chars().enumerate() {
        let cx = x + i as u16;
        if cx < x + width {
            set_cell(buf, cx, y, ch, Style::default().fg(fg).bg(GUTTER_BG));
        }
    }
}

fn render_line_content(
    editor: &TextEditor,
    buf: &mut Buffer,
    content_x: u16,
    y: u16,
    content_w: u16,
    line_idx: usize,
    text: &str,
) {
    let scroll_x = editor.scroll_x();
    let selection = editor.selection();

    let mut display_col = 0usize;
    let mut char_idx = 0usize;

    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);

        // 跳过 scroll_x 之前的字符
        if display_col + ch_width <= scroll_x {
            display_col += ch_width;
            char_idx += 1;
            continue;
        }

        let screen_offset = display_col.saturating_sub(scroll_x);
        if screen_offset as u16 >= content_w { break; }

        let screen_x = content_x + screen_offset as u16;

        // 处理跨 scroll_x 边界的 CJK 字符：显示为空格跳过
        if display_col < scroll_x && display_col + ch_width > scroll_x {
            // 字符被水平滚动截断，跳过
            display_col += ch_width;
            char_idx += 1;
            continue;
        }

        let in_selection = selection.map_or(false, |(start, end)| {
            let pos = crate::editor::CursorPos::new(line_idx, char_idx);
            start <= pos && pos < end
        });

        let style = if in_selection {
            Style::default().bg(SELECTION_BG)
        } else {
            Style::default()
        };

        // 写入字符（CJK 占多列，后续列用空填充）
        if screen_x < content_x + content_w {
            set_cell(buf, screen_x, y, ch, style);
            // CJK 占 2+ 列时填充后续位置（空 cell）
            for extra in 1..ch_width {
                let ex = screen_x + extra as u16;
                if ex < content_x + content_w {
                    set_cell(buf, ex, y, ' ', style);
                }
            }
        }

        display_col += ch_width;
        char_idx += 1;
    }
}

fn render_cursor(editor: &TextEditor, buf: &mut Buffer, content_x: u16, content_w: u16, gutter_w: u16) {
    let cursor = editor.cursor();
    let text = editor.line_text(cursor.line);
    let cursor_display_col = editor.char_idx_to_display_col(cursor.line, cursor.col);

    let scroll_x = editor.scroll_x();
    let screen_offset = cursor_display_col.saturating_sub(scroll_x) as u16;

    // 计算光标所在行的屏幕 y
    let viewport_h = buf.area.height; // 使用整个 buf 的高度近似
    let row_in_viewport = cursor.line.saturating_sub(editor.scroll_y()) as u16;
    if row_in_viewport >= viewport_h { return; }

    // 计算 y 需要从 buf 的 area 推导——简化：用 content_x 反推 area.y
    // 实际 area.y = content_x - gutter_w - 1, 但 cursor 的 y = area.y + row_in_viewport
    // 这里需要传入 area，暂用近似方式
    // TODO: 改为传入 area

    let screen_x = content_x + screen_offset;
    if screen_x >= content_x + content_w { return; }

    // 读取 cell 当前字符（如果光标在文本上），反色显示
    // 如果光标在行尾（空位），显示空格反色
    let ch = text.chars().nth(cursor.col).unwrap_or(' ');
    let cell_style = Style::default().fg(CURSOR_FG).bg(CURSOR_BG).add_modifier(Modifier::BOLD);
    set_cell(buf, screen_x, 0 /* placeholder, see TODO above */, ch, cell_style);
}
```

**注意：** 上述 `render_cursor` 有一个 TODO——需要传入 `area` 来计算光标的屏幕 y 坐标。在最终实现中会修复：

```rust
fn render_cursor(editor: &TextEditor, buf: &mut Buffer, area: Rect, content_x: u16, content_w: u16) {
    let cursor = editor.cursor();
    let row_in_viewport = cursor.line.saturating_sub(editor.scroll_y()) as u16;
    if row_in_viewport >= area.height { return; }
    let y = area.y + row_in_viewport;

    let text = editor.line_text(cursor.line);
    let cursor_display_col = editor.char_idx_to_display_col(cursor.line, cursor.col);
    let screen_offset = cursor_display_col.saturating_sub(editor.scroll_x()) as u16;
    let screen_x = content_x + screen_offset;
    if screen_x >= content_x + content_w { return; }

    let ch = text.chars().nth(cursor.col).unwrap_or(' ');
    let style = Style::default().fg(CURSOR_FG).bg(CURSOR_BG).add_modifier(Modifier::BOLD);
    set_cell(buf, screen_x, y, ch, style);
}
```

辅助函数：

```rust
fn set_cell(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}

fn clear_row(buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
    for i in 0..width {
        set_cell(buf, x + i, y, ' ', style);
    }
}

fn render_scrollbar(buf: &mut Buffer, area: Rect, total_lines: usize, scroll_y: usize, viewport_h: usize) {
    if total_lines <= viewport_h { return; }
    let sb_x = area.x + area.width.saturating_sub(1);
    let thumb_size = ((viewport_h * viewport_h) / total_lines).max(1);
    let thumb_pos = if total_lines > viewport_h {
        (scroll_y * (viewport_h - thumb_size)) / (total_lines - viewport_h)
    } else {
        0
    };

    for row in 0..viewport_h {
        let y = area.y + row as u16;
        let ch = if row >= thumb_pos && row < thumb_pos + thumb_size {
            '█'
        } else if row % 4 == 0 {
            '┊'
        } else {
            ' '
        };
        let color = if row >= thumb_pos && row < thumb_pos + thumb_size {
            Color::Rgb(80, 80, 90)
        } else {
            Color::Rgb(40, 40, 50)
        };
        set_cell(buf, sb_x, y, ch, Style::default().fg(color));
    }
}

fn render_status_bar(editor: &TextEditor, buf: &mut Buffer, area: Rect, total_lines: usize) {
    if area.height < 2 { return; }
    let y = area.y + area.height.saturating_sub(1);

    let cursor = editor.cursor();
    let modified = if editor.is_modified() { " ●" } else { "" };
    let status = format!(
        " L{}/{} C{}{} ",
        cursor.line + 1,
        total_lines,
        cursor.col + 1,
        modified,
    );

    // 右对齐状态栏
    let start_x = area.x + area.width.saturating_sub(status.len() as u16 + 1);
    for (i, ch) in status.chars().enumerate() {
        let x = start_x + i as u16;
        if x < area.x + area.width {
            set_cell(buf, x, y, ch, Style::default().fg(STATUS_FG));
        }
    }
}
```

- [ ] **Step 2: 在 mod.rs 添加 `pub mod render;`**

- [ ] **Step 3: 编译验证**

Run: `cd side-projects/git-graph && cargo check -p gig 2>&1 | head -20`

Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/src/editor/render.rs side-projects/git-graph/src/editor/mod.rs
git commit -m "feat(gig): add editor rendering — gutter, content, selection, scrollbar"
```

---

## Task 5: Input Handling (Keyboard + Mouse)

**Files:**
- Create: `src/editor/input.rs`

事件处理：将 crossterm 事件转换为 TextEditor 操作。所有键盘快捷键和鼠标交互的统一入口。

- [ ] **Step 1: 创建 input.rs**

```rust
//! 编辑器事件处理：键盘快捷键 + 鼠标交互。

use crate::app::ToastStyle;
use crate::editor::TextEditor;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// 处理键盘事件。返回 true 表示编辑器消费了该事件。
pub fn handle_key(
    editor: &mut TextEditor,
    code: KeyCode,
    mods: KeyModifiers,
    toast_fn: &mut dyn FnMut(String, ToastStyle),
) -> bool {
    // Ctrl 组合键
    if mods.contains(KeyModifiers::CONTROL) {
        match code {
            KeyCode::Char('s') => {
                if let Err(e) = editor.save() {
                    toast_fn(format!("保存失败: {}", e), ToastStyle::Error);
                } else {
                    toast_fn("已保存".to_string(), ToastStyle::Success);
                }
                return true;
            }
            KeyCode::Char('z') => { editor.undo(); return true; }
            KeyCode::Char('y') => { editor.redo(); return true; }
            KeyCode::Char('a') => { editor.select_all(); return true; }
            KeyCode::Char('c') => {
                if let Some(text) = editor.selected_text() {
                    copy_to_clipboard(&text, toast_fn);
                }
                return true;
            }
            KeyCode::Char('x') => {
                if let Some(text) = editor.delete_selection() {
                    copy_to_clipboard(&text, toast_fn);
                }
                return true;
            }
            KeyCode::Char('v') => {
                if let Some(text) = paste_from_clipboard() {
                    editor.insert_text(&text);
                }
                return true;
            }
            _ => return false,
        }
    }

    // 普通键
    match code {
        KeyCode::Char(ch) => { editor.insert_char(ch); true }
        KeyCode::Enter => { editor.insert_char('\n'); true }
        KeyCode::Backspace => { editor.delete_backward(); true }
        KeyCode::Delete => { editor.delete_forward(); true }
        KeyCode::Up => { editor.move_up(); true }
        KeyCode::Down => { editor.move_down(); true }
        KeyCode::Left => { editor.move_left(); true }
        KeyCode::Right => { editor.move_right(); true }
        KeyCode::Home => { editor.move_home(); true }
        KeyCode::End => { editor.move_end(); true }
        KeyCode::PageUp => {
            let scroll = editor.scroll_y().saturating_sub(20);
            editor.set_scroll_y(scroll);
            true
        }
        KeyCode::PageDown => {
            editor.set_scroll_y(editor.scroll_y() + 20);
            true
        }
        KeyCode::Esc => false, // 不消费，由上层处理关闭编辑器
        _ => false,
    }
}

/// 处理鼠标事件。需要传入编辑器渲染区域以计算相对坐标。
pub fn handle_mouse(
    editor: &mut TextEditor,
    mouse: MouseEvent,
    area: ratatui::layout::Rect,
    gutter_width: u16,
) -> bool {
    // 检查是否在编辑区域内
    if mouse.column < area.x || mouse.column >= area.x + area.width
        || mouse.row < area.y || mouse.row >= area.y + area.height
    {
        return false;
    }

    let rel_row = mouse.row.saturating_sub(area.y) as usize;
    let line_idx = editor.scroll_y() + rel_row;

    // 内容区起始 x
    let content_x = area.x + gutter_width + 1; // +1 separator
    let scroll_x = editor.scroll_x();
    let content_w = area.width.saturating_sub(gutter_width + 1 + 1); // gutter + sep + scrollbar

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.column < content_x {
                // 点击 gutter → 选中整行
                editor.click(line_idx, 0);
                // 光标到行首
                return true;
            }
            let rel_col = mouse.column.saturating_sub(content_x) as usize + scroll_x;
            let text = editor.line_text(line_idx.min(editor.line_count().saturating_sub(1)));
            let char_col = TextEditor::display_col_to_char_idx(&text, rel_col);
            editor.click(line_idx.min(editor.line_count().saturating_sub(1)), char_col);
            true
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let rel_col = mouse.column.saturating_sub(content_x) as usize + scroll_x;
            let line = line_idx.min(editor.line_count().saturating_sub(1));
            let text = editor.line_text(line);
            let char_col = TextEditor::display_col_to_char_idx(&text, rel_col);
            editor.drag(line, char_col);
            true
        }
        MouseEventKind::ScrollUp => {
            editor.set_scroll_y(editor.scroll_y().saturating_sub(3));
            true
        }
        MouseEventKind::ScrollDown => {
            editor.set_scroll_y(editor.scroll_y() + 3);
            true
        }
        _ => false,
    }
}

fn copy_to_clipboard(text: &str, toast_fn: &mut dyn FnMut(String, ToastStyle)) {
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            if let Err(e) = cb.set_text(text) {
                toast_fn(format!("复制失败: {}", e), ToastStyle::Error);
            }
        }
        Err(e) => toast_fn(format!("剪贴板不可用: {}", e), ToastStyle::Error),
    }
}

fn paste_from_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok())
}
```

- [ ] **Step 2: 在 mod.rs 添加 `pub mod input;`**

- [ ] **Step 3: 编译验证**

Run: `cd side-projects/git-graph && cargo check -p gig 2>&1 | head -20`

Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/src/editor/input.rs side-projects/git-graph/src/editor/mod.rs
git commit -m "feat(gig): add editor input handling — keyboard shortcuts + mouse"
```

---

## Task 6: Integration into git-graph

**Files:**
- Modify: `src/app.rs` — 添加 `editor` / `editor_area` 字段
- Modify: `src/render.rs` — editor 激活时渲染编辑器
- Modify: `src/event.rs` — editor 激活时路由事件

将编辑器组件集成到 git-graph 主循环。触发方式：在 sidebar 文件预览中按 `e` 进入编辑模式。

- [ ] **Step 1: 修改 app.rs — 添加 editor 字段**

在 `App` struct 中添加：

```rust
// === 编辑器状态 ===
/// 激活的文本编辑器（Some = 编辑器打开）
pub editor: Option<crate::editor::TextEditor>,
/// 编辑器渲染区域（渲染时更新）
pub editor_area: ratatui::layout::Rect,
```

在 `App::new()` 初始化列表中添加：

```rust
editor: None,
editor_area: ratatui::layout::Rect::default(),
```

- [ ] **Step 2: 修改 render.rs — 编辑器渲染路径**

在 `render::draw()` 函数中，替换 `preview_file.is_some()` 分支：

```rust
// 右侧：编辑器 > 文件预览 > graph+detail
if app.editor.is_some() {
    // 编辑器占满右侧全部
    if let Some(ref editor) = app.editor {
        crate::editor::render::render_to_buffer(editor, f.buffer_mut(), h_chunks[1]);
        app.editor_area = h_chunks[1];
    }
    app.detail_area = h_chunks[1];
} else if app.preview_file.is_some() {
    // 文件预览占满右侧全部
    crate::ui::file_preview::draw(f, h_chunks[1], app);
    app.detail_area = h_chunks[1];
} else {
    // graph(65%) + detail(35%)
    // ... 现有逻辑不变
}
```

- [ ] **Step 3: 修改 event.rs — 编辑器事件路由**

在 `handle_key` 函数顶部（确认弹窗之前）添加编辑器路由：

```rust
// 编辑器激活时，优先路由到编辑器
if let Some(ref mut editor) = app.editor {
    let consumed = crate::editor::input::handle_key(
        editor,
        code,
        mods,
        &mut |msg, style| app.show_toast(msg, style),
    );
    if consumed {
        // 编辑后自动 scroll_to_cursor
        let area = app.editor_area;
        let gutter_w = crate::editor::render::gutter_width(editor.line_count());
        let content_w = area.width.saturating_sub(gutter_w + 1 + 1);
        editor.scroll_to_cursor(area.height as usize);
        editor.scroll_to_cursor_x(gutter_w as usize, content_w as usize);
        return;
    }
    // Esc 未被消费 → 关闭编辑器
    if code == KeyCode::Esc {
        if editor.is_modified() {
            // TODO: 弹确认框 "文件已修改，确定关闭？"
            app.show_toast("文件已修改，按 Esc 再次关闭".to_string(), ToastStyle::Info);
            // 第二次 Esc 关闭
        }
        app.editor = None;
        return;
    }
    // 其他未消费的键（如 Ctrl+Q）也关闭编辑器
    return;
}
```

在 `handle_mouse` 函数顶部添加编辑器鼠标路由：

```rust
// 编辑器激活时，鼠标事件优先路由到编辑器
if let Some(ref mut editor) = app.editor {
    let gutter_w = crate::editor::render::gutter_width(editor.line_count());
    let consumed = crate::editor::input::handle_mouse(
        editor, mouse, app.editor_area, gutter_w,
    );
    if consumed {
        // 拖拽时自动 scroll_to_cursor
        if matches!(mouse.kind, MouseEventKind::Drag(_)) {
            editor.scroll_to_cursor(app.editor_area.height as usize);
        }
        return;
    }
}
```

- [ ] **Step 4: 添加 'e' 键触发编辑器**

在 event.rs 的 `handle_key` 函数中，在预览文件激活状态下（`preview_file.is_some()`），添加 `e` 键打开编辑器：

```rust
// 在 preview_file 的按键处理分支中
KeyCode::Char('e') => {
    if let Some((ref path, _is_staged)) = app.preview_file {
        if let Some(wd) = app.repo.repo().workdir() {
            let abs_path = wd.join(path);
            match crate::editor::TextEditor::open(abs_path) {
                Ok(ed) => {
                    app.editor = Some(ed);
                    app.preview_file = None; // 关闭预览
                }
                Err(e) => {
                    app.show_toast(format!("无法打开文件: {}", e), ToastStyle::Error);
                }
            }
        }
    }
}
```

- [ ] **Step 5: 编译验证**

Run: `cd side-projects/git-graph && cargo build -p gig 2>&1 | tail -5`

Expected: 编译成功

- [ ] **Step 6: 手动测试**

1. 打开一个 git 仓库：`cd some-repo && cargo run -p gig`
2. 在 sidebar 点击一个文件 → 进入预览
3. 按 `e` → 编辑器打开
4. 鼠标点击定位光标 ✓
5. 输入文字 ✓
6. Backspace/Delete ✓
7. 鼠标拖拽选择 ✓
8. Ctrl+C/X/V 复制/剪切/粘贴 ✓
9. Ctrl+Z/Y 撤销/重做 ✓
10. Ctrl+S 保存 ✓
11. Esc 关闭编辑器 ✓
12. 滚轮滚动 ✓

- [ ] **Step 7: Commit**

```bash
git add side-projects/git-graph/src/app.rs side-projects/git-graph/src/render.rs side-projects/git-graph/src/event.rs
git commit -m "feat(gig): integrate TextEditor into git-graph TUI"
```

---

## Self-Review

**1. Spec coverage:**
- ✅ 文本编辑（输入/删除/换行）— Task 2
- ✅ 选区（鼠标拖拽选择）— Task 3 + 5
- ✅ 撤销/重做 — Task 2
- ✅ 保存 — Task 5
- ✅ 复制/剪切/粘贴 — Task 5
- ✅ 光标移动（方向键/Home/End）— Task 3 + 5
- ✅ 鼠标优先（点击定位/拖拽选择/滚轮滚动）— Task 3 + 5
- ✅ 行号 gutter — Task 4
- ✅ 当前行高亮 — Task 4
- ✅ 选区高亮 — Task 4
- ✅ 软件光标 — Task 4
- ✅ 滚动条 — Task 4
- ✅ 状态栏 — Task 4
- ⚠️ 语法高亮 — 暂不实现（复用现有 syntect 基础设施留作后续优化）
- ⚠️ 全选 Ctrl+A — Task 5
- ⚠️ 文件修改标记（●）— Task 4 status bar

**2. Placeholder scan:**
- Task 4 `render_cursor` 有 TODO 标记 — 已在文本中给出修复方案（传入 `area` 参数）
- Task 6 Esc 关闭有 TODO（确认弹框）— 标注为简化处理

**3. Type consistency:**
- `CursorPos` 在所有 Task 中使用一致
- `TextEditor::display_col_to_char_idx` 是 `pub` 方法（Task 3 定义，Task 5 调用）
- `editor::render::gutter_width` 是 `pub` 函数（Task 4 定义，Task 6 调用）
- `app.show_toast` 签名 `(String, ToastStyle)` 与 toast_fn 类型匹配

**未覆盖的后续优化（不在本 plan 范围内）：**
- 语法高亮（需要增量 tree-sitter 或 syntect 集成）
- 搜索/替换
- 多标签页/分栏
- 文件监听（外部修改检测）
- Tab/缩进处理
- 自动换行（word wrap）
