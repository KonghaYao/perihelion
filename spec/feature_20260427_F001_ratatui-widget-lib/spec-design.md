# Feature: 20260427_F001 - ratatui-widget-lib

## 需求背景

当前 `rust-agent-tui` 的 UI 代码中存在大量重复模式：

- **BorderedPanel**（Clear + Block + borders）在 8+ 处重复实现
- **ScrollableArea**（内容 + 滚动条 + offset 管理）在 6+ 处重复实现
- **SelectableList**（光标高亮 + 前缀标记 + 滚动联动）在 5+ 处重复实现
- **InputField**（聚焦高亮 + 光标字符 + 密码遮罩）在 4+ 处重复实现
- **TabBar** 标签导航在 2 处重复
- 每个面板/弹窗都独立实现光标管理、滚动联动、字段导航等相同逻辑

这些重复代码增加了维护负担，修改一个模式需要同时修改多处。需要将这些通用 UI 模式抽取为独立的可复用组件库。

## 目标

- 将 TUI 渲染层重复代码抽取为独立、可复用的 ratatui widget crate
- 提供 ratatui 原生风格的 StatefulWidget trait API，与 ratatui 生态无缝融合
- 纯通用库，不依赖项目业务逻辑，可独立发布到 crates.io
- 全量抽取 11 个组件，一步到位替换 `rust-agent-tui` 中的重复代码

## 方案设计

### Crate 架构

新增 workspace crate `perihelion-widgets`，位于 workspace 根目录，与 `rust-create-agent` 同级。

**Workspace 依赖关系（变更后）：**

```
perihelion-widgets             ← 零内部依赖，仅依赖 ratatui + pulldown-cmark
    ↑
rust-agent-tui                 ← 新增依赖 perihelion-widgets
    （其他 crate 不受影响）
```

**外部依赖：**
- `ratatui ≥0.30`（必须，widget trait 定义）
- `pulldown-cmark 0.12`（Markdown 渲染组件使用）
- `unicode-width 0.2`（InputField 光标位置计算）
- 无其他外部依赖

**目录结构：**

```
perihelion-widgets/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # 重导出所有公共 API
│   ├── bordered_panel.rs       # BorderedPanel widget
│   ├── scrollable.rs           # ScrollableArea widget + ScrollState
│   ├── list.rs                 # SelectableList widget + ListState<T>
│   ├── input_field.rs          # InputField widget + InputState
│   ├── tab_bar.rs              # TabBar widget + TabState
│   ├── radio_group.rs          # RadioGroup widget + RadioState
│   ├── checkbox_group.rs       # CheckboxGroup widget + CheckboxState
│   ├── form.rs                 # FormState<Field> 状态管理
│   ├── markdown/
│   │   ├── mod.rs              # MarkdownRenderer 公共接口
│   │   └── render_state.rs     # Markdown 渲染状态机（从现有代码迁移）
│   └── theme/
│       ├── mod.rs              # Theme trait + 默认实现
│       └── presets.rs          # 预设配色方案（DarkTheme）
```

### 组件 API 设计

所有组件遵循 ratatui 原生风格：StatefulWidget trait 或直接 render 方法。

#### 1. BorderedPanel — 带边框容器

最基础的容器组件，封装 `Clear + Block + borders` 模式。无状态，直接 render。

```rust
pub struct BorderedPanel<'a> {
    title: Line<'a>,
    border_style: Style,
}

impl<'a> BorderedPanel<'a> {
    pub fn new(title: impl Into<Line<'a>>) -> Self;
    pub fn border_style(mut self, style: Style) -> Self;

    /// 渲染边框面板，返回 inner area 供后续渲染
    pub fn render(self, f: &mut Frame, area: Rect) -> Rect;
}
```

**使用示例：**
```rust
let inner = BorderedPanel::new(" Model Config ")
    .border_style(Style::default().fg(Color::Cyan))
    .render(f, panel_area);
// 在 inner 中继续渲染其他组件
```

#### 2. ScrollableArea — 可滚动区域

内容 + 滚动条的封装，管理 scroll offset。

```rust
pub struct ScrollState {
    offset: u16,
}

impl ScrollState {
    pub fn new() -> Self;
    pub fn offset(&self) -> u16;
    pub fn scroll_up(&mut self, delta: u16);
    pub fn scroll_down(&mut self, delta: u16, content_height: u16, visible_height: u16);
    pub fn ensure_visible(&mut self, row: u16, visible_height: u16);
    pub fn reset(&mut self);
}

pub struct ScrollableArea<'a> {
    content: Text<'a>,
    show_scrollbar: bool,
    scrollbar_style: Style,
}

impl<'a> ScrollableArea<'a> {
    pub fn new(content: Text<'a>) -> Self;
    pub fn show_scrollbar(mut self, show: bool) -> Self;
    pub fn scrollbar_style(mut self, style: Style) -> Self;
    pub fn render(self, f: &mut Frame, area: Rect, state: &mut ScrollState);
}
```

