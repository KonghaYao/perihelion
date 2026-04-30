# perihelion-widgets 组件库执行计划（3/3）

**目标:** 在 rust-agent-tui 中集成 perihelion-widgets，替换所有重复的 UI 渲染代码

**技术栈:** Rust 2021 edition, ratatui ≥0.30, perihelion-widgets

**设计文档:** spec/feature_20260427_F001_ratatui-widget-lib/spec-design.md

## 改动总览

- 本文件在 rust-agent-tui 中引入 perihelion-widgets 依赖，分三批替换现有重复代码：Task 8 替换 BorderedPanel + ScrollableArea（10 处 Clear+Block + 5 处手动滚动），Task 9 替换 SelectableList/InputField/TabBar/RadioGroup/CheckboxGroup 渲染，Task 10 替换 EditField+buf 为 FormState
- Task 8 是基础（Cargo.toml 依赖 + theme 桥接），Task 9 依赖 Task 8，Task 10 依赖 Task 9
- 关键决策：theme.rs 常量保留不变（保持向后兼容），新增 `use_perihelion_theme()` 辅助函数桥接 DarkTheme；FormState 替换仅涉及状态层，渲染层由 Task 9 的 InputField 组件处理

---

### Task 0: 环境准备

**背景:**
验证 spec-plan-1.md 和 spec-plan-2.md 中的 Task 1-7 已全部完成，perihelion-widgets crate 可独立编译和测试。

**执行步骤:**
- [x] 验证 perihelion-widgets 独立编译
  - `cargo build -p perihelion-widgets`
  - 预期: 编译成功
- [x] 验证 perihelion-widgets 全量测试
  - `cargo test -p perihelion-widgets`
  - 预期: 所有测试通过

**检查步骤:**
- [x] perihelion-widgets 编译无错误
- [x] perihelion-widgets 测试全部通过

---

### Task 8: TUI 集成 — 主题桥接与基础组件替换

**背景:**
在 rust-agent-tui 中引入 perihelion-widgets 依赖，并替换所有 BorderedPanel（Clear+Block+border 模式，10 处）和 ScrollableArea（手动 scroll 管理，5 处）的重复代码。现有 theme.rs 的 15 个颜色常量保留不变（保持向后兼容，避免一次性改太多），新增桥接代码让 TUI 可以使用 DarkTheme。本 Task 是 Task 9 和 Task 10 的前置条件——后续 Task 依赖 Cargo.toml 中已添加的 perihelion-widgets 依赖。

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`（添加 perihelion-widgets 依赖）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/model.rs`（render_model_panel 中 Clear+Block 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/agent.rs`（render_agent_panel 中 Clear+Block 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/relay.rs`（render_relay_panel 中 Clear+Block 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs`（render_thread_browser 中 Clear+Block + scroll 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/cron.rs`（render_cron_panel 中 Clear+Block + scroll 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/hitl.rs`（render_hitl_popup 中 Clear+Block 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/ask_user.rs`（render_ask_user_popup 中 Clear+Block + scroll 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/setup_wizard.rs`（render_setup_wizard 中 Clear+Block 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/hints.rs`（render_command_hint + render_skill_hint 中 Clear+Block 替换）

**执行步骤:**
- [x] 在 `rust-agent-tui/Cargo.toml` 添加 perihelion-widgets 依赖
  - 位置: dependencies 区域末尾（`langfuse-client` 行之后）
  - 添加: `perihelion-widgets = { path = "../perihelion-widgets", features = ["markdown"] }`
  - 原因: 启用 markdown feature 以使用 MarkdownRenderer

- [x] 替换所有 render_*_panel 函数中的 BorderedPanel 模式
  - 位置: 以下 10 个文件中的 `f.render_widget(Clear, ...)` + `Block::default()...` 模式
  - 替换为:
    ```rust
    use perihelion_widgets::BorderedPanel;
    // 旧代码:
    //   f.render_widget(Clear, area);
    //   let block = Block::default().title(...).borders(Borders::ALL).border_style(...);
    //   f.render_widget(&block, area);
    //   let inner = block.inner(area);
    // 新代码:
    //   let inner = BorderedPanel::new(title)
    //       .border_style(border_style)
    //       .render(f, area);
    ```
  - 涉及函数（每个函数做相同模式的替换）:
    - `panels/model.rs:render_model_panel` (~L18-33)
    - `panels/agent.rs:render_agent_panel` (~L19-31)
    - `panels/relay.rs:render_relay_panel` (~L17-28)
    - `panels/thread_browser.rs:render_thread_browser` (~L17-26)
    - `panels/cron.rs:render_cron_panel` (~L16-26)
    - `popups/hitl.rs:render_hitl_popup` (~L19-31)
    - `popups/ask_user.rs:render_ask_user_popup` (~L18-27)
    - `popups/setup_wizard.rs:render_setup_wizard` (~L15-41)
    - `popups/hints.rs:render_command_hint` (~L41-47)
    - `popups/hints.rs:render_skill_hint` (~L103-109)
  - 原因: 10 处 Clear+Block 模式统一为一行 BorderedPanel 调用

