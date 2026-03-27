# SubAgent 消息层级显示 执行计划

**目标:** 在 TUI 中将 SubAgent 执行的消息包裹在可折叠的层级块中，实时显示最近 4 步并记录总步数

**技术栈:** Rust / ratatui / tokio mpsc / MessageViewModel

**设计文档:** ./spec-design.md

---

### Task 1: TUI 事件与 ViewModel 扩展

**涉及文件:**
- 修改: `rust-agent-tui/src/app/events.rs`
- 修改: `rust-agent-tui/src/ui/message_view.rs`

**执行步骤:**
- [x] 在 `AgentEvent` 枚举末尾新增两个变体
  - `SubAgentStart { agent_id: String, task_preview: String }` — 由 launch_agent ToolStart 映射
  - `SubAgentEnd { result: String, is_error: bool }` — 由 launch_agent ToolEnd 映射
- [x] 在 `MessageViewModel` 枚举新增 `SubAgentGroup` 变体
  - 字段：`agent_id`, `task_preview`, `total_steps: usize`, `recent_messages: Vec<MessageViewModel>`（max 4）, `is_running: bool`, `collapsed: bool`, `final_result: Option<String>`
- [x] 为 `MessageViewModel` 添加构造函数 `subagent_group(agent_id, task_preview) -> Self`，初始状态 `is_running: true, collapsed: false, total_steps: 0`
- [x] 在 `toggle_collapse` 的 match 分支中处理 `SubAgentGroup`
- [x] 添加辅助方法 `is_subagent_group() -> bool`

**检查步骤:**
- [x] 编译通过，无 dead_code 警告
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "error|warning.*unused"`
  - 预期: 无 error，无因本次修改引入的 unused 警告
- [x] SubAgentGroup 变体存在且结构正确
  - `grep -n "SubAgentGroup" rust-agent-tui/src/ui/message_view.rs`
  - 预期: 出现包含所有字段的结构定义

---

### Task 2: 事件映射层

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在 `FnEventHandler` 的 match 中，在通用 `ToolStart` 分支**之前**插入 `launch_agent` 专属分支
  - 条件：`ExecutorEvent::ToolStart { name, input, .. } if name == "launch_agent"`
  - 提取 `input["agent_id"]`（缺失时 fallback `"unknown"`）和 `input["task"]`（取前 40 字符）
  - 返回 `AgentEvent::SubAgentStart { agent_id, task_preview }`
- [x] 在 `ToolEnd` 分支中为 `launch_agent` 添加专属处理（在通用 `ToolEnd` drop 分支之前）
  - 条件：`ExecutorEvent::ToolEnd { name, output, is_error, .. } if name == "launch_agent"`
  - 返回 `AgentEvent::SubAgentEnd { result: output, is_error }`

**检查步骤:**
- [x] 编译通过，match 分支穷举正确
  - `cargo build -p rust-agent-tui 2>&1 | grep "error"`
  - 预期: 无 error
- [x] `agent.rs` 包含 `SubAgentStart` 和 `SubAgentEnd` 的映射
  - `grep -n "SubAgentStart\|SubAgentEnd" rust-agent-tui/src/app/agent.rs`
  - 预期: 两者均出现在映射 match 块中

---

### Task 3: 渲染线程 UpdateLastMessage

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/render_thread.rs`

**执行步骤:**
- [x] 在 `RenderEvent` 枚举中新增变体 `UpdateLastMessage(MessageViewModel)`
  - 注释：SubAgentGroup 更新专用，替换最后一条消息并重新渲染
- [x] 在 `RenderTask::run` 的 match 中处理 `UpdateLastMessage(vm)`:
  1. `self.messages.last_mut().replace(*vm)`（若 messages 为空则 push）
  2. 重新渲染该消息行：复用 `render_one` 路径（与 `AppendChunk` 相同）
  3. 获取 `message_offsets.last()` 作为 start，`cache.lines.truncate(start)`，extend 新行
  4. `cache.total_lines = cache.lines.len()`, `cache.version += 1`