#### 3. SelectableList — 可选择列表

带光标高亮和滚动联动的列表组件。泛型设计，渲染方式通过闭包自定义。

```rust
pub struct ListState<T> {
    items: Vec<T>,
    cursor: usize,
    scroll: ScrollState,
}

impl<T> ListState<T> {
    pub fn new(items: Vec<T>) -> Self;
    pub fn items(&self) -> &[T];
    pub fn cursor(&self) -> usize;
    pub fn move_cursor(&mut self, delta: i32);
    pub fn clamp_cursor(&mut self);             // cursor 不超过 items.len()
    pub fn selected(&self) -> Option<&T>;
    pub fn selected_mut(&mut self) -> Option<&mut T>;
    pub fn ensure_visible(&mut self, visible: u16);
    pub fn set_items(&mut self, items: Vec<T>); // 刷新列表时保持 cursor 合法
}

pub struct SelectableList<'a, T> {
    render_item: Box<dyn Fn(&T, bool) -> Line<'a>>, // bool = is_cursor
    cursor_marker: &'a str,
    cursor_style: Style,
    normal_style: Style,
}

impl<'a, T> SelectableList<'a, T> {
    pub fn new(render_item: impl Fn(&T, bool) -> Line<'a> + 'a) -> Self;
    pub fn cursor_marker(mut self, marker: &'a str) -> Self;
    pub fn cursor_style(mut self, style: Style) -> Self;
    pub fn normal_style(mut self, style: Style) -> Self;
}

impl<T> StatefulWidget for SelectableList<'_, T> {
    type State = ListState<T>;
}
```

**支持 "特殊首项" 模式：** 调用方在 `items[0]` 放特殊项（如 "New Thread"、"No Agent"），渲染闭包中根据内容区分样式。

#### 4. InputField — 文本输入框

带光标显示和可选密码遮罩的输入字段。

```rust
pub struct InputState {
    buffer: String,
    cursor: usize,
    masked: bool,
}

impl InputState {
    pub fn new() -> Self;
    pub fn with_value(value: String) -> Self;
    pub fn masked(mut self, masked: bool) -> Self;
    pub fn value(&self) -> &str;
    pub fn set_value(&mut self, value: String);
    pub fn cursor(&self) -> usize;
    pub fn insert(&mut self, c: char);
    pub fn backspace(&mut self);
    pub fn delete(&mut self);
    pub fn cursor_left(&mut self);
    pub fn cursor_right(&mut self);
    pub fn cursor_home(&mut self);
    pub fn cursor_end(&mut self);
    pub fn paste(&mut self, text: &str);
}

pub struct InputFieldStyle {
    pub label_focused: Style,
    pub label_unfocused: Style,
    pub value_focused: Style,
    pub value_unfocused: Style,
    pub cursor_char: char,
    pub mask_char: char,
}

impl Default for InputFieldStyle { /* ... */ }

pub struct InputField<'a> {
    label: &'a str,
    focused: bool,
    style: InputFieldStyle,
}

impl<'a> InputField<'a> {
    pub fn new(label: &'a str) -> Self;
    pub fn focused(mut self, focused: bool) -> Self;
    pub fn style(mut self, style: InputFieldStyle) -> Self;
    pub fn render(&self, f: &mut Frame, area: Rect, state: &InputState);
}
```

#### 5. TabBar — 标签导航栏

带活跃高亮和可选完成标记的标签栏。

```rust
pub struct TabState {
    active: usize,
    labels: Vec<String>,
    indicators: Vec<Option<char>>,   // 如 Some('✓') 表示已完成
}

impl TabState {
    pub fn new(labels: Vec<String>) -> Self;
    pub fn active(&self) -> usize;
    pub fn next(&mut self);
    pub fn prev(&mut self);
    pub fn set_indicator(&mut self, index: usize, indicator: Option<char>);
    pub fn label(&self, index: usize) -> &str;
}

pub struct TabStyle {
    pub active: Style,
    pub inactive: Style,
    pub separator: &'static str,
}

impl Default for TabStyle { /* ... */ }

pub struct TabBar<'a> {
    style: TabStyle,
    _marker: PhantomData<&'a ()>,
}

impl TabBar<'_> {
    pub fn new() -> Self;
    pub fn style(mut self, style: TabStyle) -> Self;
}

impl StatefulWidget for TabBar<'_> {
    type State = TabState;
}
```

