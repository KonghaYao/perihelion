# perihelion-widgets 组件库执行计划（2/3）

**目标:** 实现 InputField、TabBar、RadioGroup、CheckboxGroup、FormState、MarkdownRenderer 组件

**技术栈:** Rust 2021 edition, ratatui ≥0.30, pulldown-cmark 0.12, unicode-width 0.2

**设计文档:** spec/feature_20260427_F001_ratatui-widget-lib/spec-design.md

## 改动总览

- 本文件实现 4 个交互组件（InputField、TabBar/RadioGroup/CheckboxGroup）、1 个状态管理组件（FormState）、1 个渲染组件（MarkdownRenderer）
- Task 4 InputField 依赖 Task 1 的 crate 骨架；Task 6 FormState 依赖 Task 4 的 InputState
- Task 7 MarkdownRenderer 从 TUI 层迁移 render_state.rs，颜色参数化为 MarkdownTheme，ContentBlockView 和 ensure_rendered 保留在 TUI 层
- 关键决策：InputField 实现 StatefulWidget（架构评审修正）；MarkdownRenderer 仅提供 `parse_markdown(input, theme) -> Text` 公共 API

---

### Task 4: InputField + InputState

**背景:**
实现文本输入框组件 InputField 和状态管理 InputState。封装 TUI 中 4+ 处重复的表单字段渲染模式（model_panel.rs 的 EditField + buf_*、relay_panel.rs 的 RelayEditField + current_buf、setup_wizard.rs 的 Step1Field、ask_user_prompt.rs 的 custom_input）。InputState 管理 buffer + cursor（UTF-8 字节偏移）+ masked 标志，提供完整的文本编辑操作集。InputField 实现 ratatui StatefulWidget trait（架构评审修正，与 CheckboxGroup/RadioGroup 保持一致），而非设计文档中最初的 render() 方法。Task 6 的 FormState<Field> 内嵌 HashMap<F, InputState>，依赖本 Task 的 InputState 定义。

**涉及文件:**
- 新建: `perihelion-widgets/src/input_field.rs`
- 修改: `perihelion-widgets/src/lib.rs`（添加 `pub mod input_field;` 和重导出）
- 确认: `perihelion-widgets/Cargo.toml`（确认 unicode-width 为必需依赖，已在 Task 1 设置）

**执行步骤:**
- [x] 确认 `perihelion-widgets/Cargo.toml` 中 unicode-width 为必需依赖
  - 位置: `perihelion-widgets/Cargo.toml` dependencies 区域
  - 经 Task 1 已将 `unicode-width` 设为必需依赖 `"0.2"`（非 optional），确认该行存在即可
  - 确认 `[features]` 中无 `input = ["unicode-width"]` 行（已在 Task 1 中移除）
  - 原因: InputState 的 cursor 定位和 display_width 计算依赖 unicode-width

- [x] 创建 `perihelion-widgets/src/input_field.rs`

- [x] 更新 `perihelion-widgets/src/lib.rs` 添加 input_field 模块

- [x] 为 InputState 编写单元测试

**检查步骤:**
- [x] input_field.rs 存在且可编译
- [x] InputState 和 InputField 在 lib.rs 中正确重导出
- [x] 全部单元测试通过

---

### Task 5: TabBar + RadioGroup + CheckboxGroup

**背景:**
实现三个选择类交互组件，封装 TUI 中 5+ 处重复的标签导航、单选、多选渲染模式。TabBar 用于 ask_user_popup 的多问题 Tab 和 model_panel 的 Alias Tab（Opus/Sonnet/Haiku）；RadioGroup 用于 ask_user_popup 的单选选项；CheckboxGroup 用于 hitl_popup 的批量审批。三个组件均实现 ratatui StatefulWidget trait，与 Task 4 的 InputField 保持一致。Task 8-10 TUI 集成时将替换这些现有渲染代码。

**涉及文件:**
- 新建: `perihelion-widgets/src/tab_bar.rs`
- 新建: `perihelion-widgets/src/radio_group.rs`
- 新建: `perihelion-widgets/src/checkbox_group.rs`
- 修改: `perihelion-widgets/src/lib.rs`（添加 3 个 mod 声明和重导出）

**执行步骤:**
- [x] 创建 `perihelion-widgets/src/tab_bar.rs`

