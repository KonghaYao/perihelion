# 信息显示 Widget 化升级 执行计划

**目标:** 在 perihelion-widgets 中新增 SpinnerWidget、ToolCallWidget、MessageBlockWidget 三个核心 widget，对标 Claude Code 的信息显示体验

**技术栈:** Rust 2021 + ratatui ≥0.30 + pulldown-cmark 0.12 + rand 0.8（新增）

**设计文档:** spec/feature_20260427_F001_claude-code-info-display/spec-design.md

## 改动总览

- 本次改动在 `perihelion-widgets` crate 内新增 `spinner/`、`tool_call/`、`message_block/` 三个模块（9 个新文件），并修改 `rust-agent-tui` 的 `status_bar.rs`、`message_render.rs`、`message_view.rs`、`app/mod.rs`、`app/events.rs` 完成集成
- Task 1→2 实现 SpinnerWidget（基础→TUI 集成），Task 3 实现 ToolCallWidget，Task 4→5 实现 MessageBlockWidget（基础→TUI 集成）。Task 2 依赖 Task 1，Task 5 依赖 Task 3 和 Task 4
- 经代码分析确认：`perihelion-widgets` 已有 `markdown` feature 和 `MarkdownTheme` trait（`perihelion-widgets/src/markdown/mod.rs`），MessageBlockWidget 在其基础上扩展 diff 着色和代码高亮；`message_view.rs` 已有 `MessageViewModel::ToolBlock` 折叠逻辑和 `tool_color()` 颜色分级函数，ToolCallWidget 替换其渲染部分

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 workspace 构建可用
  - 运行 `cargo build -p perihelion-widgets` 确认 widget crate 编译通过
- [x] 验证测试工具可用
  - 运行 `cargo test -p perihelion-widgets --lib` 确认测试框架可用

**检查步骤:**
- [x] widget crate 构建成功
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] widget crate 测试框架可用
  - `cargo test -p perihelion-widgets --lib 2>&1 | tail -5`
  - 预期: 测试运行完成，无配置错误

---

### Task 1: SpinnerWidget 基础模块

**背景:**
用户在等待 Agent 响应时，需要看到动态的加载指示。当前 `status_bar.rs` 只有静态文字 `⠿ 运行中`，没有动词提示和动画效果。本 Task 在 `perihelion-widgets` 中实现 SpinnerWidget 的核心逻辑（SpinnerMode、SpinnerState、动词管理、动画帧计算），Task 2 负责将其集成到 TUI。

**涉及文件:**
- 新建: `perihelion-widgets/src/spinner/mod.rs`
- 新建: `perihelion-widgets/src/spinner/verb.rs`
- 新建: `perihelion-widgets/src/spinner/animation.rs`
- 修改: `perihelion-widgets/src/lib.rs`
- 修改: `perihelion-widgets/Cargo.toml`

**执行步骤:**
- [x] 在 `perihelion-widgets/Cargo.toml` 添加 `rand = "0.8"` 依赖
  - 位置: `[dependencies]` 段末尾
  - 原因: 动词随机选取需要 rand

- [x] 创建 `perihelion-widgets/src/spinner/animation.rs`
  - 定义 `BRAILLE_FRAMES: &[char]` 常量：`['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']`
  - 实现 `pub fn tick_to_frame(tick: u64) -> char`：`BRAILLE_FRAMES[(tick as usize) % BRAILLE_FRAMES.len()]`
  - 实现 `pub fn smooth_increment(displayed: usize, target: usize) -> usize`：
    ```
    if displayed >= target { return target; }
    let gap = target - displayed;
    let step = if gap < 70 { 3 } else if gap < 200 { (gap * 15 / 100).max(8) } else { 50 };
    (displayed + step).min(target)
    ```
  - 实现 `pub fn format_elapsed(elapsed_ms: u64) -> String`：
    ```
    let secs = elapsed_ms / 1000;
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
    ```

