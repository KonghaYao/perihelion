# 鼠标文本选择 执行计划

**目标:** 实现鼠标拖拽选中文本、反色高亮反馈、Ctrl+C 复制到系统剪贴板，同时保留鼠标滚轮滚动功能。

**技术栈:** Rust, ratatui (Paragraph/Wrap/Scroll), crossterm (MouseEventKind), unicode-width (CJK 宽字符), arboard (剪贴板)

**设计文档:** spec/feature_20260429_F002_mouse-text-selection/spec-design.md

## 改动总览

- 涉及 7 个文件：新建 `text_selection.rs`（选区状态模型），修改 `mod.rs`（模块注册）、`core.rs`（AppCore 新字段）、`render_thread.rs`（WrappedLineInfo 映射表）、`event.rs`（鼠标事件+Ctrl+C）、`main_ui.rs`（高亮渲染+区域记录）、`status_bar.rs`（复制提示）
- 依赖链：Task 1（数据模型）→ Task 2（wrap_map）→ Task 3（事件+坐标映射）→ Task 4（渲染+复制），严格顺序依赖
- 关键决策：选区坐标采用视觉行坐标体系（含 scroll_offset），wrap_map 在渲染线程中与 lines 同步计算保证一致性；**字符级粒度**提取与高亮——利用 `visual_to_logical` 将视觉坐标映射为 `(line_idx, char_offset)`，在首行/末行做列级截取，高亮渲染时拆分 span 精确到字符范围

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [x] 验证测试工具可用
  - `cargo test -p rust-agent-tui --lib -- --test-threads=1 2>&1 | tail -5`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 构建成功，无错误
- [x] 测试命令可用
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -5`
  - 预期: 测试框架可用，现有测试通过

---

### Task 1: TextSelection 数据模型

**背景:**
实现鼠标文本选择功能的第一步：建立选区状态数据模型。当前 `AppCore` 不记录任何鼠标选区状态和消息区域坐标，后续 Task（鼠标事件处理、坐标映射、渲染高亮）均依赖本 Task 建立的 `TextSelection` 结构和 `AppCore` 新字段。

**涉及文件:**
- 新建: `rust-agent-tui/src/app/text_selection.rs`
- 修改: `rust-agent-tui/src/app/core.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**

- [x] 新建 `TextSelection` 模块文件
  - 位置: `rust-agent-tui/src/app/text_selection.rs`（新建）
  - 定义 `TextSelection` 结构体，包含以下字段：
    ```rust
    use ratatui::layout::Rect;

    /// 文本选区状态
    #[derive(Debug, Clone)]
    pub struct TextSelection {
        /// 选区起始视觉坐标（相对于消息区域左上角）
        pub start: Option<(u16, u16)>,  // (visual_row, visual_col)
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
    ```

- [x] 在 `AppCore` 中注册模块并添加字段
  - 位置: `rust-agent-tui/src/app/mod.rs` 第 1 行模块声明区域（~L1，在 `pub mod agent;` 之前）
  - 添加: `pub mod text_selection;`

- [x] 在 `AppCore` struct 中添加 `text_selection` 和 `messages_area` 字段
  - 位置: `rust-agent-tui/src/app/core.rs` `AppCore` struct 定义中，在 `pub draft_input: Option<String>` 之后（~L49）
  - 添加两个字段：
    ```rust
    pub text_selection: crate::app::text_selection::TextSelection,
    /// 消息渲染区域的 Rect，每次 render() 时更新，用于鼠标事件坐标判定
    pub messages_area: Option<ratatui::layout::Rect>,
    ```

- [x] 在 `AppCore::new()` 构造函数中初始化新字段
  - 位置: `rust-agent-tui/src/app/core.rs` `AppCore::new()` 方法的 Self 初始化块中，在 `draft_input: None,` 之后（~L90）
  - 添加：
    ```rust
    text_selection: crate::app::text_selection::TextSelection::new(),
    messages_area: None,
    ```

