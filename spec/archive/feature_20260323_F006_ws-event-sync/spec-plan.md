# WS 事件规范化 + 会话消息 Sync 执行计划

**目标:**
- Phase 1: 统一 WebSocket 事件格式（扁平化 + seq），并实现 Web 端首次连接/重连时从 Agent 侧拉取历史消息
- Phase 2: Relay 传输单元改为 BaseMessage，前端根据消息语义渲染

**技术栈:** Rust / tokio / serde_json / AtomicU64 / VecDeque / JavaScript (app.js)

**设计文档:** ./spec-design.md

---

### Task 1: 协议层扩展（protocol.rs）

**涉及文件:**
- 修改: `rust-relay-server/src/protocol.rs`

**执行步骤:**
- [x] 在 `WebMessage` 枚举中新增 `SyncRequest { since_seq: u64 }` 变体
  - serde tag: `sync_request`
- [x] 在 `RelayMessage` 枚举中新增 `SyncResponse { events: Vec<serde_json::Value> }` 变体
  - serde tag: `sync_response`
- [x] 保留 `RelayMessage::AgentEvent` 变体（向后兼容），但在注释中标注"deprecated，不再主动发送"
- [x] 在 `#[cfg(test)]` 测试模块中补充序列化/反序列化测试
  - `WebMessage::SyncRequest { since_seq: 42 }` → 含 `"type":"sync_request"` 和 `"since_seq":42`
  - `RelayMessage::SyncResponse { events: vec![] }` → 含 `"type":"sync_response"`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-relay-server 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] 序列化测试通过
  - `cargo test -p rust-relay-server --lib -- protocol 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failed

---

### Task 2: RelayClient 序列号 + 历史缓存（client/mod.rs）

**涉及文件:**
- 修改: `rust-relay-server/src/client/mod.rs`

**执行步骤:**
- [x] 在 `RelayClient` 结构体新增两个字段：
  - `seq: Arc<AtomicU64>` — 序列号计数器，初始值 0
  - `history: Arc<std::sync::Mutex<std::collections::VecDeque<(u64, String)>>>` — 最多 1000 条
- [x] 新增私有方法 `fn send_with_seq(&self, mut val: serde_json::Value)`：
  - `seq.fetch_add(1, Ordering::Relaxed)` 获取并递增序列号
  - 注入 `val["seq"] = seq` 到 JSON Value
  - 加入历史缓存（超 1000 条时 `pop_front`）
  - 通过 `self.tx.send(json)` 发送
- [x] 修改 `send_agent_event`：将 `AgentEvent` 序列化为 `serde_json::Value`，调用 `send_with_seq`
  - 不再序列化为 `RelayMessage::AgentEvent { event }` 包裹
- [x] 修改 `send_raw`：接受 `serde_json::Value` 而非 `&str`（或新增 `send_value` 方法），注入 seq 并缓存
  - 注意：`ApprovalNeeded`、`AskUserBatch`、`TodoUpdate` 通过此路径发送，也需要 seq + 缓存
- [x] 新增公共方法 `pub fn get_history_since(&self, since_seq: u64) -> Vec<String>`：
  - 返回所有 `seq > since_seq` 的缓存 JSON 字符串
- [x] 更新 `RelayClient::connect` 构造函数，初始化新字段

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-relay-server 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] seq 递增逻辑可验证（通过单元测试或 grep）
  - `grep -c "fetch_add" rust-relay-server/src/client/mod.rs`
  - 预期: 输出 >= 1
- [x] get_history_since 方法存在
  - `grep -c "get_history_since" rust-relay-server/src/client/mod.rs`
  - 预期: 输出 >= 1

---

### Task 3: TUI poll_relay 处理 SyncRequest（app/mod.rs）

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 在 `poll_relay` 方法的 `match web_msg` 中新增 `WebMessage::SyncRequest { since_seq }` 分支：
  ```rust
  WebMessage::SyncRequest { since_seq } => {
      if let Some(ref relay) = self.relay_client {
          let events = relay.get_history_since(since_seq);
          let response = serde_json::json!({
              "type": "sync_response",
              "events": events.iter()
                  .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap_or_default())
                  .collect::<Vec<_>>()
          });
          relay.send_value(response);  // 使用新的 send_value 方法（不注入 seq，不缓存）
      }
  }
  ```
- [x] 修改 `poll_agent` 中转发 `ApprovalNeeded`/`AskUserBatch`/`TodoUpdate` 的代码，改为调用 `relay.send_value(serde_json::to_value(&msg).unwrap())` 使 seq 通过 RelayClient 统一注入
  - 需确保 protocol.rs 中相关 `RelayMessage` 变体可被序列化为扁平格式（或在 app/mod.rs 直接构造扁平 JSON Value）
- [x] 新增 `send_value` 为 `RelayClient` 的公共方法（若 Task 2 未实现）：接受 `serde_json::Value`，注入 seq，缓存，发送

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] SyncRequest 处理分支存在
  - `grep -c "SyncRequest" rust-agent-tui/src/app/mod.rs`
  - 预期: 输出 >= 1
- [x] sync_response 构造逻辑存在
  - `grep -c "sync_response" rust-agent-tui/src/app/mod.rs`
  - 预期: 输出 >= 1
- [x] 测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failed

---

### Task 4: Web 端 app.js 接入 Sync 机制

**涉及文件:**
- 修改: `rust-relay-server/web/app.js`

**执行步骤:**
- [x] 在 `agents` Map 的 agent 对象中新增 `maxSeq: 0` 字段（`addAgent` 和 `connectSession` 两处）
- [x] 修改 `connectSession` 中 `ws.onopen`，发送 `sync_request`：
  ```javascript
  ws.onopen = () => {
    const agent = agents.get(sessionId);
    const since = agent ? agent.maxSeq : 0;
    ws.send(JSON.stringify({ type: 'sync_request', since_seq: since }));
  };
  ```
- [x] 在 `handleAgentEvent` 中新增 `sync_response` 处理分支：
  ```javascript
  case 'sync_response':
    (msg.events || []).forEach(ev => handleSingleEvent(sessionId, ev));
    renderMessages();
    renderTodoPanel();
    break;
  ```
- [x] 将 `handleAgentEvent` 中现有的 switch 逻辑拆分为内部函数 `handleSingleEvent(sessionId, event)`，以便 sync_response 批量调用
- [x] 在 `handleSingleEvent` 中，更新 `maxSeq`：
  ```javascript
  if (event.seq && event.seq > agent.maxSeq) agent.maxSeq = event.seq;
  ```
- [x] 移除兼容层：将 `const event = msg.event || msg;` 改为直接 `const event = msg;`（消息已扁平化）
- [x] Agent 重连（`addAgent` 中同名 Agent 复用逻辑）时，新 session 的 `ws.onopen` 应使用 `old.maxSeq` 作为 `since_seq`

**检查步骤:**
- [x] sync_request 发送逻辑存在
  - `grep -c "sync_request" rust-relay-server/web/app.js`
  - 预期: 输出 >= 1
- [x] sync_response 处理逻辑存在
  - `grep -c "sync_response" rust-relay-server/web/app.js`
  - 预期: 输出 >= 1
- [x] maxSeq 字段初始化存在
  - `grep -c "maxSeq" rust-relay-server/web/app.js`
  - 预期: 输出 >= 3（初始化、更新、读取各一处）
- [x] 兼容层已移除
  - `grep -c "msg\.event || msg" rust-relay-server/web/app.js`
  - 预期: 输出 0

---

### Task 5: WS Event Sync Acceptance

**Prerequisites:**
- Start Relay Server: `RELAY_TOKEN=test RELAY_PORT=18080 cargo run -p rust-relay-server`
- Start Agent TUI with relay: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:18080 --relay-token test --relay-name Agent-A`
- Open browser: `http://localhost:18080/web/?token=test`