- [x] 创建 `perihelion-widgets/src/spinner/verb.rs`
  - 定义 `pub const DEFAULT_VERBS: &[&str]`：`["处理中", "分析中", "思考中", "生成中", "搜索中", "读取中", "编写中", "执行中", "计算中"]`
  - 实现 `pub fn pick_verb(active_form: Option<&str>) -> String`：
    ```
    active_form.map(|s| format!("{}…", s)).unwrap_or_else(|| {
        DEFAULT_VERBS[rand::random::<usize>() % DEFAULT_VERBS.len()].to_string()
    })
    ```

- [x] 创建 `perihelion-widgets/src/spinner/mod.rs`
  - 声明 `pub mod animation; pub mod verb;`
  - 定义 `pub enum SpinnerMode { Thinking, ToolUse, Responding, Idle }`
  - 定义 `pub struct SpinnerState` 字段：
    ```
    mode: SpinnerMode,
    verb: String,
    start_time: std::time::Instant,
    token_count: usize,
    displayed_tokens: usize,
    tick: u64,
    ```
  - 实现 SpinnerState 方法：
    - `pub fn new(mode: SpinnerMode) -> Self` — verb 从 `verb::pick_verb(None)` 初始化
    - `pub fn set_mode(&mut self, mode: SpinnerMode)` — 切换模式
    - `pub fn set_verb(&mut self, active_form: Option<&str>)` — 调用 `verb::pick_verb(active_form)`
    - `pub fn set_token_count(&mut self, count: usize)` — 更新目标 token 数
    - `pub fn advance_tick(&mut self)` — tick += 1, displayed_tokens = animation::smooth_increment(displayed_tokens, token_count)
    - `pub fn elapsed_ms(&self) -> u64` — self.start_time.elapsed().as_millis() as u64
  - 定义 `pub struct SpinnerWidget<'a>` 字段：`state: &'a SpinnerState, show_elapsed: bool, show_tokens: bool`
  - 实现 `impl<'a> SpinnerWidget<'a>` 的 `pub fn render(&self, area: Rect, buf: &mut Buffer)`：
    - 构建一行 `Line`：
      - 第一段：`animation::tick_to_frame(state.tick)` + 空格
      - 第二段：`state.verb`（主文字颜色）
      - 如果 `show_elapsed && elapsed > 30_000`：追加 `format_elapsed(elapsed_ms)`
      - 如果 `show_tokens && state.displayed_tokens > 0`：追加 `format!("{} tokens", state.displayed_tokens)`
    - 渲染为 `Paragraph::new(Line::from(spans))`

- [x] 在 `perihelion-widgets/src/lib.rs` 注册 spinner 模块
  - 位置: `pub mod theme;` 之后 (~L10)
  - 添加: `pub mod spinner;`
  - 重导出: `pub use spinner::{SpinnerMode, SpinnerState, SpinnerWidget};`

- [x] 为 SpinnerWidget 核心逻辑编写单元测试
  - 测试文件: `perihelion-widgets/src/spinner/mod.rs` 内 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_tick_to_frame_cycle`: 调用 `animation::tick_to_frame` 20 次，验证返回的字符都在 BRAILLE_FRAMES 中
    - `test_smooth_increment_convergence`: 设置 target=100，连续调用 `smooth_increment(0, 100)` 直到收敛，验证最终达到 100
    - `test_pick_verb_with_active_form`: 传入 `Some("搜索文件")`，验证结果包含 "搜索文件…"
    - `test_pick_verb_random`: 传入 `None`，验证结果非空且以 "中" 结尾
    - `test_format_elapsed`: 验证 `format_elapsed(90_000)` == "1:30"
  - 运行命令: `cargo test -p perihelion-widgets --lib -- spinner`
  - 预期: 所有测试通过

**检查步骤:**
- [x] spinner 模块编译通过
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: "Finished" 且无 error
- [x] spinner 模块导出正确
  - `grep -c "pub use spinner" perihelion-widgets/src/lib.rs`
  - 预期: 输出 1
- [x] 单元测试通过
  - `cargo test -p perihelion-widgets --lib -- spinner 2>&1 | tail -5`
  - 预期: "5 passed" 且无 failed

---

### Task 2: SpinnerWidget TUI 集成

**背景:**
Task 1 已实现 SpinnerWidget 组件，本 Task 将其集成到 `rust-agent-tui` 的状态栏中，替换当前静态的 `⠿ 运行中` 文字。用户在 Agent 运行时将看到动态动画 + 动词提示。本 Task 依赖 Task 1 的 SpinnerWidget 输出。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/events.rs`
- 修改: `rust-agent-tui/Cargo.toml`（确认 perihelion-widgets 依赖）