- [x] 替换 ScrollableArea 模式（5 处）
  - 位置: 以下函数中的手动 Paragraph+scroll+Scrollbar 渲染
  - 替换为:
    ```rust
    use perihelion_widgets::{ScrollableArea, ScrollState};
    // 旧代码:
    //   let text_area = Rect { width: inner.width.saturating_sub(1), ..inner };
    //   f.render_widget(Paragraph::new(Text::from(lines)).scroll((offset, 0)).wrap(Wrap{trim:false}), text_area);
    //   if total > visible { f.render_stateful_widget(Scrollbar::new(...), inner, &mut state); }
    // 新代码:
    //   ScrollableArea::new(Text::from(lines))
    //       .scrollbar_style(Style::default().fg(theme::MUTED))
    //       .render(f, inner, &mut scroll_state);
    ```
  - 涉及函数:
    - `panels/agent.rs:render_agent_panel` (~L98-114) — 列表滚动
    - `panels/thread_browser.rs:render_thread_browser` (~L68-82) — 列表滚动
    - `panels/cron.rs:render_cron_panel` (~L89-102) — 列表滚动
    - `popups/ask_user.rs:render_ask_user_popup` (~L128-138) — 内容滚动
    - `panels/model.rs:render_model_panel` 的 Browse 模式中的 provider 列表滚动
  - 注意: ScrollState 需要在各面板结构体中替换 `scroll_offset: u16` 字段。此步骤仅替换渲染层调用；状态字段替换在 Task 10 中处理。此处先用 `ScrollState { offset: panel.scroll_offset }` 临时适配
  - 原因: 5 处手动滚动管理统一为 ScrollableArea 组件

- [x] 验证全量编译
  - `cargo build -p rust-agent-tui`
  - 预期: 编译成功（可能有 unused import 警告，后续 Task 清理）

- [x] 为 BorderedPanel 集成编写 headless 冒烟测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs` 底部 `#[cfg(test)] mod tests` 追加
  - 测试场景:
    - `bordered_panel_integration`: 创建 headless app，渲染 agent panel，验证输出包含 "Agent" 且无 panic
  - 运行命令: `cargo test -p rust-agent-tui -- bordered_panel_integration`
  - 预期: 测试通过

**检查步骤:**
- [x] TUI 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished"，无 error
- [x] 无 `f.render_widget(Clear,` 残留（BorderedPanel 替换完成）
  - `grep -c "f.render_widget(Clear" rust-agent-tui/src/ui/main_ui/panels/*.rs rust-agent-tui/src/ui/main_ui/popups/*.rs`
  - 预期: 输出全部为 0（或仅 main_ui.rs 中的非面板 Clear 使用保留）
- [x] 现有 headless 测试不受影响
  - `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"`
  - 预期: 153+ passed（与当前基线一致）

---

### Task 9: TUI 集成 — 列表/输入/选择组件渲染替换

