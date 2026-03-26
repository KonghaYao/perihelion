# SubAgent 消息层级显示 人工验收清单

**生成时间:** 2026-03-26
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [ ] [AUTO] 全量测试通过（验收前确认基线）: `cargo test -p rust-agent-tui 2>&1 | tail -3`

### 测试数据说明
- 场景 1-3 的所有验收项均通过自动化 headless 测试完成，无需准备额外数据
- 场景 4 需要一个已配置 API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）且有 `.claude/agents/` 目录下存在至少一个 agent 定义文件的工作目录

---

## 验收项目

### 场景 1：代码实现完整性

#### - [x] 1.1 SubAgentGroup 变体与事件变体已正确添加

- **来源:** Task 1、Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "SubAgentGroup" rust-agent-tui/src/ui/message_view.rs` → 期望: 输出至少包含 `SubAgentGroup {` 结构定义行，字段含 `agent_id`、`total_steps`、`recent_messages`、`is_running`、`collapsed`、`final_result`
  2. [A] `grep -n "SubAgentStart\|SubAgentEnd" rust-agent-tui/src/app/events.rs` → 期望: 输出包含 `SubAgentStart` 和 `SubAgentEnd` 两个变体定义
  3. [A] `grep -n "subagent_group" rust-agent-tui/src/ui/message_view.rs` → 期望: 包含 `subagent_group` 构造函数定义
- **异常排查:**
  - 如果 grep 无输出: 检查 Task 1 的代码变更是否正确写入 `rust-agent-tui/src/ui/message_view.rs` 和 `rust-agent-tui/src/app/events.rs`

#### - [x] 1.2 launch_agent 事件映射已添加

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "SubAgentStart\|SubAgentEnd" rust-agent-tui/src/app/agent.rs` → 期望: 至少出现 2 行，分别对应 ToolStart 映射为 SubAgentStart 和 ToolEnd 映射为 SubAgentEnd
  2. [A] `grep -n "launch_agent" rust-agent-tui/src/app/agent.rs | grep -v "^--$"` → 期望: 包含 `if name == "launch_agent"` 的条件守卫行
- **异常排查:**
  - 如果映射不存在: 检查 Task 2，确认 `agent.rs` 中的 match 分支顺序（launch_agent 分支必须在通用 ToolStart 分支之前）

#### - [x] 1.3 渲染线程 UpdateLastMessage 变体存在

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -c "UpdateLastMessage" rust-agent-tui/src/ui/render_thread.rs` → 期望: 输出 `2`（定义行 + match 处理行各一次）
  2. [A] `grep -n "subagent_group_idx" rust-agent-tui/src/app/mod.rs` → 期望: 至少 2 行（字段声明 + `None` 初始化）
  3. [A] `grep -n "SubAgentGroup" rust-agent-tui/src/ui/message_render.rs` → 期望: 出现 match 分支行（渲染实现）
- **异常排查:**
  - 如果 UpdateLastMessage 只出现 1 次: 检查 Task 3，确认 `RenderTask::run` 中的 match 处理分支已添加

---

### 场景 2：SubAgentGroup 事件生命周期

#### - [x] 2.1 基础生命周期：SubAgentStart → ToolCall × 2 → SubAgentEnd

- **来源:** Task 6 `test_subagent_group_basic`、spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_subagent_group_basic -- --nocapture 2>&1 | grep -E "FAILED|ok\."` → 期望: 输出包含 `ok` 且不含 `FAILED`
  2. [A] `cargo test -p rust-agent-tui test_subagent_group_basic -- --nocapture 2>&1 | grep -v "^$"` → 期望: 测试输出中应无 panic 信息，is_running=false 且 total_steps=2 的断言应通过
- **异常排查:**
  - 如果测试失败并显示 `is_running should be false`: 检查 Task 4 中 `SubAgentEnd` 分支是否正确设置 `is_running = false`
  - 如果测试失败并显示 `total_steps 应为 2`: 检查 Task 4 中 ToolCall 路由分支是否正确执行 `total_steps += 1`

#### - [x] 2.2 滑动窗口：6 步只保留最近 4 条，但 total_steps 仍为 6

- **来源:** Task 6 `test_subagent_group_sliding_window`、spec-design.md 目标
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_subagent_group_sliding_window -- --nocapture 2>&1 | grep -E "FAILED|ok\."` → 期望: 输出包含 `ok` 且不含 `FAILED`
  2. [A] `cargo test -p rust-agent-tui test_subagent_group_sliding_window -- --nocapture 2>&1 | grep -v "^$"` → 期望: `recent_messages.len() <= 4` 断言通过，`total_steps == 6` 断言通过
- **异常排查:**
  - 如果 recent_messages > 4: 检查 Task 4 中 ToolCall 路由的滑动窗口逻辑（`if recent_messages.len() >= 4 { remove(0) }`）
  - 如果 total_steps != 6: 确认 `total_steps` 字段在每次 ToolCall 时都递增且不受 remove(0) 影响

#### - [x] 2.3 AssistantChunk 路由进 SubAgentGroup 而非父 Agent

- **来源:** Task 6 `test_subagent_group_assistant_chunk`、spec-design.md 方案设计
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_subagent_group_assistant_chunk -- --nocapture 2>&1 | grep -E "FAILED|ok\."` → 期望: 输出包含 `ok` 且不含 `FAILED`
  2. [A] `cargo test -p rust-agent-tui test_subagent_group_assistant_chunk -- --nocapture 2>&1 | grep -v "^$"` → 期望: `recent_messages 应包含 AssistantBubble` 和 `final_result 应为工具返回值` 两个断言均通过
- **异常排查:**
  - 如果 AssistantBubble 不在 recent_messages 中: 检查 Task 4 中 `AssistantChunk` 分支，确认 `subagent_group_idx.is_some()` 时走 SubAgentGroup 路由而不是父 Agent 路由

---

### 场景 3：全量回归与渲染线程

#### - [x] 3.1 全量测试无回归（57 个测试全部通过）

- **来源:** Task 7 验收场景 4-5
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep "test result"` → 期望: 输出 `test result: ok. 57 passed; 0 failed`（或更多通过，0 失败）
  2. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^error"` → 期望: 无输出（无编译错误）
- **异常排查:**
  - 如果有测试失败: 运行 `cargo test -p rust-agent-tui -- --nocapture 2>&1 | grep "FAILED"` 获取失败测试名，定位到对应 Task

#### - [x] 3.2 渲染缓存 UpdateLastMessage 正确替换最后一条消息

- **来源:** Task 3 实现、render_thread 内置测试
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui render_thread -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: 所有 render_thread 相关测试通过（含 `test_add_message_increments_version` 和 `test_append_chunk_updates_last_message`）
- **异常排查:**
  - 如果 render_thread 测试失败: 检查 Task 3 中 `UpdateLastMessage` 的实现，确认 `message_offsets.last()` 正确获取 start offset

---

### 场景 4：TUI 实际视觉效果（需要人工运行 TUI）

> **前提：** 需要已配置 API Key 的工作目录，以及在 `.claude/agents/` 下存在至少一个 agent 定义文件（如 `code-reviewer.md`）

#### - [x] 4.1 TUI 界面中 SubAgentGroup 头行视觉正确

- **来源:** spec-design.md 渲染样式、验收标准第 1-3 条
- **操作步骤:**
  1. [H] 在配置好 API Key 的目录下运行 `cargo run -p rust-agent-tui`，发送一条会触发 launch_agent 的消息（如 "请用 {agent_id} agent 来分析代码"）。观察 TUI 消息区域：SubAgent 执行时是否出现 `▾ 🤖 {agent_id}` 的绿色头行，头行右侧是否显示黄色的"运行中 · 已执行 N 步" → 是/否
  2. [H] SubAgent 执行完成后，观察头行是否变为绿色"已完成 N 步"，头行下方是否出现"结果:"摘要行 → 是/否
- **异常排查:**
  - 如果头行颜色不是绿色: 检查 `message_render.rs` 中 `agent_color = Color::Rgb(129, 199, 132)` 的设置
  - 如果没有出现头行: 检查 `agent.rs` 中 ToolStart 的 guard 条件是否正确匹配 `"launch_agent"`

#### - [x] 4.2 折叠展开交互通过 Shift+T 全局切换

- **来源:** spec-design.md 实现要点第 4 条（折叠交互）
- **操作步骤:**
  1. [A] `grep -n "toggle_collapse\|SubAgentGroup" rust-agent-tui/src/ui/message_view.rs | grep -A2 "toggle_collapse"` → 期望: SubAgentGroup 在 toggle_collapse 的 match 分支中出现，说明折叠逻辑已接入
  2. [H] 在 TUI 运行状态下（SubAgent 已完成、SubAgentGroup 处于展开状态），按 `Shift+T`，观察 SubAgentGroup 是否折叠为单行（显示 `▸ 🤖 {agent_id}  「已完成 N 步」  摘要...`），再按 `Shift+T` 是否恢复展开 → 是/否
- **异常排查:**
  - 如果 Shift+T 无效: 检查 `event.rs` 中 `toggle_collapsed_messages()` 的触发逻辑，确认 `SubAgentGroup` 在 `ToggleToolMessages` 事件处理中被包含
  - 如果折叠后显示内容不对: 检查 `message_render.rs` 中折叠状态（`collapsed == true`）的渲染分支

#### - [x] 4.3 父 Agent 消息在 SubAgentGroup 外正常显示，不被吞噬

- **来源:** spec-design.md 验收标准第 6 条，app 状态管理 MessageAdded 忽略逻辑
- **操作步骤:**
  1. [A] `grep -n "subagent_group_idx.is_some()" rust-agent-tui/src/app/agent_ops.rs | head -5` → 期望: 出现 3 处以上（ToolCall 路由、AssistantChunk 路由、MessageAdded 忽略各一处）
  2. [H] 在 TUI 中触发一次 SubAgent 后，观察父 Agent 随后的文字回复（SubAgentEnd 之后的 AssistantChunk）是否正常显示在 SubAgentGroup 块的**外部**（不在块内），而不是被塞入 SubAgentGroup 的 recent_messages → 是/否
- **异常排查:**
  - 如果父 Agent 回复出现在 SubAgentGroup 内: 检查 Task 4 中 `subagent_group_idx` 是否在 SubAgentEnd 分支中正确清空为 `None`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | SubAgentGroup 变体与事件变体完整性 | 3 | 0 | ✅ | |
| 场景 1 | 1.2 | launch_agent 事件映射正确性 | 2 | 0 | ✅ | |
| 场景 1 | 1.3 | UpdateLastMessage + subagent_group_idx | 3 | 0 | ✅ | |
| 场景 2 | 2.1 | 基础生命周期 headless 测试 | 2 | 0 | ✅ | |
| 场景 2 | 2.2 | 滑动窗口 headless 测试 | 2 | 0 | ✅ | |
| 场景 2 | 2.3 | AssistantChunk 路由 headless 测试 | 2 | 0 | ✅ | |
| 场景 3 | 3.1 | 全量测试无回归 | 2 | 0 | ✅ | |
| 场景 3 | 3.2 | 渲染线程 UpdateLastMessage | 1 | 0 | ✅ | |
| 场景 4 | 4.1 | TUI 视觉：SubAgentGroup 头行颜色与格式 | 0 | 2 | ✅ | |
| 场景 4 | 4.2 | TUI 交互：Shift+T 折叠展开 | 1 | 1 | ✅ | |
| 场景 4 | 4.3 | TUI 隔离：父 Agent 消息不被吞噬 | 1 | 1 | ✅ | |

**验收结论:** ✅ 全部通过