- [x] 创建 `perihelion-widgets/src/radio_group.rs`

- [x] 创建 `perihelion-widgets/src/checkbox_group.rs`

- [x] 更新 `perihelion-widgets/src/lib.rs` 添加 3 个模块和重导出

- [x] 为三个组件编写单元测试

**检查步骤:**
- [x] 三个组件文件存在且可编译
- [x] 所有公共类型在 lib.rs 中正确重导出
- [x] 全部单元测试通过

---

### Task 6: FormState<Field> 泛型表单状态管理

**背景:**
实现泛型表单状态管理 FormState<F>，将 TUI 中 model_panel.rs 和 relay_panel.rs 重复的 EditField + buf_* 模式统一抽象。当前 ModelPanel 有 6 个 buf_* 字段 + EditField 枚举（Name/ProviderType/ModelId/ApiKey/BaseUrl/ThinkingBudget），RelayPanel 有 3 个 buf_* 字段 + RelayEditField 枚举（Url/Token/Name）。两者都实现了 next()/prev()/label() 方法 + push_char/pop_char/paste_text/cursor_* 操作。FormState<F> 用 HashMap<F, InputState> 替代这些 buf_* 字段，FormField trait 统一 next/prev/label 循环。Task 6 依赖 Task 4 的 InputState 定义。Task 10 TUI 集成时将 RelayPanel 和 ModelPanel 的 EditField+buf 替换为 FormState。

**涉及文件:**
- 新建: `perihelion-widgets/src/form.rs`
- 修改: `perihelion-widgets/src/lib.rs`（添加 `pub mod form;` 和重导出）

**执行步骤:**
- [x] 创建 `perihelion-widgets/src/form.rs`

- [x] 更新 `perihelion-widgets/src/lib.rs` 添加 form 模块

- [x] 为 FormState 编写单元测试

**检查步骤:**
- [x] form.rs 存在且可编译
- [x] FormField 和 FormState 在 lib.rs 中正确重导出
- [x] 全部单元测试通过

---

### Task 7: MarkdownRenderer 迁移

**背景:**
从 `rust-agent-tui/src/ui/markdown/` 迁移 Markdown 渲染器到 `perihelion-widgets/src/markdown/`。现有 `render_state.rs` 有 19 处硬编码 `theme::XXX` 颜色常量（heading 用 WARNING、border/separator/code_tag 用 MUTED、text/list_bullet 用 TEXT、code_prefix/link 用 SAGE），迁移后需参数化为 `MarkdownTheme` trait。`ContentBlockView`（定义在 message_view.rs）和 `ensure_rendered()`（操作 ContentBlockView 的 dirty/lazy 渲染）保留在 TUI 层——它们是 TUI 视图模型的一部分，不是通用 markdown 渲染。Task 8 TUI 集成时将 `rust-agent-tui/src/ui/markdown/mod.rs` 改为调用 widget crate 的 `parse_markdown()`。

**涉及文件:**
- 新建: `perihelion-widgets/src/markdown/mod.rs`
- 新建: `perihelion-widgets/src/markdown/render_state.rs`
- 修改: `perihelion-widgets/src/lib.rs`（添加 `#[cfg(feature = "markdown")] pub mod markdown;` 和条件重导出）
- 修改: `perihelion-widgets/Cargo.toml`（pulldown-cmark 已在 `[features]` 中定义，确认即可）