- [x] 为 `TextSelection` 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/text_selection.rs`（`#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_start_drag_sets_coords`: 调用 `start_drag(5, 10)` → `start == Some((5, 10))`, `end == Some((5, 10))`, `dragging == true`, `selected_text == None`
    - `test_update_drag_moves_end`: 先 `start_drag(0, 0)` 再 `update_drag(3, 8)` → `start == Some((0, 0))`, `end == Some((3, 8))`
    - `test_end_drag_stops_dragging`: `start_drag` 后 `end_drag()` → `dragging == false`, `start/end` 保持不变
    - `test_clear_resets_all`: 设置选区后 `clear()` → 所有字段为 None/false
    - `test_is_active`: 无选区时 `is_active() == false`；`start_drag` 后 `is_active() == true`；`end_drag` 后（无 selected_text）`is_active() == false`；`set_selected_text(Some("x".into()))` 后 `is_active() == true`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::text_selection::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证模块编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证新字段在构造时正确初始化
  - `cargo test -p rust-agent-tui --lib -- app::core::tests::test_appcore_pipeline_initialized`
  - 预期: 已有测试仍通过（新增字段有默认初始化，不破坏现有构造逻辑）
- [x] 验证 TextSelection 单元测试通过
  - `cargo test -p rust-agent-tui --lib -- app::text_selection::tests`
  - 预期: 5 个测试全部通过

---

### Task 2: WrapMap 计算与存储

