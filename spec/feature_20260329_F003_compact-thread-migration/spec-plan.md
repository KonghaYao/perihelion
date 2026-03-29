# Compact Thread Migration 执行计划

**目标:** `/compact` 执行后创建新 Thread，旧 Thread 完整保留，新 Thread 以摘要开始继续对话

**技术栈:** Rust (tokio, serde), JavaScript (Preact Signals)

**设计文档:** spec-design.md

---

### Task 1: AgentEvent CompactDone 结构体变更

**涉及文件:**
- 修改: `rust-agent-tui/src/app/events.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 将 `AgentEvent::CompactDone(String)` 改为结构体变体：
  ```rust
  CompactDone {
      summary: String,
      new_thread_id: ThreadId,
  }
  ```
  - 在 `events.rs` 中引入 `ThreadId` 类型（`use crate::thread::ThreadId` 或已有路径）
- [x] 更新 `agent.rs` 中 `compact_task()` 内所有 `CompactDone(summary)` 构造为 `CompactDone { summary, new_thread_id }`
  - 注意：`compact_task` 是异步函数，此时不知道 `new_thread_id`。改为 `compact_task` 仍然发送 `CompactDone` 时只携带 `summary`，由 `agent_ops.rs` 的 `handle_agent_event` 在收到事件时负责创建新 Thread 并填充 `new_thread_id`
  - **替代方案**：`compact_task` 仍然发 `CompactDone(summary)`（事件结构中 `new_thread_id` 留空），在 `agent_ops.rs` 收到后创建新 Thread 并赋值。但这样事件结构中字段语义不完整
  - **最终方案**：在 `agent_ops.rs` 的 `handle_agent_event` 中收到 `CompactDone` 后创建新 Thread，将 `new_thread_id` 写入 `self.current_thread_id`，不需要通过事件传递。因此将事件保持为 `CompactDone { summary: String, new_thread_id: String }` 格式，`new_thread_id` 在 `agent_ops` 侧填充

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出 `Compiling rust-agent-tui` 且无 error
- [x] 所有引用 `CompactDone` 的代码已更新
  - `grep -rn "CompactDone" rust-agent-tui/src/`
  - 预期: 无旧的 `CompactDone(String)` 模式匹配

---

### Task 2: TUI CompactDone 分支迁移逻辑

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 重写 `handle_agent_event` 中 `AgentEvent::CompactDone` 分支（当前在 `agent_ops.rs:503-558`）
  - **创建新 Thread**：使用 `block_in_place` + `block_on` 调用 `self.thread_store.create_thread(ThreadMeta::new(&self.cwd))`
  - **构造新消息**：`let new_messages = vec![BaseMessage::system(summary.clone())]`
  - **持久化新 Thread 消息**：使用 `block_on` 调用 `self.thread_store.save_messages(&new_thread_id, &new_messages)`
    - 注意：ThreadStore trait 有 `append_messages` 方法，用于追加写入
  - **更新 current_thread_id**：`self.current_thread_id = Some(new_thread_id.clone())`
  - **更新 agent_state_messages**：`self.agent_state_messages = new_messages`
  - **清空 view_messages**：插入压缩提示 + 摘要
    - `view_messages.clear()`
    - 插入 `MessageViewModel::system("📦 上下文已压缩（从旧对话迁移到新 Thread）")`
    - 插入 `MessageViewModel::system(format!("📋 压缩摘要：\n{}", summary))`
  - **通知渲染线程重建**：`RenderEvent::Clear` + 逐条 `AddMessage`
  - **Relay 通知**：发送 `CompactDone` 事件（含 summary + new_thread_id + old_thread_id），或使用 `send_thread_reset` 推送新 Thread 消息
  - **清理 loading 状态**：`set_loading(false)`、`agent_rx = None`
  - **刷新 pending_messages**：与现有逻辑一致
- [x] 保存旧 Thread ID 用于 Relay 通知
  - 在创建新 Thread 之前记录 `let old_thread_id = self.current_thread_id.clone()`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 error
- [x] CompactDone 分支使用 `block_in_place` 模式（与 `ensure_thread_id` 一致）
  - `grep -A5 "CompactDone" rust-agent-tui/src/app/agent_ops.rs | head -20`
  - 预期: 包含 `block_in_place` 调用

---

### Task 3: RelayMessage 新增 CompactDone 变体

**涉及文件:**
- 修改: `rust-relay-server/src/protocol.rs`
- 修改: `rust-relay-server/src/client/mod.rs`（仅在 TUI 侧通过 `send_value` 发送）

**执行步骤:**
- [x] 在 `RelayMessage` 枚举中新增 `CompactDone` 变体：
  ```rust
  /// 上下文压缩完成（Agent → Web），携带摘要和新旧 Thread ID
  CompactDone {
      summary: String,
      new_thread_id: String,
      old_thread_id: String,
  }
  ```
- [x] 新增序列化测试 `test_compact_done_serialization`
  - 验证 `type` 为 `compact_done`、字段正确
  - 验证反序列化 round-trip

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-relay-server 2>&1 | tail -5`
  - 预期: 无 error