**执行步骤:**
- [x] 在 `rust-agent-tui/src/ui/main_ui/status_bar.rs` 中引入 SpinnerWidget
  - 位置: 文件顶部 use 语句区域 (~L1-L10)
  - 添加: `use perihelion_widgets::{SpinnerMode, SpinnerState, SpinnerWidget};`

- [x] 在 `App` 结构体中添加 `spinner_state: SpinnerState` 字段
  - 位置: `rust-agent-tui/src/app/mod.rs` 中 `App` 结构体的 `mode_highlight_until` 字段之后（~L83）
  - 添加: `pub spinner_state: perihelion_widgets::SpinnerState`
  - 初始化: `rust-agent-tui/src/app/mod.rs` 的 `App::new()` 方法中，`Self { ... }` 构造体末尾（`mode_highlight_until: None,` 之后）
  - 添加: `spinner_state: perihelion_widgets::SpinnerState::new(perihelion_widgets::SpinnerMode::Idle),`

- [x] 修改 `render_second_row` 函数，在计时器之前添加 Spinner 动画显示
  - 位置: `status_bar.rs` 中 `render_second_row` 函数，`left_spans` 声明之后、计时器逻辑之前（~L70-L73）
  - 经代码确认：当前 `render_second_row` 没有独立的加载状态指示器（加载状态仅通过计时器颜色和 textarea 标题 "处理中…" 体现），需新增 Spinner 行内显示
  - 在 `has_content` 声明之前插入：
    ```
    // Spinner 动画（仅 loading 时显示）
    if app.core.loading {
        let frame = perihelion_widgets::spinner::animation::tick_to_frame(app.spinner_state.tick());
        let verb = app.spinner_state.verb();
        left_spans.push(Span::styled(
            format!(" {} ", frame),
            Style::default().fg(theme::LOADING),
        ));
        left_spans.push(Span::styled(
            verb.to_string(),
            Style::default().fg(theme::LOADING),
        ));
        has_content = true;
    }
    ```
  - 原因: 在状态栏第二行左侧添加动态动画 + 动词提示，替换仅依赖 textarea 标题的静态反馈

- [x] 在事件循环中驱动 Spinner tick
  - 位置: `rust-agent-tui/src/app/events.rs` 的事件处理循环中，TerminalEvent::Render 分支或每帧渲染前
  - 在每帧渲染前调用 `app.spinner_state.advance_tick()`
  - 当 Agent 开始运行时（`app.core.loading` 从 false→true）：调用 `app.spinner_state.set_mode(SpinnerMode::Responding)`
  - 当 Agent 完成时（`app.core.loading` 从 true→false）：调用 `app.spinner_state.set_mode(SpinnerMode::Idle)`

- [x] 为 Spinner 集成编写 headless 测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（追加测试函数）
  - 测试场景:
    - `test_spinner_shows_verb_in_status_bar`: 设置 `app.spinner_state.set_verb(Some("搜索代码"))`, 设置 `app.core.loading = true`, 渲染后检查 handle 包含 "搜索代码"
  - 运行命令: `cargo test -p rust-agent-tui --lib -- test_spinner`
  - 预期: 测试通过

**检查步骤:**
- [x] TUI 构建通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: "Finished" 且无 error
- [x] headless 测试通过
  - `cargo test -p rust-agent-tui --lib -- test_spinner 2>&1 | tail -5`
  - 预期: "1 passed"

