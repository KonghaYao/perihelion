# relay-command-sync 执行计划

**目标:** 实现 Web 前端命令发送（/clear、/compact）及 Agent 侧 thread 状态变更自动同步到 Web

**技术栈:** Rust (serde_json, tokio), JavaScript (ES Modules)

**设计文档:** ./spec-design.md

---

### Task 1: 协议扩展

**涉及文件:**
- 修改: `rust-relay-server/src/protocol.rs`

**执行步骤:**
- [x] 在 `WebMessage` 枚举中新增 `CompactThread` 变体（无字段）
  - 位置：`WebMessage::SyncRequest` 变体之后
  - 序列化形式：`{"type":"compact_thread"}`
- [x] 在 `RelayMessage` 枚举中新增 `ThreadReset` 变体
  - 字段：`messages: Vec<serde_json::Value>`（BaseMessage JSON 数组，空表示清空）
  - 序列化形式：`{"type":"thread_reset","messages":[...]}`
- [x] 在 `mod tests` 中新增两个序列化/反序列化测试
  - `test_compact_thread_serialization`：验证 `CompactThread` 序列化为 `{"type":"compact_thread"}`
  - `test_thread_reset_serialization`：验证 `ThreadReset{messages:vec![]}` 序列化包含 `"type":"thread_reset"` 和 `"messages":[]`

**检查步骤:**
- [x] 协议序列化测试通过
  - `cargo test -p rust-relay-server --lib -- protocol::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok` 且无 FAILED
- [x] 新增变体序列化结果正确
  - `cargo test -p rust-relay-server --lib -- test_compact_thread 2>&1 | tail -5`
  - 预期: `test protocol::tests::test_compact_thread_serialization ... ok`

---

### Task 2: RelayClient 新增 send_thread_reset

**涉及文件:**
- 修改: `rust-relay-server/src/client/mod.rs`

**执行步骤:**
- [x] 在 `RelayClient` 的 `impl` 块末尾添加 `send_thread_reset` 方法
  - 签名：`pub fn send_thread_reset(&self, messages: &[rust_create_agent::messages::BaseMessage])`
  - 实现：将每条 BaseMessage 序列化为 `serde_json::Value`，构造 `{"type":"thread_reset","messages":[...]}` JSON
  - 调用 `self.send_raw(&s)` 发送（不注入 seq，不进历史缓存）
  - 若 `connected` 为 false 则静默跳过（与其他方法保持一致）

**检查步骤:**
- [x] client/mod.rs 编译通过
  - `cargo build -p rust-relay-server 2>&1 | tail -5`
  - 预期: 无 error，最多有 warning
- [x] send_thread_reset 方法存在
  - `grep -n "send_thread_reset" rust-relay-server/src/client/mod.rs`
  - 预期: 至少一行包含 `pub fn send_thread_reset`

---

### Task 3: Agent relay_ops.rs 改动

**涉及文件:**
- 修改: `rust-agent-tui/src/app/relay_ops.rs`

**执行步骤:**
- [x] 在 `WebMessage::ClearThread` 分支中，`relay.clear_history()` 调用之后、`self.new_thread()` 调用之前，追加 `relay.send_thread_reset(&[])`
  - 注意：`relay` 来自 `if let Some(ref relay) = self.relay_client` 的借用；`send_thread_reset` 接受 `&[BaseMessage]`，传入空切片 `&[]`
- [x] 在 `poll_relay` 的 `for web_msg in events` 循环中，新增 `WebMessage::CompactThread` 分支
  - 实现：`WebMessage::CompactThread => { self.start_compact(String::new()); }`
  - 位置：紧接 `WebMessage::ClearThread` 分支之后

**检查步骤:**
- [x] relay_ops.rs 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 error
- [x] ClearThread 分支包含 send_thread_reset 调用
  - `grep -A 10 "WebMessage::ClearThread" rust-agent-tui/src/app/relay_ops.rs`
  - 预期: 输出中包含 `send_thread_reset`
- [x] CompactThread 分支存在
  - `grep -n "CompactThread\|start_compact" rust-agent-tui/src/app/relay_ops.rs`
  - 预期: 至少两行，分别包含 `CompactThread` 和 `start_compact`

---

### Task 4: Agent thread_ops.rs + agent_ops.rs 状态变更通知