**执行步骤:**
- [x] 创建 `perihelion-widgets/src/markdown/mod.rs`
  - 位置: crate 根目录下 markdown/ 子目录
  - 内容:
    ```rust
    mod render_state;

    use pulldown_cmark::{Options, Parser};
    use ratatui::style::Color;
    use ratatui::text::Text;

    use render_state::RenderState;

    // ── MarkdownTheme trait ──────────────────────────────────────

    /// Markdown 渲染颜色主题——将 render_state.rs 中的 19 处硬编码颜色参数化
    pub trait MarkdownTheme {
        /// 标题颜色（H1-H3，对应原 theme::WARNING）
        fn heading(&self) -> Color;
        /// 主文字颜色（列表前缀、代码内容，对应原 theme::TEXT）
        fn text(&self) -> Color;
        /// 弱化文字颜色（边框、分隔线、代码标签，对应原 theme::MUTED）
        fn muted(&self) -> Color;
        /// 行内代码颜色（对应原 theme::WARNING，与 heading 共用）
        fn code(&self) -> Color;
        /// 链接颜色（对应原 theme::SAGE）
        fn link(&self) -> Color;
        /// 代码块行前缀颜色（`│`，对应原 theme::SAGE）
        fn code_prefix(&self) -> Color;
        /// 引用块前缀颜色（`▍`，对应原 theme::MUTED）
        fn quote_prefix(&self) -> Color;
        /// 列表项目符号颜色（`•`，对应原 theme::TEXT）
        fn list_bullet(&self) -> Color;
        /// 水平线颜色（`─`，对应原 theme::MUTED）
        fn separator(&self) -> Color;
    }

    /// 默认 Markdown 主题——色值与 DarkTheme 一致
    #[derive(Debug, Clone)]
    pub struct DefaultMarkdownTheme;

    impl MarkdownTheme for DefaultMarkdownTheme {
        fn heading(&self) -> Color { Color::Rgb(176, 152, 120) }    // WARNING
        fn text(&self) -> Color { Color::Rgb(218, 206, 208) }       // TEXT
        fn muted(&self) -> Color { Color::Rgb(140, 125, 120) }      // MUTED
        fn code(&self) -> Color { Color::Rgb(176, 152, 120) }       // WARNING
        fn link(&self) -> Color { Color::Rgb(110, 181, 106) }       // SAGE
        fn code_prefix(&self) -> Color { Color::Rgb(110, 181, 106) } // SAGE
        fn quote_prefix(&self) -> Color { Color::Rgb(140, 125, 120) } // MUTED
        fn list_bullet(&self) -> Color { Color::Rgb(218, 206, 208) } // TEXT
        fn separator(&self) -> Color { Color::Rgb(140, 125, 120) }  // MUTED
    }

    /// 解析 markdown 文本为 ratatui Text
    pub fn parse_markdown(input: &str, theme: &dyn MarkdownTheme) -> Text<'static> {
        if input.is_empty() {
            return Text::raw("");
        }
        let options = Options::all() - Options::ENABLE_SMART_PUNCTUATION;
        let parser = Parser::new_ext(input, options);
        let mut state = RenderState::new(theme);
        for event in parser {
            state.handle_event(event);
        }
        if !state.current_spans.is_empty() {
            state.flush_line();
        }
        Text::from(state.lines)
    }
    ```
  - 原因: `MarkdownTheme` trait 将 render_state.rs 中 19 处硬编码的 `theme::XXX` 替换为 trait 方法调用。`DefaultMarkdownTheme` 色值与现有 DarkTheme 常量一一对应。`parse_markdown` 新增 `theme` 参数。

- [x] 创建 `perihelion-widgets/src/markdown/render_state.rs`
  - 位置: markdown/ 子目录
  - 内容: 从 `rust-agent-tui/src/ui/markdown/render_state.rs` 复制并修改：
    - 删除 `use super::super::theme;`
    - 删除 `use ratatui::{style::{Color, Modifier, Style}, text::{Line, Span}};` 中的 `Color`（改为通过 theme 获取）
    - `RenderState` 结构体新增 `theme: &'a dyn MarkdownTheme` 字段（改用生命周期 `RenderState<'a>`）
    - 构造函数 `RenderState::new(theme: &'a dyn MarkdownTheme) -> Self`
    - 所有 `theme::WARNING` → `self.theme.heading()`
    - 所有 `theme::MUTED` → `self.theme.muted()`（border, separator, code tag, quote prefix）
    - 所有 `theme::TEXT` → `self.theme.text()`（list bullet, code content）
    - 所有 `theme::SAGE` → `self.theme.link()` 或 `self.theme.code_prefix()`（根据上下文）
    - 具体替换映射（按 render_state.rs 行号）：
      - L132 `theme::MUTED` (make_border) → `self.theme.muted()`
      - L142 `theme::MUTED` (make_data_line 边框) → `self.theme.muted()`
      - L217 `theme::MUTED` (quote prefix) → `self.theme.quote_prefix()`
      - L239-243 `theme::WARNING` (heading H1-H3) → `self.theme.heading()`；`theme::MUTED` (H4+) → `self.theme.muted()`
      - L273 `theme::MUTED` (code block lang tag) → `self.theme.muted()`
      - L315 `theme::TEXT` (list bullet) → `self.theme.list_bullet()`
      - L337 `theme::MUTED` (horizontal rule) → `self.theme.separator()`
      - L353 `theme::SAGE` (code prefix │) → `self.theme.code_prefix()`
      - L357 `theme::TEXT` (code content) → `self.theme.text()`
      - L374 `theme::WARNING` (inline code) → `self.theme.code()`
      - L408 `theme::SAGE` (link) → `self.theme.link()`
    - `make_border()` 和 `make_data_line()` 函数需接收 `&dyn MarkdownTheme` 参数（改为方法或在调用处传 theme）
  - 原因: 颜色参数化是架构评审的核心修正——widget crate 不应硬编码项目特有的主题常量