**检查步骤:**
- [x] `UpdateLastMessage` 变体存在于 `RenderEvent`
  - `grep -n "UpdateLastMessage" rust-agent-tui/src/ui/render_thread.rs`
  - 预期: 定义和 match 处理各出现一次
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep "error"`
  - 预期: 无 error

---

### Task 4: App 状态管理与消息路由

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 在 `App` 结构体中新增字段 `subagent_group_idx: Option<usize>`，初始化为 `None`
- [x] 在 `handle_agent_event` 的 match 中新增 `SubAgentStart` 分支
  - 创建 `MessageViewModel::subagent_group(agent_id, task_preview)`
  - push 到 `self.view_messages`，记录 `subagent_group_idx = Some(len - 1)`
  - 发送 `RenderEvent::AddMessage(vm)`
- [x] 修改 `ToolCall` 分支：当 `subagent_group_idx.is_some()` 时路由进 SubAgentGroup
  - 取 `view_messages[idx]` 的可变引用，确认为 `SubAgentGroup`
  - `total_steps += 1`；若 `recent_messages.len() >= 4`，移除 index 0
  - 将新 `ToolBlock` 推入 `recent_messages`
  - 发送 `RenderEvent::UpdateLastMessage(vm.clone())`
  - 当 `subagent_group_idx == None` 时保持原有行为（正常 push ToolBlock）
- [x] 修改 `AssistantChunk` 分支：当 `subagent_group_idx.is_some()` 时路由进 SubAgentGroup
  - 找到 `recent_messages` 最后一条是否为 `AssistantBubble`，若是则 `append_chunk`
  - 若不是则新建 `AssistantBubble` 推入（若已满 4 条先 pop 最旧的）
  - 发送 `RenderEvent::UpdateLastMessage(vm.clone())`
- [x] 修改 `MessageAdded` 分支：当 `subagent_group_idx.is_some()` 时直接 return（忽略）
- [x] 新增 `SubAgentEnd` 分支
  - 从 `view_messages[idx]` 取可变引用，设 `is_running = false`，写 `final_result = Some(result)`
  - 若 `is_error` 则不修改 collapsed（保持展开展示错误信息）
  - 发送 `RenderEvent::UpdateLastMessage(vm.clone())`
  - 清空 `subagent_group_idx = None`
- [x] 在 `Done` / `Error` / `Disconnected` 分支确保清空 `subagent_group_idx = None`（异常退出时兜底）

**检查步骤:**
- [x] App 结构体包含新字段且初始化正确
  - `grep -n "subagent_group_idx" rust-agent-tui/src/app/mod.rs`
  - 预期: 字段声明 + `None` 初始值各出现一次
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep "error"`
  - 预期: 无 error

---

### Task 5: SubAgentGroup 渲染

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_render.rs`

**执行步骤:**
- [x] 在 `render_view_model` 的 match 中新增 `MessageViewModel::SubAgentGroup { .. }` 分支
- [x] **折叠状态（collapsed == true）**：渲染单行
  - `▸ 🤖 {agent_id}  「已完成 {total_steps} 步」  {final_result 前 50 字符}…`
  - 颜色：`Color::Rgb(129, 199, 132)`（绿）；错误时用 `Color::Red`
- [x] **展开状态（collapsed == false）**：渲染多行
  - 头行：`▾ 🤖 {agent_id}  「{task_preview}」  [{status_label}]`
    - `status_label`：`is_running` 时 `"运行中 · 已执行 {total_steps} 步"` (`Color::Yellow`)，否则 `"已完成 {total_steps} 步"` (绿色)
  - 嵌套内容：遍历 `recent_messages`，每行前缀 `"  "`，复用对应 ViewModel 的渲染逻辑（内联调用 `render_view_model`，index 传 0 以隐藏序号）
  - 若 `total_steps > 4`：末尾追加 `"  [仅显示最近 4/{total_steps} 步]"` (`Color::DarkGray`)
  - 若 `!is_running && final_result.is_some()`：追加 `"  结果: {final_result}"` (绿色)
- [x] 嵌套渲染时，`SubAgentGroup` 内的 `ToolBlock` 和 `AssistantBubble` 使用 `render_view_model` 递归，确保折叠/展开状态正确

**检查步骤:**
- [x] `render_view_model` 包含 SubAgentGroup 分支
  - `grep -n "SubAgentGroup" rust-agent-tui/src/ui/message_render.rs`
  - 预期: 出现 match 分支
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep "error"`
  - 预期: 无 error

