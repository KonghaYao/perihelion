# perihelion-widgets 组件库执行计划（1/3）

**目标:** 创建独立的 ratatui widget crate，实现 Theme trait、BorderedPanel、ScrollableArea、SelectableList 基础组件

**技术栈:** Rust 2021 edition, ratatui ≥0.30, Cargo Workspace

**设计文档:** spec/feature_20260427_F001_ratatui-widget-lib/spec-design.md

## 改动总览

- 新增 `perihelion-widgets` workspace crate，包含 Theme trait、DarkTheme、BorderedPanel、ScrollableArea、ListState<T>、SelectableList 组件
- Task 1 创建 crate 骨架和 Theme trait，Task 2-3 依赖 Task 1 的 Theme 和 ScrollState
- 关键决策：Theme trait 仅含纯 UI 颜色方法（不含 tool_danger 等业务方法）；RenderState 颜色参数化在 Task 7（MarkdownRenderer）处理

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证全量构建可用
  - `cargo build` 在 workspace 根目录
  - 预期: 所有 5 个 crate 编译成功
- [x] 验证全量测试可用
  - `cargo test` 在 workspace 根目录
  - 预期: 测试框架正常（允许 1 个已知的多线程运行时失败：`command::loop_cmd::tests::test_loop_cmd_valid_args_submits_message`）

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 和 "Finished"，无 error
- [x] 测试命令可用
  - `cargo test 2>&1 | grep "test result"`
  - 预期: 显示多个 "test result" 行，大部分 passed

---

### Task 1: Crate 骨架 + Theme trait

**背景:**
创建 `perihelion-widgets` crate 的基础结构。本 Task 是整个组件库的地基——后续所有 Task（2-10）都依赖本 Task 创建的 crate 骨架和 Theme trait。Theme trait 仅包含纯 UI 颜色方法，不含业务语义（如工具分级色）。现有 `rust-agent-tui/src/ui/theme.rs` 有 15 个颜色常量，经分析 13 个是纯 UI 概念、2 个是业务别名（TOOL_NAME=SAGE, SUB_AGENT=SAGE）、1 个是业务专用（MODEL_INFO），这些业务常量保留在 TUI 层不迁移。

**涉及文件:**
- 新建: `perihelion-widgets/Cargo.toml`
- 新建: `perihelion-widgets/src/lib.rs`
- 新建: `perihelion-widgets/src/theme/mod.rs`
- 新建: `perihelion-widgets/src/theme/presets.rs`
- 修改: `Cargo.toml`（~L2，在 members 数组中添加 `perihelion-widgets`）

**执行步骤:**
- [x] 创建 `perihelion-widgets/Cargo.toml`
  - 位置: workspace 根目录下新建
  - 内容:
    ```toml
    [package]
    name = "perihelion-widgets"
    version = "0.1.0"
    edition = "2021"
    description = "Reusable ratatui widget library for Perihelion"

    [dependencies]
    ratatui = { version = ">=0.30", features = ["unstable-rendered-line-info"] }
    pulldown-cmark = { version = "0.12", optional = true }
    unicode-width = "0.2"

    [features]
    default = []
    markdown = ["pulldown-cmark"]
    ```
  - 原因: pulldown-cmark 仅被 MarkdownRenderer 使用，通过 feature gate 按需启用。unicode-width 是 InputState 必需依赖（Task 4 光标定位），不设为 optional

- [x] 在 workspace Cargo.toml 中注册新 crate
  - 位置: `Cargo.toml` ~L2 members 数组
  - 在 `"langfuse-client",` 行后追加 `"perihelion-widgets",`

- [x] 创建 `perihelion-widgets/src/lib.rs`
  - 位置: crate 根模块
  - 内容:
    ```rust
    pub mod theme;

    // 重导出核心类型
    pub use theme::{Theme, DarkTheme};
    ```