**背景:**
鼠标选区需要将屏幕像素坐标映射到文本字符位置。由于 ratatui 的 `Paragraph` widget 使用 `Wrap` 换行，一个逻辑行可能跨越多个视觉行。本 Task 在 `RenderCache` 中建立 `wrap_map` 映射表，记录每个逻辑行对应的视觉行范围和纯文本内容，供 Task 3（坐标映射）和 Task 4（高亮渲染）使用。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/render_thread.rs`

**执行步骤:**

- [x] 在 `render_thread.rs` 顶部新增 `WrappedLineInfo` struct
  - 位置: `render_thread.rs` 在 `RenderCache` struct 定义之前（~L12，`pub struct RenderCache` 之前）
  - 代码：
    ```rust
    /// 单个逻辑行的换行映射信息
    #[derive(Debug, Clone)]
    pub struct WrappedLineInfo {
        /// 该行在 cache.lines 中的索引
        pub line_idx: usize,
        /// 该逻辑行渲染后的起始视觉行号（基于 0）
        pub visual_row_start: u16,
        /// 该逻辑行渲染后的结束视觉行号（不含）
        pub visual_row_end: u16,
        /// 该逻辑行的纯文本内容（去样式，用于复制）
        pub plain_text: String,
        /// 每个字符的显示宽度序列（ASCII=1, CJK=2）
        pub char_widths: Vec<u8>,
    }
    ```

- [x] 在 `RenderCache` struct 中新增 `wrap_map` 字段
  - 位置: `render_thread.rs` `RenderCache` struct 中，在 `pub version: u64,` 之后（~L21）
  - 添加字段：`pub wrap_map: Vec<WrappedLineInfo>,`
  - 在 `RenderCache::new()` 中初始化：`wrap_map: Vec::new(),`

- [x] 在 `RenderTask` 中实现 `build_wrap_map` 方法
  - 位置: `render_thread.rs` `impl RenderTask` 块内，在 `rebuild_all` 方法之前（~L78）
  - 方法签名和关键逻辑：
    ```rust
    /// 根据 cache.lines 和当前宽度计算 wrap_map
    fn build_wrap_map(lines: &[Line<'static>], width: u16) -> Vec<WrappedLineInfo> {
        let usable_width = width.saturating_sub(1) as usize; // 右侧留 1 列给滚动条
        if usable_width == 0 || lines.is_empty() {
            return Vec::new();
        }
        let mut wrap_map = Vec::with_capacity(lines.len());
        let mut visual_row: u16 = 0;

        for (idx, line) in lines.iter().enumerate() {
            // 1. 提取纯文本
            let plain_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            // 2. 计算每个字符的显示宽度
            let char_widths: Vec<u8> = plain_text.chars()
                .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u8)
                .collect();
            // 3. 模拟 word-wrap 计算该行占几个视觉行
            let line_display_width: usize = char_widths.iter().map(|&w| w as usize).sum();
            let visual_count = if line_display_width == 0 {
                1 // 空行占 1 个视觉行
            } else {
                (line_display_width + usable_width - 1) / usable_width // 向上取整
            };
            let visual_count = visual_count.max(1) as u16;

            wrap_map.push(WrappedLineInfo {
                line_idx: idx,
                visual_row_start: visual_row,
                visual_row_end: visual_row + visual_count,
                plain_text,
                char_widths,
            });
            visual_row += visual_count;
        }
        wrap_map
    }
    ```
  - 原因: 使用 `unicode_width` crate（项目已依赖 `unicode-width = "0.2"`）计算字符宽度，CJK 字符宽度为 2。wrap 模拟用总显示宽度除以可用宽度向上取整。

- [x] 在 `rebuild_all` 方法中调用 `build_wrap_map`
  - 位置: `render_thread.rs` `RenderTask::rebuild_all()` 方法中，在 `cache.version += 1;` 之前（~L112）
  - 在写入 cache 的代码块末尾添加：
    ```rust
    cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
    ```

- [x] 在所有增量更新 cache.lines 的地方同步更新 wrap_map
  - 位置: 以下每个 `RenderEvent` 分支中，在 `cache.version += 1;` 之前添加 `cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);`
  - 涉及分支（共 6 处，均已在 cache.write() 锁内）：
    1. `RenderEvent::AddMessage`（~L131 之前）
    2. `RenderEvent::AppendChunk`（~L173 之前）
    3. `RenderEvent::StreamingDone`（~L193 之前）
    4. `RenderEvent::Clear`（~L207 之前，改写为 `cache.wrap_map = Vec::new();`）
    5. `RenderEvent::LoadHistory`（调用 `rebuild_all()` 后自动包含，无需额外处理）
    6. `RenderEvent::UpdateLastMessage`（~L239 之前）
    7. `RenderEvent::RemoveLastMessage`（~L257 之前）
  - 原因: wrap_map 必须与 lines 始终保持同步，否则坐标映射会产生错误结果

- [x] 为 `build_wrap_map` 编写单元测试
  - 测试文件: `rust-agent-tui/src/ui/render_thread.rs`（`#[cfg(test)] mod tests` 块内）
  - 测试场景:
    - `test_build_wrap_map_empty`: 传入空 `lines` 和 `width=80` → 返回空 Vec
    - `test_build_wrap_map_single_short_line`: 一行短文本 "Hello"（5 字符，宽度 80）→ wrap_map 长度为 1，`visual_row_start=0, visual_row_end=1, plain_text="Hello"`
    - `test_build_wrap_map_single_long_line_wraps`: 一行 200 字符 ASCII 文本（宽度 40）→ `visual_count = 200/40 = 5`，`visual_row_start=0, visual_row_end=5`
    - `test_build_wrap_map_cjk_char_width`: 一行含中文字符 "你好世界"（每个占 2 宽度）→ `char_widths` 为 `[2,2,2,2]`，`line_display_width=8`
    - `test_build_wrap_map_multi_line_visual_rows`: 两行文本，第一行 wrap 占 2 个视觉行，第二行占 1 个 → `wrap_map[0].visual_row_start=0, visual_row_end=2; wrap_map[1].visual_row_start=2, visual_row_end=3`
    - `test_build_wrap_map_empty_line`: 空行 "" → `visual_row_start` 和 `visual_row_end` 差值为 1
  - 测试辅助: 在测试中构造 `vec![Line::from("Hello")]` 等 `Vec<Line<'static>>` 作为输入
  - 运行命令: `cargo test -p rust-agent-tui --lib -- ui::render_thread::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证已有渲染测试仍通过
  - `cargo test -p rust-agent-tui --lib -- ui::render_thread::tests`
  - 预期: 既有 `test_add_message_increments_version` 和 `test_append_chunk_updates_last_message` 通过，新增 wrap_map 测试通过
- [x] 验证 wrap_map 在 rebuild 后有正确条目数
  - `cargo test -p rust-agent-tui --lib -- ui::render_thread::tests::test_build_wrap_map`
  - 预期: 所有 build_wrap_map 测试通过

---

### Task 3: 鼠标事件处理与坐标映射

**背景:**
用户在消息区域拖拽鼠标时，需要捕获 Down/Drag/Up 事件并记录视觉坐标，松开鼠标后通过 wrap_map 将视觉坐标映射为逻辑行索引和字符偏移，最终提取选区纯文本。当前 event.rs 的 `Event::Mouse` 分支（L449-453）仅处理 ScrollUp/ScrollDown，其他鼠标事件被丢弃。本 Task 扩展鼠标事件处理并实现坐标映射与文本提取逻辑，供 Task 4 使用选区数据。

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/app/text_selection.rs`