#### 6. RadioGroup — 单选按钮组

```rust
pub struct RadioState {
    selected: Option<usize>,
    cursor: usize,
}

impl RadioState {
    pub fn new() -> Self;
    pub fn select(&mut self, index: usize);
    pub fn selected(&self) -> Option<usize>;
    pub fn move_cursor(&mut self, delta: i32, total: usize);
    pub fn cursor(&self) -> usize;
}

pub struct RadioOption<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
}

pub struct RadioGroup<'a> {
    options: Vec<RadioOption<'a>>,
    marker_char: char,          // '◉' / '○'
    cursor_style: Style,
    selected_style: Style,
}

impl<'a> RadioGroup<'a> {
    pub fn new(options: Vec<RadioOption<'a>>) -> Self;
    pub fn marker_char(mut self, c: char) -> Self;
    pub fn cursor_style(mut self, style: Style) -> Self;
}

impl StatefulWidget for RadioGroup<'_> {
    type State = RadioState;
}
```

#### 7. CheckboxGroup — 多选按钮组

```rust
pub struct CheckboxState {
    checked: Vec<bool>,
    cursor: usize,
}

impl CheckboxState {
    pub fn new(count: usize) -> Self;
    pub fn toggle(&mut self);
    pub fn select_all(&mut self);
    pub fn select_none(&mut self);
    pub fn move_cursor(&mut self, delta: i32);
    pub fn is_checked(&self, index: usize) -> bool;
    pub fn checked_indices(&self) -> Vec<usize>;
    pub fn cursor(&self) -> usize;
}

pub struct CheckboxGroup<'a> {
    labels: Vec<&'a str>,
    checked_char: char,         // '☑' / '☐'
    cursor_style: Style,
    summary_template: Option<String>,  // 如 "已选: {approved} 批准 / {rejected} 拒绝"
}

impl<'a> CheckboxGroup<'a> {
    pub fn new(labels: Vec<&'a str>) -> Self;
    pub fn checked_char(mut self, c: char) -> Self;
    pub fn cursor_style(mut self, style: Style) -> Self;
}

impl StatefulWidget for CheckboxGroup<'_> {
    type State = CheckboxState;
}
```

#### 8. FormState\<Field\> — 表单字段管理

泛型表单状态，管理字段间导航和每个字段的 InputState。

```rust
/// 由使用方实现的字段枚举 trait
pub trait FormField: Copy + Eq + Hash {
    fn next(self) -> Self;
    fn prev(self) -> Self;
    fn label(self) -> &'static str;
}

pub struct FormState<F: FormField> {
    active: F,
    fields: HashMap<F, InputState>,
}

impl<F: FormField> FormState<F> {
    pub fn new(fields: impl Iterator<Item = F>) -> Self;
    pub fn next_field(&mut self);
    pub fn prev_field(&mut self);
    pub fn active_field(&self) -> F;
    pub fn input(&self, field: F) -> &InputState;
    pub fn input_mut(&mut self, field: F) -> &mut InputState;
    pub fn handle_char(&mut self, c: char);
    pub fn handle_backspace(&mut self);
    pub fn handle_delete(&mut self);
    pub fn handle_cursor_left/right/home/end(&mut self);
    pub fn handle_paste(&mut self, text: &str);
}
```

#### 9-10. MarkdownRenderer

从现有 `rust-agent-tui/src/ui/markdown/` 迁移，保持公共 API 不变：

```rust
pub fn parse_markdown(input: &str) -> Text<'static>;
pub fn ensure_rendered(block: &mut ContentBlockView);
```

内部迁移 `render_state.rs` 的 Markdown 渲染状态机（事件驱动，pulldown-cmark → ratatui Spans）。需要将 `ContentBlockView` 类型也迁移或通过泛型解耦。

#### 11. Theme trait — 主题抽象

```rust
pub trait Theme: Clone + Send + Sync + 'static {
    // 基础色
    fn accent(&self) -> Color;
    fn text(&self) -> Color;
    fn muted(&self) -> Color;
    fn warning(&self) -> Color;
    fn success(&self) -> Color;
    fn error(&self) -> Color;

    // UI 组件色
    fn border(&self) -> Color;
    fn cursor_bg(&self) -> Color;
    fn selected_bg(&self) -> Color;
    fn input_bg(&self) -> Color;

    // 工具分级色（可选）
    fn tool_danger(&self) -> Color { self.warning() }
    fn tool_readonly(&self) -> Color { self.muted() }
    fn tool_shell(&self) -> Color { self.accent() }
}

/// 预设深色主题
#[derive(Clone)]
pub struct DarkTheme { /* 内部色值 */ }
impl Theme for DarkTheme { /* ... */ }

/// 组件可接受 Theme 泛型或 &dyn Theme
pub trait Themed {
    fn theme(&self) -> &dyn Theme;
}
```