---

### Task 3: ToolCallWidget

**背景:**
当前 `message_render.rs` 中 `MessageViewModel::ToolBlock` 的渲染已有基础折叠（`▸`/`▾` 箭头 + 结果内容），但缺少状态指示器（运行中闪烁/完成静态）和智能折叠策略。本 Task 在 `perihelion-widgets` 中实现独立的 ToolCallWidget，后续 Task 5 替换 `message_render.rs` 中的渲染逻辑。

**涉及文件:**
- 新建: `perihelion-widgets/src/tool_call/mod.rs`
- 新建: `perihelion-widgets/src/tool_call/display.rs`
- 新建: `perihelion-widgets/src/tool_call/collapse.rs`
- 修改: `perihelion-widgets/src/lib.rs`

**执行步骤:**
- [x] 创建 `perihelion-widgets/src/tool_call/collapse.rs`
  - 定义 `pub const READ_ONLY_TOOLS: &[&str]`：`["read_file", "glob_files", "search_files_rg", "ask_user_question"]`
  - 定义 `pub const MAX_RESULT_LINES: usize = 20`
  - 实现 `pub fn should_collapse_by_default(tool_name: &str) -> bool`：READ_ONLY_TOOLS.contains(tool_name)
  - 实现 `pub fn truncate_result(lines: &[String], max: usize) -> (Vec<String>, Option<usize>)`：
    ```
    if lines.len() <= max { return (lines.to_vec(), None); }
    (lines[..max].to_vec(), Some(lines.len() - max))
    ```

- [x] 创建 `perihelion-widgets/src/tool_call/display.rs`
  - 实现 `pub fn format_indicator(status: ToolCallStatus, tick: u64) -> &'static str`：
    ```
    match status {
        ToolCallStatus::Pending => "●",  // 暗淡由调用方通过 Style 控制
        ToolCallStatus::Running => if (tick / 4) % 2 == 0 { "●" } else { " " },  // 闪烁
        ToolCallStatus::Completed => "●",
        ToolCallStatus::Failed => "✗",
    }
    ```
  - 实现 `pub fn format_args_summary(args: &str, max_width: usize) -> String`：
    - 截断超过 max_width 的参数，末尾追加 "…"

- [x] 创建 `perihelion-widgets/src/tool_call/mod.rs`
  - 声明 `pub mod collapse; pub mod display;`
  - 定义 `pub enum ToolCallStatus { Pending, Running, Completed, Failed }`
  - 定义 `pub struct ToolCallState` 字段：
    ```
    pub tool_name: String,
    pub args_summary: String,
    pub status: ToolCallStatus,
    pub collapsed: bool,
    pub result_lines: Vec<String>,
    pub is_error: bool,
    pub tick: u64,
    pub color: Color,
    ```
  - 实现 ToolCallState 方法：
    - `pub fn new(tool_name: String, color: Color) -> Self` — collapsed 从 `collapse::should_collapse_by_default` 取默认值
    - `pub fn advance_tick(&mut self)` — tick += 1
    - `pub fn toggle_collapse(&mut self)` — collapsed = !collapsed
    - `pub fn set_result(&mut self, content: String)` — 按 `\n` 分割为 result_lines，调用 truncate_result
  - 定义 `pub struct ToolCallWidget<'a>` 字段：`state: &'a ToolCallState`
  - 实现 `render(&self, area: Rect, buf: &mut Buffer)`：
    - 头行：indicator + 空格 + bold tool_name + args_summary（括号内）
    - 展开 + 有结果时：逐行渲染 result_lines，前缀 `"  │ "`

- [x] 在 `perihelion-widgets/src/lib.rs` 注册 tool_call 模块
  - 位置: `pub mod spinner;` 之后
  - 添加: `pub mod tool_call;`
  - 重导出: `pub use tool_call::{ToolCallState, ToolCallStatus, ToolCallWidget};`