- [x] 序列化测试通过
  - `cargo test -p rust-relay-server -- test_compact_done_serialization 2>&1 | tail -5`
  - 预期: `test result: ok`

---

### Task 4: Web 前端 compact_done 事件处理

**涉及文件:**
- 修改: `rust-relay-server/web/events.js`
- 修改: `rust-relay-server/web/components/Pane.js`（如需 UI 更新）

**执行步骤:**
- [x] 在 `events.js` 的 `handleLegacyEvent` 函数中新增 `case 'compact_done'`
  - 清空当前面板消息：`agent.messages = []`
  - 显示压缩提示：`agent.messages.push({ type: 'system', text: '📦 上下文已从旧对话压缩' })`
  - 显示摘要：`agent.messages.push({ type: 'assistant', text: event.summary })`
  - 触发 Signal 更新：`agents.value = new Map(agents.value)`
- [x] 在 `handleAgentEvent` 中确保 `compact_done` 事件被路由到 `handleLegacyEvent`（无 `role` 字段，走 else 分支即可，无需额外改动）
- [x] 在 `Pane.js` 中确认 `/compact` 命令发送逻辑不变（`sendMessage(sessionId, { type: 'compact_thread' })`）

**检查步骤:**
- [x] 前端 JS 无语法错误
  - `node -c rust-relay-server/web/events.js 2>&1`
  - 预期: 无输出（语法正确）
- [x] `compact_done` case 存在于 events.js
  - `grep -n "compact_done" rust-relay-server/web/events.js`
  - 预期: 匹配到对应 case 分支

---

### Task 5: Compact Thread Acceptance

**Prerequisites:**
- Start command: `cargo build -p rust-agent-tui -p rust-relay-server`
- Test data setup: 确保已有至少一条对话历史（发送过至少一条消息的 Thread）

**End-to-end verification:**

1. 编译全量通过
   - `cargo build 2>&1 | tail -5`
   - ✅ 无 error（仅有 unused_mut warning）
   - On failure: 检查 Task 1-3 对应文件的编译错误

2. 全量测试通过
   - `cargo test 2>&1 | tail -10`
   - ✅ 全部通过（0 FAILED）
   - On failure: 检查对应测试用例引用的 Task

3. CompactDone 事件结构验证
   - `grep -A5 "CompactDone {" rust-agent-tui/src/app/events.rs`
   - ✅ 结构体变体包含 `summary: String` 和 `new_thread_id: String`
   - On failure: 检查 Task 1

4. RelayMessage CompactDone 变体验证
   - `grep -A6 "CompactDone {" rust-relay-server/src/protocol.rs`
   - ✅ 包含 `summary`、`new_thread_id`、`old_thread_id` 三个字段
   - On failure: 检查 Task 3

5. 前端事件处理验证
   - `grep -c "compact_done" rust-relay-server/web/events.js`
   - ✅ 1 处匹配
   - On failure: 检查 Task 4