**执行步骤:**

- [x] 在 event.rs 顶部 import 中添加 `MouseButton`
  - 位置: `rust-agent-tui/src/event.rs` 第 3 行 import 语句
  - 修改为: `use ratatui::crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};`
  - 原因: Down/Drag/Up 事件需要 `MouseButton::Left` 参数

- [x] 扩展 `Event::Mouse` 分支，添加鼠标左键 Down/Drag/Up 处理
  - 位置: `rust-agent-tui/src/event.rs` L449-453 `Event::Mouse(mouse) => match mouse.kind {` 块
  - 将现有 `_ => {}` 分支替换为完整的鼠标事件处理：
    ```rust
    Event::Mouse(mouse) => match mouse.kind {
        MouseEventKind::ScrollUp => app.scroll_up(),
        MouseEventKind::ScrollDown => app.scroll_down(),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(area) = app.core.messages_area {
                if mouse.row >= area.y
                    && mouse.row < area.y + area.height
                    && mouse.column >= area.x
                    && mouse.column < area.x + area.width
                {
                    let visual_row = mouse.row - area.y + app.core.scroll_offset;
                    let visual_col = mouse.column - area.x;
                    app.core.text_selection.start_drag(visual_row, visual_col);
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.core.text_selection.dragging {
                if let Some(area) = app.core.messages_area {
                    let visual_row = mouse.row.saturating_sub(area.y)
                        .saturating_add(app.core.scroll_offset);
                    let visual_col = mouse.column.saturating_sub(area.x);
                    app.core.text_selection.update_drag(visual_row, visual_col);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.core.text_selection.dragging {
                app.core.text_selection.end_drag();
                let ts = &app.core.text_selection;
                if let (Some(start), Some(end)) = (ts.start, ts.end) {
                    let cache = app.core.render_cache.read();
                    let text = crate::app::text_selection::extract_selected_text(
                        start, end, &cache.wrap_map,
                    );
                    drop(cache);
                    app.core.text_selection.set_selected_text(text);
                }
            }
        }
        _ => {}
    },
    ```

- [x] 在 `Event::Resize` 分支中清除选区
  - 位置: `rust-agent-tui/src/event.rs` L41-43 `Event::Resize(w, _) => {` 块
  - 在 `let _ = app.core.render_tx.send(RenderEvent::Resize(w));` 之后添加：
    ```rust
    app.core.text_selection.clear();
    ```
  - 原因: resize 后 wrap_map 重新计算，旧选区坐标失效

- [x] 在 `text_selection.rs` 中添加 `visual_to_logical` 坐标映射函数
  - 位置: `rust-agent-tui/src/app/text_selection.rs` `impl TextSelection` 块之后，模块级别
  - 代码：
    ```rust
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
    ```

- [x] 在 `text_selection.rs` 中添加 `char_col_to_offset` 辅助函数
  - 位置: `rust-agent-tui/src/app/text_selection.rs` 模块级别（`visual_to_logical` 之前）
  - 代码：
    ```rust
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
    ```