**背景:**
替换 TUI 中 SelectableList（5 处列表渲染）、InputField（4 处表单字段渲染）、TabBar（2 处标签渲染）、RadioGroup（1 处单选列表）、CheckboxGroup（1 处多选列表）的渲染代码。本 Task 仅替换渲染层代码，不修改状态管理（状态管理替换在 Task 10）。各面板/弹窗结构体保持不变，渲染函数内部使用 perihelion-widgets 组件替换手动 for 循环 + Span 拼接。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/panels/agent.rs`（列表渲染替换为 SelectableList）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs`（列表渲染替换为 SelectableList）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/cron.rs`（列表渲染替换为 SelectableList）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/model.rs`（Browse 模式列表 + Edit 模式输入字段）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/relay.rs`（Edit 模式输入字段）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/hitl.rs`（CheckboxGroup 替换）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/ask_user.rs`（TabBar + RadioGroup + 自定义输入替换）

**执行步骤:**
- [x] 替换 AskUser popup 的 TabBar 渲染
  - 位置: `popups/ask_user.rs:render_ask_user_popup` (~L31-55)
  - 替换手动 tab Span 拼接为:
    ```rust
    use perihelion_widgets::{TabBar, TabState};
    let mut tab_state = TabState::new(labels);
    tab_state.set_indicator(i, if confirmed[i] { Some('✓') } else { None });
    f.render_stateful_widget(TabBar::new().style(tab_style), tab_area, &mut tab_state);
    ```
  - 注意: 为 TabState 添加了 `set_active()` 方法
  - 原因: 标签导航渲染统一

- [ ] 替换 AgentPanel/ThreadBrowser/CronPanel 的列表渲染为 SelectableList
  - 跳过原因: SelectableList 不支持 Scrollbar（会丢失 Task 8 添加的滚动条），且 agent.rs 有多行 item（name + description），ListState 缺少 `set_cursor()` 方法。需要先增强 SelectableList widget 再替换。
  - 位置: `panels/agent.rs:render_agent_panel` (~L38-96)
  - 原因: 5 处 for 循环列表渲染统一为 SelectableList 组件

- [ ] 替换 RelayPanel Edit 模式的输入字段为 InputField
  - 跳过原因: InputField.to_line() 的 cursor 固定在文本末尾（`hello█`），而当前代码的 cursor 在文本内部（`hel▏lo`），UX 差异显著。需要先增强 InputField widget 支持内部 cursor 渲染。
  - 位置: `panels/relay.rs:render_relay_edit` (~L106-176)
  - 原因: 3 个字段的 label+value+cursor 渲染统一为 InputField 组件

- [ ] 替换 ModelPanel Edit 模式的输入字段为 InputField
  - 跳过原因: 同上（cursor 位置差异），且 ProviderType 字段是类型选择器而非文本输入
  - 位置: `panels/model.rs:render_model_panel` 的 Edit/New 模式区域
  - 原因: 6 个表单字段的渲染统一

- [ ] 替换 AskUser popup 的 RadioGroup 渲染
  - 跳过原因: 当前渲染有 ▶ cursor 前缀、独立 description 行、自定义输入行，RadioGroup widget 不支持这些特性
  - 位置: `popups/ask_user.rs:render_ask_user_popup` (~L81-108)
  - 原因: 单选列表渲染统一

- [ ] 替换 HITL popup 的 CheckboxGroup 渲染
  - 跳过原因: 当前渲染每个 item 有 2 行（tool name + parameter preview），CheckboxGroup 仅支持单行
  - 位置: `popups/hitl.rs:render_hitl_popup` (~L39-79)
  - 原因: 多选列表渲染统一

- [x] 验证全量编译和测试
  - `cargo build -p rust-agent-tui && cargo test -p rust-agent-tui --lib`
  - 预期: 编译成功，153+ 测试通过

- [x] 为组件集成编写 headless 回归测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs` 底部追加
  - 测试场景:
    - `tab_bar_integration`: 渲染 ask_user popup（构造 mock 数据），验证包含 tab 标签
  - 运行命令: `cargo test -p rust-agent-tui -- tab_bar_integration`
  - 预期: 测试通过

**检查步骤:**
- [x] TUI 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: "Finished"，无 error
- [x] 全量测试通过
  - `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"`
  - 预期: 153+ passed
- [ ] SelectableList 渲染无残留的手动 for 循环列表
  - 跳过: SelectableList 替换未执行（widget 限制），此检查不适用

---

### Task 10: TUI 集成 — FormState 状态管理替换

