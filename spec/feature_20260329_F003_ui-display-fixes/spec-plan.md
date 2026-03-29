# TUI 显示修复 执行计划

**目标:** 修复 TUI 的 3 个 UI 显示问题：SubAgent 序号多余、AI 工具调用重复、弹窗高度不足

**技术栈:** Rust + ratatui（TUI 渲染）

**设计文档:** `spec/feature_20260329_F003_ui-display-fixes/spec-design.md`

---

### Task 1: SubAgentGroup 内部消息去序号

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_render.rs`
- 修改: `rust-agent-tui/src/ui/render_thread.rs`

**执行步骤:**
- [x] 修改 `render_view_model` 签名：`index: usize` → `index: Option<usize>`
  - `Some(n)` 表示外层消息，渲染时带序号前缀
  - `None` 表示 SubAgent 内部消息，不渲染序号
- [x] 更新 `UserBubble` 分支：`None` 时直接渲染内容，不加 `n ` 前缀
- [x] 更新 `AssistantBubble` 分支：`None` 时标题行不显示 index，ToolUse 的 `tool_idx` 也不渲染（统一改为跳过，与 Task 2 合并处理）
- [x] 更新 `ToolBlock` 分支：`None` 时标题只显示 `▸ display_name`
- [x] 更新 `SubAgentGroup` 分支：
  - 头行/折叠行根据 `index` 决定是否显示序号
  - 内部递归调用改为 `render_view_model(inner_vm, None, _width)`
- [x] 更新 `render_thread.rs` 的 `render_one`：将 `index: usize` 透传为 `Some(index)`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无编译错误
- [x] SubAgentGroup headless 测试通过
  - `cargo test -p rust-agent-tui --lib -- test_subagent_group 2>&1 | tail -10`
  - 预期: 3 个 subagent group 测试通过

---

### Task 2: AI 消息去除 ToolUse 渲染

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_render.rs`

**执行步骤:**
- [x] 在 `AssistantBubble` 渲染循环中，将 `ContentBlockView::ToolUse` 分支改为 `continue`
  - 不再渲染 `🔧 name` 行
  - 如果第一个 block 就是 ToolUse，`first_text_merged` 仍为 false，需要确保后续 Text block 能正确创建标题行（当前逻辑已能处理：后续遇到 Text 时会检查 `first_text_merged`）
- [x] 移除 `tool_idx` 变量及其递增逻辑（不再需要）

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无编译错误
- [x] headless 测试通过（验证 ToolBlock 仍然正确渲染）
  - `cargo test -p rust-agent-tui --lib -- test_tool_call 2>&1 | tail -5`
  - 预期: 测试通过

---

### Task 3: 弹窗高度上限提升

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 修改 `active_panel_height` 函数中的 `max_h` 计算
  - `screen_height * 2 / 5` → `screen_height * 3 / 5`
  - 注释同步更新：`最多占 40% 屏高` → `最多占 60% 屏高`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无编译错误

---

### Task 4: UI Display Fixes Acceptance

**Prerequisites:**
- Start command: `cargo build -p rust-agent-tui`
- 所有 Task 1-3 已完成

**End-to-end verification:**

1. [x] 全量测试通过
   - `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - Expected: 所有测试通过，无失败
   - On failure: 检查 Task 1 (签名变更) 和 Task 2 (ToolUse 跳过)

2. [x] SubAgentGroup 内部消息无序号
   - `cargo test -p rust-agent-tui --lib -- test_subagent_group_basic 2>&1 | tail -5`
   - Expected: 测试通过，snapshot 中内部工具调用无 `0`、`01` 等序号前缀
   - On failure: 检查 Task 1

3. [x] AI 消息无 ToolUse 行
   - `cargo test -p rust-agent-tui --lib -- test_ai_message_with_only_tool_calls 2>&1 | tail -5`
   - Expected: `message_view.rs` 单元测试通过（数据层不变，仅渲染层跳过）
   - On failure: 检查 Task 2

4. [x] 弹窗高度计算正确
   - `cargo test -p rust-agent-tui --lib -- test_subagent_group_basic 2>&1 | tail -5`
   - Expected: 测试通过（弹窗高度变更不影响 headless 渲染逻辑）
   - On failure: 检查 Task 3