- [ ] 在 `text_selection.rs` 中修改 `extract_selected_text` 函数，实现字符级提取
  - 位置: `rust-agent-tui/src/app/text_selection.rs` 模块级别（`visual_to_logical` 之后）
  - **变更说明:** 原实现忽略列坐标（`_start_col`、`_end_col`），整行整行提取。现改为利用 `char_col_to_offset` 计算首行起始字符偏移和末行结束字符偏移，中间行保持整行提取。
  - 新增 unicode 安全辅助函数（放在 `text_selection.rs` 模块级别）：
    ```rust
    /// 将字符索引转换为字节索引，用于安全切割 String。
    /// `char_idx` 是 plain_text 中的字符位置（从 0 开始）。
    /// 返回对应的 byte 偏移量。如果 char_idx 超出字符数，返回 text.len()。
    fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
        text.char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(text.len())
    }
    ```
  - `extract_selected_text` 代码：
    ```rust
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
    ```
  - **同时需要更新 `event.rs` 调用点:** `extract_selected_text` 新增 `usable_width` 参数，调用处需传入消息区域宽度。在 `MouseEventKind::Up(MouseButton::Left)` 分支中，将 `extract_selected_text(start, end, &cache.wrap_map)` 改为 `extract_selected_text(start, end, &cache.wrap_map, usable_width)`，其中 `usable_width` 从 `app.core.messages_area` 获取：`app.core.messages_area.map(|a| a.width.saturating_sub(1)).unwrap_or(0)`

- [ ] 更新坐标映射和文本提取的单元测试（适配字符级提取）
  - 测试文件: `rust-agent-tui/src/app/text_selection.rs`（`#[cfg(test)] mod tests` 块内）
  - **变更说明:** 所有 `extract_selected_text` 调用需新增 `usable_width` 参数（如 `80`）；单行/跨行断言改为精确子串而非整行。
  - 测试场景（需更新）:
    - `test_visual_to_logical_basic`: 无需改动
    - `test_visual_to_logical_out_of_range`: 无需改动
    - `test_extract_selected_text_single_line`: wrap_map 1 条目 "Hello World"（char_widths=[1,1,1,1,1,1,1,1,1,1,1]），start=(0,2) end=(0,8) → 返回 `"llo Wor"`（第 2~8 字符）
    - `test_extract_selected_text_multi_line`: wrap_map 3 条目，start=(0,0) end=(2,5) → 首行从 char 0 到行尾 + 中间行整行 + 末行到 char 5，即 `"Line0\nLine1\nLine2"`
    - `test_extract_selected_text_swapped`: start=(2,5) end=(0,0) → 与正向 swap 结果相同
    - `test_extract_selected_text_partial_first_and_last`: wrap_map 2 条目 "Hello" + "World"，start=(0,2) end=(1,3) → `"llo\nWor"`
    - `test_char_col_to_offset_ascii`: 无需改动
    - `test_char_col_to_offset_cjk`: 无需改动
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::text_selection::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [ ] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [ ] 验证鼠标滚轮不受影响
  - `grep -n "ScrollUp\|ScrollDown" rust-agent-tui/src/event.rs`
  - 预期: ScrollUp/ScrollDown 分支仍在 Event::Mouse match 中
- [ ] 验证 `extract_selected_text` 调用点已传入 `usable_width`
  - `grep -n "extract_selected_text" rust-agent-tui/src/event.rs`
  - 预期: 调用包含 4 个参数（start, end, wrap_map, usable_width）
- [ ] 验证坐标映射测试通过
  - `cargo test -p rust-agent-tui --lib -- app::text_selection::tests`
  - 预期: Task 1 的 5 个 + Task 3 更新后的字符级提取测试全部通过

---

### Task 4: 选区渲染与 Ctrl+C 复制