**背景:**
将 RelayPanel（3 个 buf_* 字段 + RelayEditField 枚举）和 ModelPanel（6 个 buf_* 字段 + EditField 枚举）的 EditField+buffer 模式替换为 FormState<F>。这是最复杂的集成步骤——涉及状态结构体、事件处理、操作方法的重构。RelayPanel 的 RelayEditField 实现 FormField trait，ModelPanel 的 EditField 实现 FormField trait。ModelPanel 的 `buf_thinking_enabled: bool` 和 `buf_thinking_budget: String` 需要特殊处理（thinking_enabled 不进 FormState，thinking_budget 作为普通 InputState 进 FormState）。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/relay_panel.rs`（RelayPanel 结构体重构，实现 FormField）
- 修改: `rust-agent-tui/src/app/model_panel.rs`（ModelPanel 结构体重构，实现 FormField）
- 修改: `rust-agent-tui/src/app/panel_ops.rs`（relay panel 操作适配 FormState API）
- 修改: `rust-agent-tui/src/event.rs`（键盘事件处理适配 FormState API）
- 修改: `rust-agent-tui/src/app/hitl_ops.rs`（如有 relay 相关操作引用）
- 修改: `rust-agent-tui/src/app/ask_user_ops.rs`（custom_input 如适用）

**执行步骤:**
- [x] 为 RelayEditField 实现 FormField trait
  - 位置: `rust-agent-tui/src/app/relay_panel.rs` 顶部
  - 内容:
    ```rust
    use perihelion_widgets::FormField;
    impl FormField for RelayEditField {
        fn next(self) -> Self { match self { Self::Url => Self::Token, Self::Token => Self::Name, Self::Name => Self::Url } }
        fn prev(self) -> Self { match self { Self::Url => Self::Name, Self::Token => Self::Url, Self::Name => Self::Token } }
        fn label(self) -> &'static str { match self { Self::Url => "URL", Self::Token => "Token", Self::Name => "名称" } }
    }
    ```

- [x] 重构 RelayPanel 结构体
  - 位置: `rust-agent-tui/src/app/relay_panel.rs`
  - 删除字段: `buf_url`, `buf_token`, `buf_name`, `cursor`
  - 新增字段: `form: FormState<RelayEditField>`
  - 修改 `from_config()` 初始化 FormState
  - 修改 `enter_edit()` 从 config 读取值到 FormState
  - 修改 `apply_edit()` 从 FormState 读取值写回 config
  - 删除方法: `current_buf()`, `push_char()`, `pop_char()`, `delete_char()`, `cursor_left()`, `cursor_right()`, `cursor_home()`, `cursor_end()`, `paste_text()`（这些操作改为通过 `form.handle_*()` 调用）
  - 修改 `field_next()` 为 `form.next_field()`
  - 修改 `field_prev()` 为 `form.prev_field()`
  - 修改 `edit_field` getter 为 `form.active_field()`
  - 保留 `display_token()` 使用 `form.input(RelayEditField::Token).value()` 实现
  - 原因: 3 个 buf_* + cursor 完全由 FormState 管理

- [x] 为 EditField 实现 FormField trait
  - 位置: `rust-agent-tui/src/app/model_panel.rs` 顶部
  - 内容:
    ```rust
    impl FormField for EditField {
        fn next(self) -> Self { match self {
            Self::Name => Self::ProviderType,
            Self::ProviderType => Self::ModelId,
            Self::ModelId => Self::ApiKey,
            Self::ApiKey => Self::BaseUrl,
            Self::BaseUrl => Self::ThinkingBudget,
            Self::ThinkingBudget => Self::Name,
        }}
        fn prev(self) -> Self { /* 反向 */ }
        fn label(self) -> &'static str { match self {
            Self::Name => "名称", Self::ProviderType => "类型",
            Self::ModelId => "模型", Self::ApiKey => "API Key",
            Self::BaseUrl => "Base URL", Self::ThinkingBudget => "思考预算",
        }}
    }
    ```

- [x] 重构 ModelPanel 结构体
  - 位置: `rust-agent-tui/src/app/model_panel.rs`
  - 删除字段: `buf_name`, `buf_type`, `buf_model`, `buf_api_key`, `buf_base_url`, `buf_thinking_budget`
  - 新增字段: `form: FormState<EditField>`
  - 保留字段: `buf_thinking_enabled: bool`（不在 FormState 中，因为是 bool toggle 而非文本输入）
  - 修改 `enter_edit()` 从 provider 读取值到 FormState
  - 修改 `enter_new()` 初始化 FormState
  - 修改 `apply_edit()` 从 FormState 读取值写回 config
  - 删除方法: `push_char()`, `pop_char()`, `paste_text()`（改为通过 `form.handle_*()` 调用）
  - 修改 `field_next()` 为 `form.next_field()`
  - 保留 `cycle_type()` 和 `toggle_thinking()`（非文本编辑操作）
  - 原因: 6 个 buf_* 字段由 FormState 管理，buf_thinking_enabled 保持原样

- [x] 适配 event.rs 中的键盘事件处理
  - 位置: `rust-agent-tui/src/event.rs` 中 relay_panel 和 model_panel 的字符输入处理
  - 将 `panel.push_char(c)` 改为 `panel.form.handle_char(c)`
  - 将 `panel.pop_char()` 改为 `panel.form.handle_backspace()`
  - 将 `panel.paste_text(text)` 改为 `panel.form.handle_paste(text)`
  - 同理适配 cursor_left/right/home/end

- [x] 适配 panel_ops.rs 中的面板操作
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`
  - 将 `panel.field_next()` 改为 `panel.form.next_field()`
  - 确保所有对 buf_* 字段的直接引用改为通过 `form.input(field).value()`