**End-to-end verification:**

1. ✓ 验证实时事件携带 seq 字段
   - `cargo build -p rust-relay-server 2>&1 | tail -1`
   - Expected: 编译无 error（通过编译验证 seq 注入逻辑存在）
   - On failure: check Task 2 RelayClient send_with_seq 实现

2. ✓ 验证 sync_request 发送逻辑
   - `grep "sync_request" rust-relay-server/web/app.js`
   - Expected: 输出包含 `ws.send` 和 `sync_request`
   - On failure: check Task 4 connectSession onopen 实现

3. ✓ 验证 sync_response 处理逻辑
   - `grep "sync_response" rust-relay-server/web/app.js`
   - Expected: 输出包含 `case 'sync_response'`
   - On failure: check Task 4 handleAgentEvent 扩展

4. ✓ 验证历史缓存方法存在
   - `grep "get_history_since" rust-relay-server/src/client/mod.rs`
   - Expected: 输出包含 `pub fn get_history_since`
   - On failure: check Task 2 RelayClient 新增方法

5. ✓ 验证 TUI poll_relay 处理 SyncRequest
   - `grep "SyncRequest" rust-agent-tui/src/app/mod.rs`
   - Expected: 输出包含 `WebMessage::SyncRequest`
   - On failure: check Task 3 poll_relay 分支

6. ✓ 全量编译无 error
   - `cargo build 2>&1 | grep -E "^error" | wc -l`
   - Expected: 输出 0
   - On failure: 根据 error 信息定位对应 Task

---