**背景:**
前三个 Task 已建立选区数据模型（Task 1）、wrap_map 映射表（Task 2）和鼠标事件处理+坐标映射（Task 3）。本 Task 实现两个用户可感知的功能：1）拖拽选中文本时在消息区域显示反色高亮，2）Ctrl+C 将选中文字复制到系统剪贴板并在状态栏显示复制成功提示。当前 `render_messages` 函数不处理选区高亮，Ctrl+C 仅支持中断/退出。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/app/core.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`

**执行步骤:**

- [x] 在 `AppCore` 中新增 `copy_message_until` 和 `copy_char_count` 字段
  - 位置: `rust-agent-tui/src/app/core.rs` `AppCore` struct 中，在 `pub draft_input: Option<String>` 之后
  - 添加：
    ```rust
    /// 复制成功提示截止时间，None 表示不显示
    pub copy_message_until: Option<std::time::Instant>,
    /// 复制的字符数（用于提示文案）
    pub copy_char_count: usize,
    ```
  - 在 `AppCore::new()` 的 Self 初始化块中添加：
    ```rust
    copy_message_until: None,
    copy_char_count: 0,
    ```

- [x] 在 `render_messages` 中保存消息区域 Rect 到 `app.core.messages_area`
  - 位置: `rust-agent-tui/src/ui/main_ui.rs` `render_messages()` 函数中，在 `let inner = messages_area;` 之后（L165 之后）
  - 添加：
    ```rust
    app.core.messages_area = Some(inner);
    ```

- [ ] 在 `render_messages` 中实现字符级选区高亮渲染
  - 位置: `rust-agent-tui/src/ui/main_ui.rs` `render_messages()` 函数中，替换现有的整行高亮逻辑（~L268-296）
  - **变更说明:** 原实现对整行所有 span 应用 `REVERSED`。现改为：利用 `visual_to_logical` 将起止视觉坐标映射为 `(line_idx, char_offset)`，然后对每行的 span 做字符级拆分，仅对选区范围内的字符应用 `REVERSED`。
  - 核心思路:
    1. 用 `visual_to_logical(start)` 和 `visual_to_logical(end)` 获取首尾逻辑位置
    2. 首行：从 `start_char_offset` 到行尾高亮
    3. 中间行：整行高亮
    4. 末行：从行首到 `end_char_offset` 高亮
    5. 如果首行=末行：仅高亮 `[start_char_offset, end_char_offset)` 范围
  - 高亮辅助函数（添加到 `text_selection.rs` 或 `main_ui.rs` 内）：
    ```rust
    /// 对一行的 spans 做字符级选区高亮。
    /// `char_start` / `char_end` 是该行 plain_text 的**字符偏移**（非 byte 索引）。
    /// 将 spans 中对应范围的字符的 style 追加 Modifier::REVERSED，范围外的 span 保持原样。
    /// 使用 char_indices() 保证 unicode 安全切割。
    fn highlight_line_spans(
        spans: Vec<Span<'static>>,
        char_start: usize,
        char_end: usize,
    ) -> Vec<Span<'static>> {
        let mut result = Vec::new();
        let mut cursor: usize = 0; // 当前在 plain_text 中的字符位置
        for span in spans {
            let span_char_len = span.content.chars().count(); // 字符数（非 byte 数）
            let span_start = cursor;
            let span_end = cursor + span_char_len;

            if span_end <= char_start || span_start >= char_end {
                // 完全在选区外 → 保持原样
                result.push(span);
            } else if span_start >= char_start && span_end <= char_end {
                // 完全在选区内 → 整个 span 反色
                result.push(span.patch_style(Style::default().add_modifier(Modifier::REVERSED)));
            } else {
                // 部分重叠 → 拆分为 2~3 个子 span
                // 左段（选区外）
                if span_start < char_start {
                    let skip = char_start - span_start;
                    let byte_cut = span.content.char_indices()
                        .nth(skip).map(|(i,_)| i).unwrap_or(span.content.len());
                    result.push(Span::styled(
                        span.content[..byte_cut].to_string(),
                        span.style,
                    ));
                }
                // 中段（选区内，反色）
                let hl_char_start = span_start.max(char_start) - span_start;
                let hl_char_end = span_end.min(char_end) - span_start;
                let byte_start = span.content.char_indices()
                    .nth(hl_char_start).map(|(i,_)| i).unwrap_or(0);
                let byte_end = span.content.char_indices()
                    .nth(hl_char_end).map(|(i,_)| i).unwrap_or(span.content.len());
                result.push(Span::styled(
                    span.content[byte_start..byte_end].to_string(),
                    span.style.add_modifier(Modifier::REVERSED),
                ));
                // 右段（选区外）
                if span_end > char_end {
                    let skip = char_end - span_start;
                    let byte_cut = span.content.char_indices()
                        .nth(skip).map(|(i,_)| i).unwrap_or(span.content.len());
                    result.push(Span::styled(
                        span.content[byte_cut..].to_string(),
                        span.style,
                    ));
                }
            }
            cursor = span_end;
        }
        result
    }
    ```
  - 选区高亮主逻辑：
    ```rust
    // 字符级选区高亮
    if app.core.text_selection.is_active() {
        let ts = &app.core.text_selection;
        if let (Some(start), Some(end)) = (ts.start, ts.end) {
            let cache = app.core.render_cache.read();
            let wrap_map = &cache.wrap_map;
            let usable_width = app.core.messages_area
                .map(|a| a.width.saturating_sub(1))
                .unwrap_or(0);

            // 映射为逻辑坐标
            let (logical_start, logical_end) = {
                let ((sr, sc), (er, ec)) = if start <= end { (start, end) } else { (end, start) };
                let ls = visual_to_logical(sr, sc, wrap_map, usable_width);
                let le = visual_to_logical(er, ec, wrap_map, usable_width);
                (ls, le)
            };

            if let (Some((start_line, start_char)), Some((end_line, end_char))) = (logical_start, logical_end) {
                let start_line = wrap_map.iter().find(|i| i.line_idx == start_line).map(|i| i.line_idx).unwrap_or(start_line);
                for line_idx in start_line..=end_line {
                    if line_idx >= all_lines.len() { continue; }
                    let (cs, ce) = if line_idx == start_line && line_idx == end_line {
                        (start_char, end_char)
                    } else if line_idx == start_line {
                        (start_char, usize::MAX)
                    } else if line_idx == end_line {
                        (0, end_char)
                    } else {
                        (0, usize::MAX)
                    };
                    let spans = std::mem::take(&mut all_lines[line_idx].spans);
                    all_lines[line_idx] = Line::from(highlight_line_spans(spans, cs, ce));
                }
            }
            drop(cache);
        }
    }
    ```
  - **注意:** `usize::MAX` 作为"到行尾"的哨兵值，`highlight_line_spans` 中 `char_end = usize::MAX` 等价于不裁剪右端（`span_end <= usize::MAX` 恒成立，最后一个 else 分支中 `nth(char_end - span_start)` 不会越界因为 `unwrap_or` 兜底到 `span.content.len()`）。
  - **Unicode 安全:** `char_col_to_offset` 返回的是**字符索引**（遍历 `char_widths` 得到），所有对 `plain_text` 和 `span.content` 的切割均通过 `char_to_byte_idx` / `char_indices().nth()` 转换为字节索引后再切片，保证 CJK/emoji 等多字节字符不会 panic。

- [x] 修改 Ctrl+C 处理逻辑，增加选区复制最高优先级
  - 位置: `rust-agent-tui/src/event.rs` L214-226 Ctrl+C 分支
  - 将现有 Ctrl+C 分支替换为：
    ```rust
    Input {
        key: Key::Char('c'),
        ctrl: true,
        ..
    } => {
        if app.core.text_selection.selected_text.is_some() {
            // 有选区文本 → 复制到剪贴板（最高优先级）
            if let Some(text) = app.core.text_selection.selected_text.take() {
                let char_count = text.chars().count();
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(&text);
                }
                app.core.copy_char_count = char_count;
                app.core.copy_message_until = Some(
                    std::time::Instant::now() + std::time::Duration::from_millis(2000)
                );
                app.core.text_selection.clear();
            }
            return Ok(Some(Action::Redraw));
        } else if app.core.loading {
            app.interrupt();
        } else {
            return Ok(Some(Action::Quit));
        }
    }
    ```
  - 原因: 有选区时 Ctrl+C 应复制而非中断/退出，符合用户对文本选择的直觉预期。`arboard` crate 已是项目依赖（用于 Ctrl+V 粘贴）。

- [x] 在状态栏第一行显示复制成功提示
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs` `render_first_row()` 函数中，在"工作目录"部分之前（L51 之前）
  - 添加复制提示显示逻辑：
    ```rust
    // 复制成功提示
    if let Some(until) = app.core.copy_message_until {
        if std::time::Instant::now() < until {
            spans.push(Span::styled(
                format!(" ✅ 已复制 {} 个字符", app.core.copy_char_count),
                Style::default().fg(theme::SAGE),
            ));
            spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        }
    }
    ```