- [x] 验证全量编译和测试
  - `cargo build -p rust-agent-tui && cargo test -p rust-agent-tui --lib`
  - 预期: 编译成功，153+ 测试通过

- [x] 为 FormState 集成编写单元测试
  - 测试文件: `rust-agent-tui/src/app/relay_panel.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `relay_form_state_field_navigation`: 创建 RelayPanel，调用 field_next() 3 次，验证循环回到 Url
    - `relay_form_state_text_editing`: 调用 form.handle_char() 系列，验证 input(RelayEditField::Url).value() 正确
  - 运行命令: `cargo test -p rust-agent-tui -- relay_form_state`
  - 预期: 测试通过

**检查步骤:**
- [x] TUI 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: "Finished"，无 error
- [x] 无 buf_* 字段残留（RelayPanel 已完全迁移到 FormState）
  - `grep "buf_url\|buf_token\|buf_name" rust-agent-tui/src/app/relay_panel.rs`
  - 预期: 无输出（已删除）
- [x] ModelPanel 的文本 buf_* 已迁移（buf_thinking_enabled 除外）
  - `grep -c "buf_name\|buf_type\|buf_model\|buf_api_key\|buf_base_url\|buf_thinking_budget" rust-agent-tui/src/app/model_panel.rs`
  - 预期: 0
- [ ] 全量测试通过
  - `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"`
  - 预期: 153+ passed

---

### Task 11: perihelion-widgets 组件库验收

**前置条件:**
- spec-plan-1.md 的 Task 1-3 已完成
- spec-plan-2.md 的 Task 4-7 已完成
- 本文件 Task 8-10 已完成
- `cargo build` 全量编译通过
- `cargo test` 全量测试通过

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | grep "test result"`
   - 预期: 所有 crate 测试通过（允许 1 个已知多线程运行时失败）
   - 失败排查: 检查各 Task 的检查步骤，逐个 crate 运行 `cargo test -p <crate>`

2. perihelion-widgets 可独立编译和发布
   - `cargo build -p perihelion-widgets --features markdown 2>&1 | tail -3`
   - 预期: "Finished"，无 error
   - 失败排查: 检查 Task 1 的 Cargo.toml 配置

3. 无循环依赖验证
   - `cargo tree -p perihelion-widgets 2>&1 | head -20`
   - 预期: 依赖树仅包含 ratatui、pulldown-cmark、unicode-width，不包含 rust-create-agent 或 rust-agent-tui
   - 失败排查: 检查 perihelion-widgets/Cargo.toml 确保无内部依赖

4. BorderedPanel 替换完整性
   - `grep -rn "f.render_widget(Clear" rust-agent-tui/src/ui/main_ui/panels/ rust-agent-tui/src/ui/main_ui/popups/ 2>/dev/null`
   - 预期: 无输出（所有 10 处已替换为 BorderedPanel）
   - 失败排查: 检查 Task 8 中遗漏的文件

5. MarkdownRenderer 迁移验证
   - `grep "use perihelion_widgets.*markdown" rust-agent-tui/src/ui/markdown/mod.rs`
   - 预期: 输出包含 perihelion_widgets 的 markdown 模块引用
   - 失败排查: 检查 Task 7 的 TUI 层适配

6. Headless 测试回归验证
   - `cargo test -p rust-agent-tui --lib -- ui::headless 2>&1 | grep "test result"`
   - 预期: 所有 headless 测试通过（渲染输出与迁移前一致）
   - 失败排查: 对比迁移前后 headless snapshot，检查颜色/布局变化

7. workspace 全量编译
   - `cargo build 2>&1 | tail -5`
   - 预期: 所有 6 个 crate（含新增 perihelion-widgets）编译成功
   - 失败排查: 逐个 crate 编译定位错误

8. perihelion-widgets 全部 11 个组件测试通过
   - `cargo test -p perihelion-widgets --features markdown 2>&1 | grep "test result"`
   - 预期: 所有测试通过（覆盖 Theme、BorderedPanel、ScrollState、SelectableList、InputState、TabBar、RadioGroup、CheckboxGroup、FormState、MarkdownRenderer 共 11 个组件）
   - 失败排查: 检查各 Task 的检查步骤，按模块逐个运行 `cargo test -p perihelion-widgets --features markdown -- module_name`