---

### Task 6: Headless 测试

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/headless.rs`

**执行步骤:**
- [x] 新增测试 `test_subagent_group_basic`：注入 SubAgentStart → 2×ToolCall → SubAgentEnd 事件序列
  - 断言屏幕包含 `"🤖"` 或对应 agent_id 文本
  - 断言 total_steps 计数反映在输出中
- [x] 新增测试 `test_subagent_group_sliding_window`：注入 SubAgentStart → 6×ToolCall → SubAgentEnd
  - 断言 recent_messages 最多显示 4 条工具行（不含额外的）
  - 断言出现 `"4/6"` 或相应提示（步数文字）
- [x] 新增测试 `test_subagent_group_assistant_chunk`：注入 SubAgentStart → AssistantChunk → SubAgentEnd
  - 断言 AssistantChunk 文字出现在输出中
- [x] 注入事件使用已有的 `push_agent_event` + `process_pending_events` + `render_notify.notified()` 模式

**检查步骤:**
- [x] 新增的三个测试全部通过
  - `cargo test -p rust-agent-tui test_subagent_group -- --nocapture 2>&1 | tail -10`
  - 预期: 输出包含 `ok` 且无 `FAILED`
- [x] 全量测试无回归
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: `test result: ok`

---

### Task 7: SubAgent 消息层级验收

**Prerequisites:**
- 启动命令: `cargo build -p rust-agent-tui 2>&1 | tail -3`
- 测试前提: `cargo test -p rust-agent-tui` 全绿

**End-to-end verification:**

1. **headless 基础层级结构**（subagent_group_basic 测试）
   - `cargo test -p rust-agent-tui test_subagent_group_basic -- --nocapture 2>&1 | grep -E "ok|FAILED"`
   - Expected: 输出包含 `ok`
   - On failure: 检查 Task 4（SubAgentStart 路由）和 Task 5（渲染逻辑）
   - [x] ✅ PASSED

2. **headless 滑动窗口限制**（6 步只显示 4 步）
   - `cargo test -p rust-agent-tui test_subagent_group_sliding_window -- --nocapture 2>&1 | grep -E "ok|FAILED"`
   - Expected: 输出包含 `ok`
   - On failure: 检查 Task 4（ToolCall 路由中的滑动窗口逻辑）
   - [x] ✅ PASSED

3. **headless AssistantChunk 路由**
   - `cargo test -p rust-agent-tui test_subagent_group_assistant_chunk -- --nocapture 2>&1 | grep -E "ok|FAILED"`
   - Expected: 输出包含 `ok`
   - On failure: 检查 Task 4（AssistantChunk 路由分支）
   - [x] ✅ PASSED

4. **编译无回归**（全量 lint + build）
   - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
   - Expected: 无输出（无 error）
   - On failure: 检查对应 Task 的编译步骤
   - [x] ✅ PASSED

5. **全量测试无回归**
   - `cargo test -p rust-agent-tui 2>&1 | tail -3`
   - Expected: 最后一行包含 `test result: ok`
   - On failure: 检查 Task 6（headless 测试），定位失败测试名称后回溯至对应 Task
   - [x] ✅ PASSED (57 passed)
