# TUI 显示修复 人工验收清单

**生成时间:** 2026-03-29
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链: `rustc --version`
- [ ] [AUTO] 编译 TUI: `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望: `Finished` 且无编译错误
- [ ] [AUTO] 确认修改文件存在: `test -f rust-agent-tui/src/ui/message_render.rs && test -f rust-agent-tui/src/ui/render_thread.rs && test -f rust-agent-tui/src/ui/main_ui.rs && echo OK` → 期望: `OK`

---

## 验收项目

### 场景 1：单元测试验证

#### - [x] 1.1 SubAgentGroup 内部消息无序号
- **来源:** Task 1 检查步骤 / spec-design.md 修改 1
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_subagent_group 2>&1 | tail -10` → 期望: 3 个 subagent group 测试通过，内部消息渲染结果中不含 `0`、`01` 等无意义序号
- **异常排查:**
  - 如果测试失败: 检查 `message_render.rs` 中 `render_view_model` 签名是否为 `Option<usize>`，SubAgentGroup 内部递归调用是否传入 `None`

#### - [x] 1.2 AI 消息无 ToolUse 行
- **来源:** Task 2 检查步骤 / spec-design.md 修改 2
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_tool_call 2>&1 | tail -10` → 期望: 4 个 tool_call 测试通过
  2. [A] `cargo test -p rust-agent-tui --lib -- test_ai_message_with_only_tool_calls 2>&1 | tail -5` → 期望: 测试通过
- **异常排查:**
  - 如果测试失败: 检查 `message_render.rs` 中 `AssistantBubble` 的 `ContentBlockView::ToolUse` 分支是否已跳过渲染

### 场景 2：构建与回归验证

#### - [x] 2.1 编译通过
- **来源:** Task 1-3 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望: `Finished` 且无编译错误（允许 warning）
- **异常排查:**
  - 如果编译失败: 检查 `render_view_model` 的 `Option<usize>` 签名变更是否已同步到所有调用点

#### - [x] 2.2 全量测试通过
- **来源:** Task 4 End-to-end verification
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep "test result:"` → 期望: 所有测试通过（3 行 `ok`，总计 108+ 测试）
- **异常排查:**
  - 如果有测试失败: 查看失败测试名称，对照 Task 1（签名变更）或 Task 2（ToolUse 跳过）排查

#### - [x] 2.3 弹窗高度上限 60%
- **来源:** Task 3 / spec-design.md 修改 3
- **操作步骤:**
  1. [A] `grep -n 'screen_height \* 3 / 5' rust-agent-tui/src/ui/main_ui.rs` → 期望: 匹配到 `active_panel_height` 函数中的 `max_h` 赋值行，注释为 `最多占 60% 屏高`
- **异常排查:**
  - 如果未匹配: 检查 `main_ui.rs` 的 `active_panel_height` 函数是否仍为 `screen_height * 2 / 5`

### 场景 3：TUI 运行时视觉验证

#### - [x] 3.1 TUI 运行时视觉验证
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui` → 期望: 编译成功
  2. [H] 启动 TUI：在终端运行 `cargo run -p rust-agent-tui -- -a`，输入一个会触发子 Agent 的问题（如"帮我查看项目结构"），观察 SubAgentGroup 展开时内部消息是否无序号前缀 → 是/否
  3. [H] 在 TUI 中观察 AI 回复消息区域，确认不再显示 `🔧 toolname` 的 ToolUse 行（工具调用仅以 `▸ toolname ▾` 的 ToolBlock 形式显示） → 是/否
  4. [H] 触发 HITL 审批（需审批模式下执行 bash 命令），观察弹窗高度是否比之前更大（最多占屏幕高度 60%） → 是/否
- **异常排查:**
  - 如果 SubAgentGroup 内部仍有序号: 检查 `render_view_model` 内部递归是否传入 `None`
  - 如果仍有 ToolUse 行: 检查 `AssistantBubble` 渲染中 `ToolUse` 分支是否已跳过
  - 如果弹窗仍偏小: 检查 `active_panel_height` 中 `max_h` 计算

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | SubAgentGroup 内部消息无序号 | 1 | 0 | ✅ | |
| 场景 1 | 1.2 | AI 消息无 ToolUse 行 | 2 | 0 | ✅ | |
| 场景 2 | 2.1 | 编译通过 | 1 | 0 | ✅ | |
| 场景 2 | 2.2 | 全量测试通过 | 1 | 0 | ✅ | |
| 场景 2 | 2.3 | 弹窗高度上限 60% | 1 | 0 | ✅ | |
| 场景 3 | 3.1 | TUI 运行时视觉验证 | 1 | 3 | ✅ | |

**验收结论:** ✅ 全部通过