- [x] 创建 `perihelion-widgets/src/theme/mod.rs`
  - 位置: theme 子模块入口
  - 内容:
    ```rust
    mod presets;

    pub use presets::DarkTheme;

    use ratatui::style::Color;

    /// 纯 UI 颜色主题 trait——不含业务语义方法
    ///
    /// 组件通过此 trait 查询颜色，不硬编码色值。
    /// 业务特有颜色（工具分级色、模型信息色等）由调用方在 TUI 层自行管理。
    pub trait Theme: Clone + Send + Sync + 'static {
        // ── 强调色 ──────────────────────────────────────────────
        /// 主交互色（激活边框、光标、关键操作）
        fn accent(&self) -> Color;

        // ── 功能色 ──────────────────────────────────────────────
        /// 成功/完成色
        fn success(&self) -> Color;
        /// 次要强调/警告色
        fn warning(&self) -> Color;
        /// 错误/拒绝色
        fn error(&self) -> Color;
        /// 推理/思考色
        fn thinking(&self) -> Color;

        // ── 文字层级 ────────────────────────────────────────────
        /// 主文字（需要立即看到的内容）
        fn text(&self) -> Color;
        /// 次要文字（标签、路径、辅助信息）
        fn muted(&self) -> Color;
        /// 极弱文字（占位、已完成项、分隔符）
        fn dim(&self) -> Color;

        // ── 边框 ────────────────────────────────────────────────
        /// 空闲边框色
        fn border(&self) -> Color;
        /// 激活边框色（输入框/当前 panel focus）
        fn border_active(&self) -> Color;

        // ── 弹窗专用 ────────────────────────────────────────────
        /// 弹窗底色（Clear 后的背景）
        fn popup_bg(&self) -> Color;
        /// 光标行背景（列表选中行）
        fn cursor_bg(&self) -> Color;

        // ── 状态 ────────────────────────────────────────────────
        /// Loading 色（高辨识度状态指示）
        fn loading(&self) -> Color;
    }
    ```
  - 原因: 13 个方法对应现有 theme.rs 中全部 13 个纯 UI 颜色常量。不含 TOOL_NAME、SUB_AGENT、MODEL_INFO 等业务别名/专用常量

- [x] 创建 `perihelion-widgets/src/theme/presets.rs`
  - 位置: 预设主题实现
  - 内容:
    ```rust
    use ratatui::style::Color;
    use super::Theme;

    /// 项目默认深色主题
    ///
    /// 色值与 rust-agent-tui/src/ui/theme.rs 的常量一一对应。
    /// 业务特有常量（TOOL_NAME=SAGE, SUB_AGENT=SAGE, MODEL_INFO=#A0825F）
    /// 保留在 TUI 层，不在此处定义。
    #[derive(Debug, Clone)]
    pub struct DarkTheme;

    impl Theme for DarkTheme {
        fn accent(&self) -> Color { Color::Rgb(255, 107, 43) }      // ACCENT #FF6B2B
        fn success(&self) -> Color { Color::Rgb(110, 181, 106) }    // SAGE #6EB56A
        fn warning(&self) -> Color { Color::Rgb(176, 152, 120) }    // WARNING #B09878
        fn error(&self) -> Color { Color::Rgb(204, 70, 62) }        // ERROR #CC463E
        fn thinking(&self) -> Color { Color::Rgb(167, 139, 250) }   // THINKING #A78BFA
        fn text(&self) -> Color { Color::Rgb(218, 206, 208) }       // TEXT #DACED0
        fn muted(&self) -> Color { Color::Rgb(140, 125, 120) }      // MUTED #8C7D78
        fn dim(&self) -> Color { Color::Rgb(72, 62, 58) }           // DIM #483E3A
        fn border(&self) -> Color { Color::Rgb(48, 38, 32) }        // BORDER #302620
        fn border_active(&self) -> Color { Color::Rgb(255, 107, 43) } // = accent
        fn popup_bg(&self) -> Color { Color::Rgb(10, 8, 6) }        // POPUP_BG #0A0806
        fn cursor_bg(&self) -> Color { Color::Rgb(38, 22, 10) }     // CURSOR_BG #261608
        fn loading(&self) -> Color { Color::Rgb(34, 211, 238) }     // LOADING #22D3EE
    }
    ```
  - 原因: DarkTheme 是零大小类型（ZST），所有色值编译期内联，无运行时开销

- [x] 验证 crate 独立编译
  - `cargo build -p perihelion-widgets`
  - 预期: 编译成功