- [x] 为 ToolCallWidget 核心逻辑编写单元测试
  - 测试文件: `perihelion-widgets/src/tool_call/mod.rs` 内 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_should_collapse_read_file`: 调用 `should_collapse_by_default("read_file")` → true
    - `test_should_not_collapse_bash`: 调用 `should_collapse_by_default("bash")` → false
    - `test_truncate_result_short`: 10 行输入，max=20 → 返回 10 行，None
    - `test_truncate_result_long`: 30 行输入，max=20 → 返回 20 行，Some(10)
    - `test_toggle_collapse`: 创建 state（collapsed=true），toggle → collapsed=false
    - `test_indicator_running_blinks`: tick=0 返回 "●", tick=4 返回 " "（闪烁效果）
  - 运行命令: `cargo test -p perihelion-widgets --lib -- tool_call`
  - 预期: 所有测试通过

**检查步骤:**
- [x] tool_call 模块编译通过
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: "Finished" 且无 error
- [x] 单元测试通过
  - `cargo test -p perihelion-widgets --lib -- tool_call 2>&1 | tail -5`
  - 预期: "6 passed"

---

### Task 4: MessageBlockWidget 基础模块

**背景:**
当前 `perihelion-widgets/src/markdown/mod.rs` 已有基础 Markdown 渲染（标题、列表、代码块标签、表格等），但不支持代码语法高亮、diff 着色和内联代码背景色。本 Task 在 `perihelion-widgets` 中实现 MessageBlockWidget，扩展 Markdown 渲染能力，增加 BlockRenderStrategy 枚举来统一管理不同类型的内容块渲染。

**涉及文件:**
- 新建: `perihelion-widgets/src/message_block/mod.rs`
- 新建: `perihelion-widgets/src/message_block/highlight.rs`（代码高亮 + diff 着色）
- 新建: `perihelion-widgets/src/message_block/blocks.rs`（BlockRenderStrategy 渲染策略）
- 修改: `perihelion-widgets/src/lib.rs`
- 修改: `perihelion-widgets/Cargo.toml`（添加 regex 依赖）

**执行步骤:**
- [x] 在 `perihelion-widgets/Cargo.toml` 添加 `regex = "1"` 依赖
  - 位置: `[dependencies]` 段
  - 原因: 代码高亮和 diff 检测需要正则匹配

- [x] 创建 `perihelion-widgets/src/message_block/highlight.rs`
  - 实现 `pub fn highlight_diff_line(line: &str) -> Vec<Span<'static>>`：
    ```
    if line.starts_with("@@ ") → 蓝色 (Color::Cyan)
    else if line.starts_with('+') → 绿色 (Color::Rgb(110, 181, 106))
    else if line.starts_with('-') → 红色 (Color::Rgb(204, 70, 62))
    else → 默认色
    ```
  - 实现 `pub fn is_diff_content(text: &str) -> bool`：
    - 检查前 5 行中是否有 `@@ ` 或 `+++` 行，有则认为 diff 内容
  - 实现 `pub fn highlight_code_line(line: &str, lang: &str) -> Vec<Span<'static>>`：
    - 简单正则匹配：关键字（`fn|let|mut|pub|use|struct|enum|impl|if|else|match|return|for|while|async|await`）用亮黄色，字符串（`"[^"]*"`）用绿色，注释（`//.*$`）用灰色
    - 不做精确 tokenizer，只做行内正则替换

- [x] 创建 `perihelion-widgets/src/message_block/blocks.rs`
  - 定义 `pub enum BlockRenderStrategy`：
    ```
    Text { content: String, streaming: bool },
    ToolCall(ToolCallState),  // 复用 Task 3 的 ToolCallState
    SubAgent { agent_id: String, task_preview: String, total_steps: usize, collapsed: bool, result: Option<String> },
    Thinking { char_count: usize, expanded: bool },
    SystemNote { content: String },
    ```
  - 实现 `pub fn render_block(strategy: &BlockRenderStrategy, width: usize) -> Vec<Line<'static>>`：
    - Text: 调用 `parse_markdown` 渲染，如果是 diff 内容则用 `highlight_diff_line`
    - ToolCall: 委托给 ToolCallWidget 渲染
    - SubAgent: 头行 `▸/▾ 🤖 agent_id` + 折叠逻辑
    - Thinking: `💭 思考 (N chars)`，expanded 时追加完整内容
    - SystemNote: `ℹ content`