### 迁移计划

从 `rust-agent-tui` 迁移到 `perihelion-widgets` 的代码映射：

| 现有代码 | 迁移目标 | 说明 |
|----------|----------|------|
| `ui/theme.rs` 的常量 | `theme/presets.rs` DarkTheme 实现 | 常量 → Theme trait 方法 |
| `ui/markdown/*` | `markdown/*` | 整体迁移，公共 API 不变 |
| `app/mod.rs:ensure_cursor_visible()` | `scrollable.rs ScrollState::ensure_visible()` | 提取为 ScrollState 方法 |
| 各面板的 Clear+Block 模式 | `bordered_panel.rs` | 8+ 处替换 |
| agent_panel / thread_browser 的列表 | `list.rs ListState<T>` | 5+ 处替换 |
| model_panel / relay_panel 的 InputField | `input_field.rs InputState` | 4+ 处替换 |
| ask_user 的 RadioGroup | `radio_group.rs` | 1 处替换 |
| hitl 的 CheckboxGroup | `checkbox_group.rs` | 1 处替换 |
| model_panel / ask_user 的 TabBar | `tab_bar.rs` | 2 处替换 |
| model_panel / relay_panel 的 EditField + buffer | `form.rs FormState<F>` | 2 处替换 |

## 实现要点

1. **Lifetime 管理**：`SelectableList` 的 `render_item` 闭包需要 `+'a` 约束，使用方在 render 前构造闭包，避免持有临时引用。`BorderedPanel` 使用 `Line<'a>` 允许动态标题样式。

2. **ListState 泛型**：`ListState<T>` 不要求 `T: Clone`，仅要求引用。`set_items` 刷新时 clamp cursor。

3. **MarkdownRenderer 迁移**：现有 `ContentBlockView` 类型与 `MessageViewModel` 耦合。迁移时需将渲染逻辑与视图模型解耦：`MarkdownRenderer` 只负责 `&str → Text<'static>` 转换，`ContentBlockView` 的 dirty/lazy 渲染逻辑保留在 TUI 层。

4. **Theme 集成**：组件不硬编码颜色，通过 `Style` 参数传入。`rust-agent-tui` 在调用点从 `theme::XXX` 常量构建 Style。Theme trait 提供统一的颜色查询接口，组件库提供 `DarkTheme` 默认实现。

5. **ScrollState 复用**：`ListState<T>` 内嵌 `ScrollState`，滚动与光标联动通过 `ensure_visible` 自动处理。`ScrollableArea` 也使用同一个 `ScrollState`。

6. **测试**：每个组件提供独立的单元测试（使用 ratatui `TestBackend`），不依赖 TUI 应用状态。迁移后 headless 测试模式不受影响。

## 约束一致性

- **Workspace 多 crate 分层**：`perihelion-widgets` 位于 `rust-create-agent` 同级，不违反"禁止下层依赖上层"约束。只有 `rust-agent-tui` 新增依赖 `perihelion-widgets`。
- **技术栈一致**：使用 `ratatui ≥0.30` + `pulldown-cmark 0.12`，与现有技术栈完全一致。
- **编码规范一致**：Rust 标准命名、`thiserror` 定义错误、`#[cfg(test)] mod tests`。
- **无新增架构偏离**。

## 验收标准

- [ ] `perihelion-widgets` crate 可独立编译（`cargo build -p perihelion-widgets`）
- [ ] 11 个组件全部实现并通过单元测试
- [ ] `rust-agent-tui` 中 BorderedPanel 模式的 8+ 处全部替换为 `BorderedPanel::new().render()`
- [ ] `rust-agent-tui` 中列表管理的 5+ 处全部替换为 `ListState<T>`
- [ ] `rust-agent-tui` 中输入字段的 4+ 处全部替换为 `InputState`
- [ ] MarkdownRenderer 从 TUI 迁移到 widget crate，公共 API 不变
- [ ] `cargo test` 全量通过（包括现有 headless 测试）
- [ ] Theme trait 抽象完成，现有 `theme.rs` 常量迁移为 `DarkTheme` 实现
- [ ] 组件库不依赖任何项目业务逻辑 crate