- [x] 为 Theme trait 和 DarkTheme 编写单元测试
  - 测试文件: `perihelion-widgets/src/theme/presets.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `dark_theme_returns_correct_colors`: 验证 `DarkTheme` 的 `accent()` 返回 `Color::Rgb(255, 107, 43)`
    - `dark_theme_trait_object_usable`: 验证 `let theme: &dyn Theme = &DarkTheme;` 可正常调用所有方法
    - `dark_theme_cloneable`: 验证 `DarkTheme` 可 clone（`Theme: Clone` 约束）
  - 运行命令: `cargo test -p perihelion-widgets`
  - 预期: 所有测试通过

**检查步骤:**
- [x] workspace Cargo.toml 包含新 crate
  - `grep "perihelion-widgets" /Users/konghayao/code/ai/perihelion/Cargo.toml`
  - 预期: 输出包含 `"perihelion-widgets"`
- [x] 新 crate 可独立编译
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 输出包含 "Compiling perihelion-widgets" 和 "Finished"
- [x] 公共 API 可被外部 crate 引用
  - `cargo test -p perihelion-widgets 2>&1 | grep "test result"`
  - 预期: 所有测试通过

---

### Task 2: BorderedPanel + ScrollableArea

**背景:**
实现两个最基础的容器组件。BorderedPanel 封装 TUI 中 8+ 处重复的 `Clear + Block + borders` 模式（popups/hitl.rs、popups/ask_user.rs、popups/hints.rs、popups/setup_wizard.rs、panels/model.rs、panels/agent.rs、panels/relay.rs、panels/thread_browser.rs、panels/cron.rs）。ScrollableArea 封装 6+ 处重复的 `Paragraph + scroll offset + Scrollbar` 模式。ScrollState 内含 ensure_visible 方法，从 `app/mod.rs:ensure_cursor_visible()` 迁移。Task 3 的 ListState<T> 内嵌 ScrollState，依赖本 Task 的 ScrollState 定义。

**涉及文件:**
- 新建: `perihelion-widgets/src/bordered_panel.rs`
- 新建: `perihelion-widgets/src/scrollable.rs`
- 修改: `perihelion-widgets/src/lib.rs`（添加 mod 声明和重导出）

**执行步骤:**
- [x] 创建 `perihelion-widgets/src/bordered_panel.rs`
  - 位置: crate 根目录
  - 内容:
    ```rust
    use ratatui::{
        layout::Rect,
        style::Style,
        text::Line,
        widgets::{Block, Borders, Clear},
        Frame,
    };

    /// 带边框容器——封装 Clear + Block + borders 一步到位
    ///
    /// render() 返回 inner Rect 供后续渲染使用。
    pub struct BorderedPanel<'a> {
        title: Line<'a>,
        border_style: Style,
    }

    impl<'a> BorderedPanel<'a> {
        pub fn new(title: impl Into<Line<'a>>) -> Self {
            Self {
                title: title.into(),
                border_style: Style::default(),
            }
        }

        pub fn border_style(mut self, style: Style) -> Self {
            self.border_style = style;
            self
        }

        /// 渲染边框面板：先 Clear 背景，再渲染 Block 边框，返回 inner area
        pub fn render(self, f: &mut Frame, area: Rect) -> Rect {
            f.render_widget(Clear, area);
            let block = Block::default()
                .title(self.title)
                .borders(Borders::ALL)
                .border_style(self.border_style);
            let inner = block.inner(area);
            f.render_widget(&block, area);
            inner
        }
    }
    ```
  - 原因: 将 `f.render_widget(Clear, area)` + `Block::default().title(...).borders(Borders::ALL).border_style(...)` + `block.inner(area)` 三步封装为一步调用

- [x] 创建 `perihelion-widgets/src/scrollable.rs`
  - 位置: crate 根目录
  - 内容:
    ```rust
    use ratatui::{
        layout::Rect,
        style::Style,
        text::Text,
        widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
        Frame,
    };

    /// 滚动偏移状态
    ///
    /// 管理垂直滚动 offset，提供 ensure_visible 方法自动调整 offset 使指定行可见。
    /// 逻辑从 `rust-agent-tui/src/app/mod.rs:ensure_cursor_visible()` 迁移。
    #[derive(Debug, Clone, Default)]
    pub struct ScrollState {
        offset: u16,
    }

    impl ScrollState {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn offset(&self) -> u16 {
            self.offset
        }

        /// 向上滚动 delta 行
        pub fn scroll_up(&mut self, delta: u16) {
            self.offset = self.offset.saturating_sub(delta);
        }

        /// 向下滚动 delta 行（不超过 max_scroll）
        pub fn scroll_down(&mut self, delta: u16, content_height: u16, visible_height: u16) {
            let max_scroll = content_height.saturating_sub(visible_height);
            self.offset = (self.offset + delta).min(max_scroll);
        }

        /// 确保 row 行在可见视口内，自动调整 offset
        ///
        /// 从 `ensure_cursor_visible(cursor_row, scroll_offset, visible_height)` 迁移。
        pub fn ensure_visible(&mut self, row: u16, visible_height: u16) {
            if visible_height == 0 {
                self.offset = 0;
                return;
            }
            if row < self.offset {
                self.offset = row;
            } else if row >= self.offset + visible_height {
                self.offset = row.saturating_sub(visible_height.saturating_sub(1));
            }
        }

        pub fn reset(&mut self) {
            self.offset = 0;
        }
    }

    /// 可滚动区域——内容 + 可选滚动条
    pub struct ScrollableArea<'a> {
        content: Text<'a>,
        show_scrollbar: bool,
        scrollbar_style: Style,
    }

    impl<'a> ScrollableArea<'a> {
        pub fn new(content: Text<'a>) -> Self {
            Self {
                content,
                show_scrollbar: true,
                scrollbar_style: Style::default(),
            }
        }

        pub fn show_scrollbar(mut self, show: bool) -> Self {
            self.show_scrollbar = show;
            self
        }

        pub fn scrollbar_style(mut self, style: Style) -> Self {
            self.scrollbar_style = style;
            self
        }

        /// 渲染可滚动区域：Paragraph + 可选 Scrollbar
        ///
        /// 自动根据内容高度和可见高度决定是否显示滚动条。
        /// 内容区域宽度减 1 留给滚动条（当 scrollbar 显示时）。
        pub fn render(self, f: &mut Frame, area: Rect, state: &mut ScrollState) {
            let content_height = self.content.height() as u16;
            let visible_height = area.height;
            let max_scroll = content_height.saturating_sub(visible_height);
            // clamp offset
            state.offset = state.offset.min(max_scroll);

            let needs_scrollbar = self.show_scrollbar && content_height > visible_height;
            let text_width = if needs_scrollbar {
                area.width.saturating_sub(1)
            } else {
                area.width
            };
            let text_area = Rect { width: text_width, ..area };

            let paragraph = Paragraph::new(self.content)
                .scroll((state.offset, 0))
                .wrap(Wrap { trim: false });
            f.render_widget(paragraph, text_area);

            if needs_scrollbar {
                let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
                    .position(state.offset as usize);
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .style(self.scrollbar_style);
                f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
            }
        }
    }
    ```
  - 原因: ScrollState 的 ensure_visible 方法合并了 TUI 层 `ensure_cursor_visible()` 的逻辑；ScrollableArea 封装了 Paragraph + Scrollbar + offset clamp 的完整渲染逻辑

- [x] 更新 `perihelion-widgets/src/lib.rs` 添加 mod 声明和重导出
  - 位置: `perihelion-widgets/src/lib.rs`
  - 将现有内容替换为:
    ```rust
    pub mod bordered_panel;
    pub mod scrollable;
    pub mod theme;

    // 重导出核心类型
    pub use bordered_panel::BorderedPanel;
    pub use scrollable::{ScrollState, ScrollableArea};
    pub use theme::{DarkTheme, Theme};
    ```
  - 原因: 添加 bordered_panel 和 scrollable 模块声明及重导出

- [x] 为 BorderedPanel 编写单元测试
  - 测试文件: `perihelion-widgets/src/bordered_panel.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `render_returns_inner_area`: 创建 10x6 的 TestBackend area，调用 `BorderedPanel::new("Title").border_style(Style::default()).render(f, area)`，验证返回的 inner area 宽度 = area.width - 2（左右边框各 1）、高度 = area.height - 2（上下边框各 1）
    - `render_with_empty_title`: 使用空标题 `""`，验证不 panic 且 inner area 正常返回
  - 运行命令: `cargo test -p perihelion-widgets -- bordered_panel`
  - 预期: 所有测试通过