- [x] 创建 `perihelion-widgets/src/message_block/mod.rs`
  - 声明 `pub mod blocks; pub mod highlight;`
  - 定义 `pub struct MessageBlockState` 字段：`blocks: Vec<BlockRenderStrategy>`
  - 定义 `pub struct MessageBlockWidget<'a>` 字段：`state: &'a MessageBlockState, index: Option<usize>, width: usize`
  - 实现 `render(&self, area: Rect, buf: &mut Buffer)`：遍历 blocks 调用 `render_block`，每个 block 渲染为一组 Line

- [x] 在 `perihelion-widgets/src/lib.rs` 注册 message_block 模块
  - 位置: `pub mod tool_call;` 之后
  - 添加: `pub mod message_block;`
  - 重导出: `pub use message_block::{BlockRenderStrategy, MessageBlockState, MessageBlockWidget};`

- [x] 为 MessageBlockWidget 编写单元测试
  - 测试文件: `perihelion-widgets/src/message_block/highlight.rs` 内 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_highlight_diff_add`: `"+ added line"` → 首个 span 颜色为绿色
    - `test_highlight_diff_remove`: `"- removed line"` → 首个 span 颜色为红色
    - `test_highlight_diff_hunk`: `"@@ -1,3 +1,4 @@"` → 首个 span 颜色为 Cyan
    - `test_is_diff_true`: 输入含 `"@@ -1,3 +1,4 @@"` 的文本 → 返回 true
    - `test_is_diff_false`: 输入普通代码 → 返回 false
  - 运行命令: `cargo test -p perihelion-widgets --lib -- message_block`
  - 预期: 所有测试通过

**检查步骤:**
- [x] message_block 模块编译通过
  - `cargo build -p perihelion-widgets 2>&1 | tail -3`
  - 预期: "Finished" 且无 error
- [x] 单元测试通过
  - `cargo test -p perihelion-widgets --lib -- message_block 2>&1 | tail -5`
  - 预期: "5 passed"

---

### Task 5: MessageBlockWidget TUI 集成

**背景:**
Task 3 实现了 ToolCallWidget，Task 4 实现了 MessageBlockWidget。本 Task 将 `rust-agent-tui` 中的 `message_render.rs` 和 `message_view.rs` 的渲染逻辑迁移到使用新 widget，完成 TUI 集成。这是最后的功能 Task，完成后用户将看到增强的工具调用显示和 Markdown 渲染。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_render.rs`
- 修改: `rust-agent-tui/src/ui/message_view.rs`

**执行步骤:**
- [x] 在 `message_view.rs` 中引入 ToolCallState 和 ToolCallStatus
  - 位置: 文件顶部 use 语句区域 (~L1-L5)
  - 添加: `use perihelion_widgets::{ToolCallState, ToolCallStatus, tool_call::collapse};`

- [x] 修改 `MessageViewModel::ToolBlock` 的渲染逻辑
  - 位置: `message_render.rs` 中 `MessageViewModel::ToolBlock` 分支 (~L120-L154)
  - 替换当前的渲染代码为使用 ToolCallWidget：
    ```
    // 替换前：手动构建 header_spans + 折叠逻辑
    // 替换后：创建 ToolCallState 并委托渲染
    ```
  - 将 `ToolBlock` 的 `display_name`、`args_display`、`content`、`is_error`、`collapsed`、`color` 映射到 `ToolCallState` 字段
  - 状态映射：
    - `is_error=true` → `ToolCallStatus::Failed`
    - `content.is_empty()` → `ToolCallStatus::Running`
    - 其他 → `ToolCallStatus::Completed`
  - 调用 `ToolCallState::new(tool_name, color)` 并设置各字段，然后用 `ToolCallWidget` 的 render 方法输出 `Vec<Line>`