**涉及文件:**
- 修改: `rust-agent-tui/src/app/thread_ops.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] **thread_ops.rs `new_thread()`**：在 `self.thread_browser = None; self.langfuse_session = None;` 之后追加
  ```rust
  if let Some(ref relay) = self.relay_client {
      relay.send_thread_reset(&[]);
  }
  ```
  - 说明：TUI 侧 `/clear` 命令直接调用 `new_thread()`，需通知 Web 前端清空
- [x] **thread_ops.rs `open_thread()`**：在 `self.current_thread_id = Some(thread_id);` 之后追加
  ```rust
  if let Some(ref relay) = self.relay_client {
      relay.clear_history();
      relay.send_thread_reset(&base_msgs);
  }
  ```
  - 说明：切换历史前清空 relay 历史缓存，再推送完整历史消息；`base_msgs` 是已加载的历史 BaseMessage 列表
- [x] **agent_ops.rs `AgentEvent::CompactDone`**：在 `self.agent_rx = None;` 之后、`pending_messages` 处理之前追加
  ```rust
  if let Some(ref relay) = self.relay_client {
      relay.send_thread_reset(&self.agent_state_messages);
  }
  ```
  - 说明：compact 完成后 `agent_state_messages` 已替换为摘要，将新状态推送给 Web 前端

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 error
- [x] new_thread 中含 send_thread_reset
  - `grep -A 15 "pub fn new_thread" rust-agent-tui/src/app/thread_ops.rs | grep "send_thread_reset"`
  - 预期: 至少一行匹配
- [x] open_thread 中含 send_thread_reset 和 clear_history
  - `grep -A 50 "pub fn open_thread" rust-agent-tui/src/app/thread_ops.rs | grep -E "send_thread_reset|clear_history"`
  - 预期: 两行分别匹配
- [x] CompactDone 中含 send_thread_reset
  - `grep -A 60 "AgentEvent::CompactDone" rust-agent-tui/src/app/agent_ops.rs | grep "send_thread_reset"`
  - 预期: 至少一行匹配

---

### Task 5: Web 前端 render.js + events.js

**涉及文件:**
- 修改: `rust-relay-server/web/js/render.js`
- 修改: `rust-relay-server/web/js/events.js`

**执行步骤:**
- [x] **render.js `doSend()` 函数**：在 `if (text === '/clear')` 分支与 `else { sendMessage(...user_input...) }` 之间新增 `/compact` 分支
  - 当前代码中 doSend 位于 `renderPane` 函数内（约第 335-348 行）
  - 新增：`else if (text === '/compact') { sendMessage(sessionId, { type: 'compact_thread' }); }`
  - 确认 `/clear` 分支已有 `sendMessage(sessionId, { type: 'clear_thread' })` + 本地清空逻辑；若缺失则补全
- [x] **events.js `handleLegacyEvent()` 函数**：在 `case 'ask_user_resolved':` 分支之后新增 `thread_reset` case
  ```js
  case 'thread_reset': {
      agent.messages = [];
      agent.maxSeq = 0;
      (event.messages || []).forEach(m => handleBaseMessage(agent, m));
      break;
  }
  ```
  - `messages` 为空时仅清空；有内容时通过 `handleBaseMessage` 逐条重建（复用已有 BaseMessage 处理逻辑）

**检查步骤:**
- [x] render.js 包含 compact_thread 发送
  - `grep -n "compact_thread" rust-relay-server/web/js/render.js`
  - 预期: 包含 `type: 'compact_thread'`
- [x] render.js 包含 clear_thread 完整处理（命令 + 本地清空）
  - `grep -n "clear_thread\|agent\.messages = \[\]" rust-relay-server/web/js/render.js`
  - 预期: 至少两行，分别包含 `clear_thread` 和 `agent.messages = []`
- [x] events.js 包含 thread_reset 处理
  - `grep -n "thread_reset\|handleBaseMessage" rust-relay-server/web/js/events.js`
  - 预期: 至少两行匹配
- [x] relay-server 含前端文件的构建通过（前端通过 include_bytes! 打包）
  - `cargo build -p rust-relay-server --features server 2>&1 | tail -5`
  - 预期: 无 error

---

### Task 6: relay-command-sync Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-relay-server --features server`（默认监听 8080）
- Agent TUI 连接命令: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token <token> --relay-name test-agent`
- 浏览器访问: `http://localhost:8080/web/?token=<token>`

**端到端验证:**

1. 全量构建无错误
   - `cargo build --workspace 2>&1 | grep -E "^error" | wc -l`
   - Expected: 输出 `0`（无 error 行）
   - On failure: 检查 Task 1-4 各文件编译

2. 协议序列化测试全部通过
   - `cargo test -p rust-relay-server --lib 2>&1 | tail -5`
   - Expected: `test result: ok. N passed; 0 failed`
   - On failure: 检查 Task 1（protocol.rs 测试用例）

3. WebMessage::CompactThread 能正确反序列化
   - `cargo test -p rust-relay-server --lib -- test_compact_thread 2>&1 | tail -3`
   - Expected: `test protocol::tests::test_compact_thread_serialization ... ok`
   - On failure: 检查 Task 1（CompactThread 变体定义）

4. ThreadReset 能正确序列化
   - `cargo test -p rust-relay-server --lib -- test_thread_reset 2>&1 | tail -3`
   - Expected: `test protocol::tests::test_thread_reset_serialization ... ok`
   - On failure: 检查 Task 1（ThreadReset 变体定义）

5. 前端命令处理代码完整性检查
   - `grep -c "compact_thread\|clear_thread\|thread_reset" rust-relay-server/web/js/render.js rust-relay-server/web/js/events.js`
   - Expected: 每个文件至少有 1 行匹配（render.js ≥2，events.js ≥1）
   - On failure: 检查 Task 5（前端文件）

6. Agent 侧 ThreadReset 发送点覆盖检查
   - `grep -rn "send_thread_reset" rust-agent-tui/src/app/`
   - Expected: 至少 4 行匹配（relay_ops.rs×1，thread_ops.rs×2，agent_ops.rs×1）
   - On failure: 检查 Task 3-4 对应文件