- [x] 为 ScrollState 和 ScrollableArea 编写单元测试
  - 测试文件: `perihelion-widgets/src/scrollable.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `scroll_state_ensure_visible_above`: offset=5, row=2, visible=10 → offset 应变为 2
    - `scroll_state_ensure_visible_below`: offset=0, row=15, visible=10 → offset 应变为 6（15 - (10-1)）
    - `scroll_state_ensure_visible_within`: offset=3, row=5, visible=10 → offset 保持 3
    - `scroll_state_scroll_up_down`: scroll_down(3, 20, 10) → offset=3; scroll_up(1) → offset=2
    - `scrollable_area_renders_content`: 创建 20 行内容，20x5 area，验证 Paragraph 渲染到 buffer 中（使用 TestBackend 检查）
    - `scrollable_area_clamps_offset`: offset=100 但内容仅 20 行、可见 5 行，render 后 offset 应 clamp 为 15
  - 运行命令: `cargo test -p perihelion-widgets -- scrollable`
  - 预期: 所有测试通过

**检查步骤:**
- [x] bordered_panel.rs 和 scrollable.rs 存在且可编译
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] BorderedPanel 和 ScrollableArea 在 lib.rs 中正确重导出
  - `grep -E "pub use (bordered_panel|scrollable)" /Users/konghayao/code/ai/perihelion/perihelion-widgets/src/lib.rs`
  - 预期: 输出包含 `pub use bordered_panel::BorderedPanel` 和 `pub use scrollable::{ScrollState, ScrollableArea}`
- [x] 全部单元测试通过
  - `cargo test -p perihelion-widgets 2>&1 | grep "test result"`
  - 预期: 所有测试通过

---

### Task 3: SelectableList + ListState<T>

**背景:**
实现泛型列表选择组件，封装 TUI 中 5+ 处重复的光标管理 + 滚动联动 + 列表渲染模式。ListState<T> 内嵌 Task 2 创建的 ScrollState，统一管理 items、cursor、scroll_offset。SelectableList 通过闭包自定义每项渲染（支持"特殊首项"模式——cursor 0 是 "New Thread"/"No Agent" 等特殊选项）。本 Task 是 plan-2 中 Task 8（TUI 集成替换 agent_panel、thread_browser、cron_panel 列表）的前置依赖。

**涉及文件:**
- 新建: `perihelion-widgets/src/list.rs`
- 修改: `perihelion-widgets/src/lib.rs`（添加 `pub mod list;` 和 `pub use list::{ListState, SelectableList};`）

**执行步骤:**
- [x] 创建 `perihelion-widgets/src/list.rs`
  - 位置: crate 根目录
  - 内容:
    ```rust
    use ratatui::{
        layout::Rect,
        prelude::*,
        style::Style,
        text::{Line, Text},
        widgets::{Paragraph, StatefulWidget, Widget},
    };
    use crate::scrollable::ScrollState;

    /// 泛型列表状态——管理 items + cursor + scroll offset
    ///
    /// T 不要求 Clone。cursor 使用 clamp 模式（不循环）。
    /// 内嵌 ScrollState，滚动与光标联动通过 ensure_visible 自动处理。
    pub struct ListState<T> {
        items: Vec<T>,
        cursor: usize,
        pub scroll: ScrollState,
    }

    impl<T> ListState<T> {
        pub fn new(items: Vec<T>) -> Self {
            Self { items, cursor: 0, scroll: ScrollState::new() }
        }

        pub fn items(&self) -> &[T] { &self.items }

        pub fn cursor(&self) -> usize { self.cursor }

        /// 移动光标（clamp 模式，不循环）
        pub fn move_cursor(&mut self, delta: i32) {
            if self.items.is_empty() { return; }
            let max = self.items.len() - 1;
            let new = self.cursor as i32 + delta;
            self.cursor = new.clamp(0, max as i32) as usize;
        }

        /// 确保 cursor 不超过 items.len()（外部修改 items 后调用）
        pub fn clamp_cursor(&mut self) {
            if self.items.is_empty() {
                self.cursor = 0;
            } else {
                self.cursor = self.cursor.min(self.items.len() - 1);
            }
        }

        /// 获取当前 cursor 指向的 item 引用
        pub fn selected(&self) -> Option<&T> { self.items.get(self.cursor) }

        /// 获取当前 cursor 指向的 item 可变引用
        pub fn selected_mut(&mut self) -> Option<&mut T> { self.items.get_mut(self.cursor) }

        /// 确保 cursor 行在可见视口内（联动 ScrollState）
        pub fn ensure_visible(&mut self, visible: u16) {
            self.scroll.ensure_visible(self.cursor as u16, visible);
        }

        /// 替换 items 列表，自动 clamp cursor
        pub fn set_items(&mut self, items: Vec<T>) {
            self.items = items;
            self.clamp_cursor();
        }
    }

    /// 可选择列表 widget——通过闭包自定义每项渲染
    ///
    /// 实现 ratatui StatefulWidget trait，状态类型为 ListState<T>。
    /// render_item 闭包签名为 `Fn(&T, bool) -> Line<'a>`，bool 表示当前行是否为 cursor。
    /// "特殊首项"模式由调用方在闭包中处理（如 items[0] 是 "New Thread"）。
    pub struct SelectableList<'a, T> {
        render_item: Box<dyn Fn(&T, bool) -> Line<'a>>,
        cursor_marker: &'a str,
        cursor_style: Style,
        normal_style: Style,
    }

    impl<'a, T> SelectableList<'a, T> {
        pub fn new(render_item: impl Fn(&T, bool) -> Line<'a> + 'a) -> Self {
            Self {
                render_item: Box::new(render_item),
                cursor_marker: "▶ ",
                cursor_style: Style::default(),
                normal_style: Style::default(),
            }
        }

        pub fn cursor_marker(mut self, marker: &'a str) -> Self {
            self.cursor_marker = marker;
            self
        }

        pub fn cursor_style(mut self, style: Style) -> Self {
            self.cursor_style = style;
            self
        }

        pub fn normal_style(mut self, style: Style) -> Self {
            self.normal_style = style;
            self
        }
    }

    impl<T> StatefulWidget for SelectableList<'_, T> {
        type State = ListState<T>;

        fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
            let cursor = state.cursor;

            let mut lines: Vec<Line<'_>> = Vec::with_capacity(state.items.len());
            for (i, item) in state.items.iter().enumerate() {
                let is_cursor = i == cursor;
                let line = (self.render_item)(item, is_cursor);
                let marker = if is_cursor {
                    Span::styled(self.cursor_marker.to_string(), self.cursor_style)
                } else {
                    Span::styled(
                        " ".repeat(self.cursor_marker.chars().count()),
                        self.normal_style,
                    )
                };
                let mut spans = vec![marker];
                spans.extend(line.spans.iter().cloned());
                lines.push(Line::from(spans));
            }

            let text = Text::from(lines);
            let visible = area.height;
            state.scroll.ensure_visible(cursor as u16, visible);

            let paragraph = Paragraph::new(text).scroll((state.scroll.offset(), 0));
            Widget::render(paragraph, area, buf);
        }
    }
    ```
  - 原因: ListState<T> 内嵌 Task 2 的 ScrollState，统一 cursor + scroll 管理。SelectableList 通过闭包实现渲染灵活性。StatefulWidget 直接操作 Buffer，cursor_marker 自动在行首插入。

- [x] 更新 `perihelion-widgets/src/lib.rs` 添加 list 模块
  - 位置: `perihelion-widgets/src/lib.rs`
  - 在 `pub mod scrollable;` 行之后插入 `pub mod list;`
  - 在重导出区域添加 `pub use list::{ListState, SelectableList};`

- [x] 为 ListState<T> 和 SelectableList 编写单元测试
  - 测试文件: `perihelion-widgets/src/list.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `list_state_move_cursor_clamp`: items=["a","b","c"], move_cursor(1)→cursor=1, move_cursor(5)→cursor=2（clamp 到 max）, move_cursor(-10)→cursor=0（clamp 到 0）
    - `list_state_empty_items`: items=[], move_cursor(1) 不 panic，selected() 返回 None
    - `list_state_set_items_clamp_cursor`: items=["a","b","c","d"], cursor=3, set_items(vec!["x"])→cursor=0（clamp 后）
    - `list_state_ensure_visible`: items 长度 20, cursor=15, ensure_visible(10)→scroll.offset 应为 6（15-(10-1)）
    - `selectable_list_renders_cursor_marker`: 使用 TestBackend（ratatui::backend::TestBackend::new(20, 5)），3 个 items ["a","b","c"]，cursor=1，render 后验证 buffer 第 2 行包含 "▶ " 前缀，其余行包含 "  " 前缀
    - `selectable_list_custom_render_item`: 闭包返回 `Line::styled(format!("[{}]", item), Style::default().bold())`，验证 buffer 包含 "[a]"、"[b]" 等自定义渲染内容
  - 运行命令: `cargo test -p perihelion-widgets -- list`
  - 预期: 所有测试通过

**检查步骤:**
- [x] list.rs 存在且可编译
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] ListState 和 SelectableList 在 lib.rs 中正确重导出
  - `grep "pub use list" /Users/konghayao/code/ai/perihelion/perihelion-widgets/src/lib.rs`
  - 预期: 输出包含 `pub use list::{ListState, SelectableList}`
- [x] 全部单元测试通过
  - `cargo test -p perihelion-widgets 2>&1 | grep "test result"`
  - 预期: 所有测试通过

---