- [x] 修改 `MessageViewModel::AssistantBubble` 中 Text block 的渲染
  - 位置: `message_render.rs` 中 `ContentBlockView::Text` 分支 (~L48-L75)
  - 在调用 `parse_markdown` 后，检查内容是否为 diff（使用 `perihelion_widgets::message_block::highlight::is_diff_content`）
  - 如果是 diff 内容，使用 `highlight_diff_line` 逐行着色替换

- [x] 修改 `MessageViewModel::AssistantBubble` 中 Reasoning block 的渲染
  - 位置: `message_render.rs` 中 `ContentBlockView::Reasoning` 分支 (~L76-L95)
  - 保持当前行为（`💭 思考 (N chars)` 单行），无需修改（设计文档中思考折叠已在 `message_view.rs` 中通过 `collapsed_set` 管理）

- [x] 修改 `MessageViewModel::SubAgentGroup` 的渲染逻辑
  - 位置: `message_render.rs` 中 `SubAgentGroup` 分支 (~L155-L260)
  - 保持当前折叠/展开逻辑不变，这是已有的正确实现

- [x] 为集成编写 headless 测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（追加测试函数）
  - 测试场景:
    - `test_tool_call_widget_renders_completed`: 创建 ToolBlock view model，渲染后检查包含工具名和 "●"
    - `test_diff_rendering_in_text`: 创建含 diff 内容的 AssistantBubble，渲染后检查绿色和红色 span
  - 运行命令: `cargo test -p rust-agent-tui --lib -- test_tool_call`
  - 预期: 测试通过

**检查步骤:**
- [x] TUI 构建通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: "Finished" 且无 error
- [x] headless 测试通过
  - `cargo test -p rust-agent-tui --lib -- test_tool_call 2>&1 | tail -5`
  - 预期: 测试通过

---

### Task 6: 信息显示 Widget 化升级 验收

**前置条件:**
- 构建命令: `cargo build --workspace`
- 所有前序 Task 的单元测试已通过

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test --workspace 2>&1 | tail -10`
   - 预期: 全部测试通过，无 failed
   - 失败排查: 检查各 Task 的测试输出，定位到具体 Task

2. 验证 SpinnerWidget 基础功能
   - `cargo test -p perihelion-widgets --lib -- spinner 2>&1 | grep "test result"`
   - 预期: "test result: ok" 且 5 个 spinner 测试通过
   - 失败排查: 检查 Task 1 的 animation/verb 模块

3. 验证 ToolCallWidget 基础功能
   - `cargo test -p perihelion-widgets --lib -- tool_call 2>&1 | grep "test result"`
   - 预期: "test result: ok" 且 6 个 tool_call 测试通过
   - 失败排查: 检查 Task 3 的 collapse/display 模块

4. 验证 MessageBlockWidget 基础功能
   - `cargo test -p perihelion-widgets --lib -- message_block 2>&1 | grep "test result"`
   - 预期: "test result: ok" 且 5 个 message_block 测试通过
   - 失败排查: 检查 Task 4 的 highlight 模块

5. 验证 TUI 集成测试
   - `cargo test -p rust-agent-tui --lib -- test_spinner 2>&1 | grep "test result"`
   - `cargo test -p rust-agent-tui --lib -- test_tool_call 2>&1 | grep "test result"`
   - 预期: 所有 TUI headless 测试通过
   - 失败排查: 检查 Task 2 的 status_bar 集成和 Task 5 的 message_render 集成

6. 验证 workspace 整体编译
   - `cargo build --workspace 2>&1 | tail -3`
   - 预期: "Finished" 且无 error
   - 失败排查: 检查各 Task 中跨 crate 依赖是否正确