- [x] 更新 `perihelion-widgets/src/lib.rs`
  - 在 `pub mod form;` 行后追加:
    ```rust
    #[cfg(feature = "markdown")]
    pub mod markdown;
    ```
  - 在重导出区域追加:
    ```rust
    #[cfg(feature = "markdown")]
    pub use markdown::{MarkdownTheme, DefaultMarkdownTheme};
    ```
  - 原因: markdown 组件通过 feature gate 按需启用

- [x] 确认 `perihelion-widgets/Cargo.toml` 的 pulldown-cmark 配置
  - 验证 `[features]` 中存在 `markdown = ["pulldown-cmark"]`
  - 验证 `[dependencies]` 中 `pulldown-cmark = { version = "0.12", optional = true }`

- [x] 为 MarkdownRenderer 编写单元测试
  - 测试文件: `perihelion-widgets/src/markdown/mod.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `parse_empty_input`: `parse_markdown("", &DefaultMarkdownTheme)` → 返回空 Text
    - `parse_heading`: `parse_markdown("# Hello", &DefaultMarkdownTheme)` → 返回 1 行，包含 "Hello"，span style 含 BOLD + heading color
    - `parse_code_block`: `parse_markdown("```rust\nfn main() {}\n```", &DefaultMarkdownTheme)` → 返回包含 `[rust]` 标签行和 `│ fn main() {}` 代码行
    - `parse_inline_code`: `parse_markdown("`hello`", &DefaultMarkdownTheme)` → span style 含 code color
    - `parse_bold_italic`: `parse_markdown("**bold** *italic*", &DefaultMarkdownTheme)` → 验证 BOLD/ITALIC modifier
    - `parse_link`: `parse_markdown("[text](url)", &DefaultMarkdownTheme)` → span style 含 link color + UNDERLINED
    - `parse_unordered_list`: `parse_markdown("- item1\n- item2", &DefaultMarkdownTheme)` → 2 行，包含 `• ` 前缀
    - `parse_ordered_list`: `parse_markdown("1. first\n2. second", &DefaultMarkdownTheme)` → 2 行，包含 `1. ` / `2. ` 前缀
    - `parse_blockquote`: `parse_markdown("> quoted", &DefaultMarkdownTheme)` → 包含 `▍ ` 前缀
    - `parse_horizontal_rule`: `parse_markdown("---", &DefaultMarkdownTheme)` → 包含 `─` 重复
    - `parse_table`: `parse_markdown("| H1 | H2 |\n| --- | --- |\n| A | B |", &DefaultMarkdownTheme)` → 包含 box-drawing 边框行
  - 运行命令: `cargo test -p perihelion-widgets --features markdown -- markdown::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] markdown feature 编译通过
  - `cargo build -p perihelion-widgets --features markdown 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] DefaultMarkdownTheme 正确重导出
  - `grep "MarkdownTheme" perihelion-widgets/src/lib.rs`
  - 预期: 输出包含 `pub use markdown::{MarkdownTheme, DefaultMarkdownTheme}`
- [x] 不带 markdown feature 时不编译 markdown 模块
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 编译成功，不包含 pulldown-cmark 相关代码
- [x] 单元测试通过
  - `cargo test -p perihelion-widgets --features markdown -- markdown::tests 2>&1 | grep "test result"`
  - 预期: 所有测试通过

---