- [ ] 更新选区高亮逻辑的单元测试（字符级）
  - 测试文件: `rust-agent-tui/src/ui/main_ui.rs`（`#[cfg(test)] mod tests` 块，如不存在则新建）
  - 测试场景（需更新）:
    - `test_highlight_line_spans_full_span`: 整个 span 在选区内 → 返回 1 个 REVERSED span
    - `test_highlight_line_spans_partial_start`: 选区从 span 中间开始 → 返回 2 个 span（原样 + REVERSED）
    - `test_highlight_line_spans_partial_both`: 选区两端都在 span 内部 → 返回 3 个 span（原样 + REVERSED + 原样）
    - `test_highlight_line_spans_multi_span`: 多个 span，选区跨越两个 span → 第一个被拆分，第二个被拆分
    - `test_highlight_line_spans_outside`: 选区不覆盖该 span → 返回原 span 不变
  - 运行命令: `cargo test -p rust-agent-tui --lib -- ui::main_ui::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证 Ctrl+C 优先级正确
  - `grep -A 10 "key: Key::Char.*c.*ctrl: true" rust-agent-tui/src/event.rs | head -15`
  - 预期: 选区复制分支在 loading 中断和退出之前
- [x] 验证 messages_area 在渲染时更新
  - `grep -n "messages_area = Some" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 在 render_messages 中有赋值