### Task 6: Phase 2 - 新增 MessageAdded 事件（events.rs）

**涉及文件:**
- 修改: `rust-create-agent/src/agent/events.rs`

**执行步骤:**
- [x] 在 `AgentEvent` 枚举中新增 `MessageAdded(crate::messages::BaseMessage)` 变体

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error

---

### Task 7: Phase 2 - executor.rs 发送 MessageAdded 事件

**涉及文件:**
- 修改: `rust-create-agent/src/agent/executor.rs`

**执行步骤:**
- [x] 在用户输入位置（约 107 行）：`state.add_message(human_msg)` 后调用 `self.emit(AgentEvent::MessageAdded(human_msg))`
- [x] 在 LLM 工具调用推理位置（约 167 行）：`state.add_message(ai_msg)` 后调用 `self.emit(AgentEvent::MessageAdded(ai_msg_clone))`
- [x] 在工具执行结果位置（约 290 行）：`state.add_message(tool_msg)` 后调用 `self.emit(AgentEvent::MessageAdded(tool_msg_clone))`
- [x] 在 LLM 最终回答位置（约 331 行）：`state.add_message(ai_msg)` 后调用 `self.emit(AgentEvent::MessageAdded(ai_msg_clone))`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] 全量测试通过
  - `cargo test -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failed

---

### Task 8: Phase 2 - protocol.rs 新增 MessageBatch 变体

**涉及文件:**
- 修改: `rust-relay-server/src/protocol.rs`

**执行步骤:**
- [x] 在 `RelayMessage` 枚举中新增 `MessageBatch { messages: Vec<serde_json::Value> }` 变体
- [x] 新增 `test_message_batch_serialization` 测试

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-relay-server 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] 序列化测试通过
  - `cargo test -p rust-relay-server --lib -- protocol 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failed

---

### Task 9: Phase 2 - RelayClient 新增 send_message 方法

**涉及文件:**
- 修改: `rust-relay-server/src/client/mod.rs`

**执行步骤:**
- [x] 新增 `pub fn send_message(&self, msg: &rust_create_agent::messages::BaseMessage)` 方法
  - 将 BaseMessage 序列化为 JSON Value
  - 调用 `send_with_seq` 注入 seq 并发送

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-relay-server 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error

---

### Task 10: Phase 2 - agent.rs 处理 MessageAdded 事件

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 修改事件回调 `FnEventHandler`：
  - `ExecutorEvent::MessageAdded(msg)` → 调用 `relay.send_message(msg)` 转发到 relay
  - TUI 渲染继续使用已有 `AgentEvent::ToolCall`/`AssistantChunk` 等事件
  - 其他 `ExecutorEvent` 变体调用 `relay.send_agent_event(&event)`（兼容保留）

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error

---

### Task 11: Phase 2 - 前端 app.js 支持 BaseMessage 格式

**涉及文件:**
- 修改: `rust-relay-server/web/app.js`

**执行步骤:**
- [x] 将 `handleSingleEvent` 拆分为两个函数：
  - `handleBaseMessage(agent, event)` — 处理 BaseMessage 格式（`role` 字段）
  - `handleLegacyEvent(agent, event)` — 处理旧 AgentEvent 格式（`type` 字段）
- [x] `handleSingleEvent` 入口函数根据 `event.role` 是否存在判断格式并分发
- [x] `handleBaseMessage` 实现：
  - `role: "user"` → 添加 user 消息
  - `role: "assistant"` → 处理 tool_calls 和 text，更新工具消息的 output
  - `role: "tool"` → 查找对应 tool 消息并更新 output
  - `role: "system"` → 暂不显示
- [x] 保留 `handleLegacyEvent` 兼容旧 AgentEvent 格式

**检查步骤:**
- [x] grep 验证 `handleBaseMessage` 函数存在
  - `grep -c "handleBaseMessage" rust-relay-server/web/app.js`
  - 预期: 输出 >= 1
- [x] grep 验证 `handleLegacyEvent` 函数存在
  - `grep -c "handleLegacyEvent" rust-relay-server/web/app.js`
  - 预期: 输出 >= 1

---

### Task 12: Phase 2 - TUI poll_relay 用户消息改为 BaseMessage 格式

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 修改 `WebMessage::UserInput` 处理：将 relay 发送的用户消息格式从 `{ "type": "user_message", "text": ... }` 改为 `{ "role": "user", "content": ... }`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error

---

### Task 13: Phase 2 Acceptance

**最终验证:**
- [x] 全量编译无 error
  - `cargo build 2>&1 | grep -E "^error" | wc -l`
  - Expected: 输出 0
- [x] 全量测试通过
  - `cargo test 2>&1 | grep -E "^test result" | head -10`
  - Expected: 所有 test result 均为 ok