- [x] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib -- ui::main_ui::tests`
  - 预期: 所有测试通过

---

### Task 5: 鼠标文本选择 验收

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`
- 至少发送一条消息使消息区域有可选中内容

**端到端验证:**

1. 运行完整测试套件确保无回归
   - [x] `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤，重点关注 Task 2 的 wrap_map 测试和 Task 3 的坐标映射测试

2. 鼠标滚轮滚动不受影响
   - 启动 TUI，发送一条消息后使用鼠标滚轮上下滚动
   - 预期: 消息列表正常滚动，行为与改动前一致
   - 失败排查: 检查 Task 3 的 event.rs 修改是否保留了 ScrollUp/ScrollDown 分支

3. 鼠标拖拽选中文本有反色高亮
   - 在消息区域按住鼠标左键拖拽
   - 预期: 拖拽过程中选中文字区域显示反色高亮
   - 失败排查: 检查 Task 4 的 render_messages 高亮逻辑，确认 wrap_map 与 lines 同步

4. 松开鼠标后高亮保持
   - 拖拽选中文字后松开鼠标
   - 预期: 选区高亮保持显示
   - 失败排查: 检查 Task 1 的 end_drag 和 Task 4 的高亮条件 `is_active()`

5. Ctrl+C 复制选中文字并清除高亮
   - 选中文字后按 Ctrl+C
   - 预期: 高亮消失，状态栏短暂显示"已复制 N 个字符"，系统剪贴板包含选中文字
   - 失败排查: 检查 Task 4 的 Ctrl+C 优先级分支和 arboard 剪贴板调用

6. 无选区时 Ctrl+C 保持原有行为
   - 无选区时 Ctrl+C（非 loading 状态）→ 退出应用
   - Loading 时 Ctrl+C（无选区）→ 中断 Agent
   - 预期: 行为与改动前一致
   - 失败排查: 检查 Task 4 的 Ctrl+C 分支优先级链

7. CJK 字符选区计算正确
   - 消息中包含中文内容，拖拽选中
   - 预期: 选中范围与显示对齐，复制出的文字完整正确
   - 失败排查: 检查 Task 2 的 build_wrap_map 中 unicode-width 计算

8. 跨行选区文本提取正确
   - 拖拽选中跨越多行的文字
   - 预期: 复制的文本包含换行符，多行内容完整
   - 失败排查: 检查 Task 3 的 extract_selected_text 跨行拼接逻辑

9. 窗口 resize 后选区清除
   - 选中文字后调整终端窗口大小
   - 预期: 选区自动清除，无残留高亮
   - 失败排查: 检查 Task 3 的 Event::Resize 分支是否调用 text_selection.clear()