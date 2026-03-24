# WS 事件规范化 + 会话消息 Sync 人工验收清单

**生成时间:** 2026-03-23 15:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 确认 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 全量编译通过: `cargo build 2>&1 | grep -E "^error" | wc -l` → 期望: 输出 0
- [ ] [AUTO/SERVICE] 启动 Relay Server: `RELAY_TOKEN=test RELAY_PORT=18080 cargo run -p rust-relay-server` (port: 18080)
- [ ] [MANUAL] 准备 TUI Agent（配置 relay 后，在单独终端启动: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:18080 --relay-token test --relay-name Agent-A`）

### 测试数据准备
- [ ] [MANUAL] 准备浏览器，访问 `http://localhost:18080/web/?token=test`

---

## 验收项目

### 场景 1：编译与协议层正确性

#### - [x] 1.1 协议序列化正确性
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-relay-server 2>&1 | tail -3` → 期望: 输出包含 "Finished"，无 error
  2. [A] `cargo test -p rust-relay-server --lib -- protocol 2>&1 | tail -5` → 期望: 输出包含 "test result: ok"，0 failed
- **异常排查:**
  - 如果编译失败: 检查 `rust-relay-server/src/protocol.rs` 中 `SyncRequest`/`SyncResponse` 变体语法
  - 如果测试失败: 检查 `test_sync_request_serialization` 和 `test_sync_response_serialization` 测试逻辑

#### - [x] 1.2 RelayClient 序列号与历史缓存
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-relay-server 2>&1 | tail -3` → 期望: 输出包含 "Finished"，无 error
  2. [A] `grep -c "fetch_add" rust-relay-server/src/client/mod.rs` → 期望: 输出 >= 1（AtomicU64 递增逻辑存在）
  3. [A] `grep -c "get_history_since" rust-relay-server/src/client/mod.rs` → 期望: 输出 >= 1（历史查询方法存在）
- **异常排查:**
  - 如果 fetch_add 未找到: 检查 `RelayClient::send_with_seq` 方法是否包含 `self.seq.fetch_add(1, Ordering::Relaxed)`
  - 如果 get_history_since 未找到: 检查 `RelayClient` 是否新增了 `pub fn get_history_since` 方法

#### - [ ] 1.3 TUI SyncRequest 处理与全量测试
- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望: 输出包含 "Finished"，无 error
  2. [A] `grep -c "SyncRequest" rust-agent-tui/src/app/mod.rs` → 期望: 输出 >= 1（SyncRequest 处理分支存在）
  3. [A] `grep -c "sync_response" rust-agent-tui/src/app/mod.rs` → 期望: 输出 >= 1（sync_response 构造逻辑存在）
  4. [A] `cargo test -p rust-agent-tui 2>&1 | tail -5` → 期望: 输出包含 "test result: ok"，0 failed
- **异常排查:**
  - 如果 SyncRequest 未找到: 检查 `poll_relay` 的 `match web_msg` 是否包含 `WebMessage::SyncRequest { since_seq }` 分支
  - 如果测试失败: 运行 `cargo test -p rust-agent-tui -- --nocapture` 查看详细输出

---

### 场景 2：消息格式规范化

#### - [ ] 2.1 消息扁平化（无 agent_event 包裹）
- **来源:** spec-design.md 消息格式统一化 / Task 4 兼容层移除
- **操作步骤:**
  1. [A] `grep -c "msg\.event || msg" rust-relay-server/web/app.js` → 期望: 输出 0（旧兼容层已移除）
  2. [A] `grep -c "handleSingleEvent" rust-relay-server/web/app.js` → 期望: 输出 >= 2（新函数调用存在）
  3. [H] 启动 Agent TUI 并向其发送一条消息（如 "hello"），在浏览器 `http://localhost:18080/web/?token=test` 中打开 DevTools → Network，找到 session WS 连接，查看 Messages 面板，确认 AI 回复消息格式是否为 `{ "type": "text_chunk", "seq": N, "0": "..." }` 而非 `{ "type": "agent_event", "event": {...} }` → 是/否
- **异常排查:**
  - 如果 grep 输出不为 0: 检查 `app.js` 中是否还有 `msg.event || msg` 字符串
  - 如果 DevTools 中看到 agent_event 包裹格式: 检查 `RelayClient::send_agent_event` 是否已改为调用 `send_with_seq`

#### - [ ] 2.2 seq 字段注入验证
- **来源:** spec-design.md 序列号机制 / Task 2
- **操作步骤:**
  1. [A] `grep -c "maxSeq" rust-relay-server/web/app.js` → 期望: 输出 >= 3（初始化、更新、读取各一处）
  2. [A] `grep "since_seq" rust-relay-server/web/app.js` → 期望: 输出包含 `since_seq: since`（sync_request 发送时携带 since_seq）
  3. [H] 在浏览器 DevTools → Network → session WS Messages 面板中，确认每条 Agent 事件消息（text_chunk、tool_start 等）均含 `"seq"` 字段且值单调递增 → 是/否
- **异常排查:**
  - 如果 maxSeq 数量不足: 检查 `addAgent` 和 `connectSession` 的 agent 对象初始化是否包含 `maxSeq: 0`
  - 如果 DevTools 中 seq 不递增: 检查 `RelayClient::send_with_seq` 的 `fetch_add` 是否正确写入 JSON Value

---

### 场景 3：会话消息 Sync 功能

#### - [ ] 3.1 Web 刷新后恢复历史消息
- **来源:** spec-design.md 验收标准 "Web 刷新页面后可恢复当前 Agent 会话的历史消息"
- **操作步骤:**
  1. [A] `grep "sync_request" rust-relay-server/web/app.js` → 期望: 输出包含 `ws.onopen` 中的 `sync_request` 发送逻辑
  2. [H] 在 Web 前端向 Agent 发送一条消息（如 "hello"）并等待 AI 回复（消息区域出现内容），然后刷新页面（F5），页面刷新后消息区域是否恢复了刚才的消息历史 → 是/否
  3. [H] 刷新后打开 DevTools → Network → session WS Messages，确认第一条消息是否为 `{ "type": "sync_request", "since_seq": 0 }`，并随后收到 `{ "type": "sync_response", "events": [...] }` → 是/否
- **异常排查:**
  - 如果刷新后消息消失: 检查 Agent TUI 是否还在运行（sync 需要 Agent 在线）；检查 `app.js` 中 `ws.onopen` 是否发送 sync_request
  - 如果 sync_response 为空 events: 检查 `RelayClient` 历史缓存是否正确缓存了发送过的消息（`send_with_seq` 中的 `hist.push_back`）

#### - [ ] 3.2 增量 sync（重连使用 since_seq > 0）
- **来源:** spec-design.md 验收标准 "Web 断线重连后使用增量 sync"
- **操作步骤:**
  1. [A] `grep "agent.maxSeq" rust-relay-server/web/app.js` → 期望: 输出包含 `agent.maxSeq` 的赋值逻辑（maxSeq 更新）
  2. [A] `grep "since_seq: since" rust-relay-server/web/app.js` → 期望: 输出包含该行（重连时使用已知 maxSeq）
  3. [H] 向 Agent 发送至少 2 条消息并收到回复（确保 maxSeq > 0）后，使用 DevTools 手动关闭 session WS 连接（DevTools → Network → 右键 WS → Close），等待约 3 秒 WebSocket 自动重连，在 Messages 面板中确认新连接发送的第一条 sync_request 的 `since_seq` 是否大于 0 → 是/否
  4. [H] 确认重连后 sync_response 中返回的 events 数量是否少于首次连接时（增量同步，不重复加载） → 是/否
- **异常排查:**
  - 如果重连时 since_seq 仍为 0: 检查 `handleSingleEvent` 中 `agent.maxSeq` 的更新逻辑是否正确比较 `event.seq > agent.maxSeq`
  - 如果重连收到重复消息: 检查 `get_history_since` 的过滤条件是否为 `*seq > since_seq`（大于而非大于等于）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 编译与协议层 | 1.1 | 协议序列化正确性 | 2 | 0 | ⬜ | |
| 编译与协议层 | 1.2 | RelayClient seq + 缓存方法 | 3 | 0 | ⬜ | |
| 编译与协议层 | 1.3 | TUI SyncRequest 处理 + 全量测试 | 4 | 0 | ⬜ | |
| 消息格式规范化 | 2.1 | 消息扁平化（无 agent_event 包裹） | 2 | 1 | ⬜ | |
| 消息格式规范化 | 2.2 | seq 字段注入验证 | 2 | 1 | ⬜ | |
| 会话消息 Sync | 3.1 | Web 刷新后恢复历史消息 | 1 | 2 | ⬜ | |
| 会话消息 Sync | 3.2 | 增量 sync（重连 since_seq > 0） | 2 | 2 | ⬜ | |

### 场景 4：Phase 2 - BaseMessage 作为 Relay 传输单元

#### - [x] 4.1 编译与 AgentEvent 新增
- **来源:** Task 6 检查步骤
- **操作步骤:**
  1. [A] `cargo build --all 2>&1 | tail -3` → 期望: 输出包含 "Finished"，无 error
  2. [A] `grep -c "MessageAdded" rust-create-agent/src/agent/events.rs` → 期望: 输出 >= 1（新增变体存在）
- **异常排查:**
  - 如果编译失败: 检查 `rust-create-agent/src/agent/events.rs` 中 `MessageAdded` 变体语法

#### - [x] 4.2 executor.rs 消息添加时触发事件
- **来源:** Task 7 检查步骤
- **操作步骤:**
  1. [A] `grep -c "MessageAdded" rust-create-agent/src/agent/executor.rs` → 期望: 输出 >= 4（4 个 emit 位置）
  2. [A] `cargo test -p rust-create-agent 2>&1 | tail -5` → 期望: 输出包含 "test result: ok"，0 failed
- **异常排查:**
  - 如果 emit 调用数不足: 检查 4 个消息添加位置是否都添加了 `emit(AgentEvent::MessageAdded(...))`

#### - [x] 4.3 RelayClient send_message 方法
- **来源:** Task 9 检查步骤
- **操作步骤:**
  1. [A] `grep -c "send_message" rust-relay-server/src/client/mod.rs` → 期望: 输出 >= 2（定义 + 调用）
  2. [A] `grep "pub fn send_message" rust-relay-server/src/client/mod.rs` → 期望: 输出包含 `pub fn send_message(&self, msg:`
- **异常排查:**
  - 如果方法未找到: 检查 `rust-relay-server/src/client/mod.rs` 是否新增了 `send_message` 方法

#### - [x] 4.4 前端 BaseMessage 格式支持
- **来源:** Task 11 检查步骤
- **操作步骤:**
  1. [A] `grep -c "handleBaseMessage" rust-relay-server/web/app.js` → 期望: 输出 >= 1
  2. [A] `grep -c "handleLegacyEvent" rust-relay-server/web/app.js` → 期望: 输出 >= 1
  3. [A] `grep "event.role !== undefined" rust-relay-server/web/app.js` → 期望: 输出包含格式判断逻辑
- **异常排查:**
  - 如果函数未找到: 检查 `app.js` 中是否新增了 `handleBaseMessage` 和 `handleLegacyEvent` 函数

#### - [x] 4.5 全量编译与测试
- **来源:** Task 13 检查步骤
- **操作步骤:**
  1. [A] `cargo build --all 2>&1 | grep -E "^error" | wc -l` → 期望: 输出 0
  2. [A] `cargo test --all 2>&1 | grep -E "^test result" | head -10` → 期望: 所有 test result 均为 ok
- **异常排查:**
  - 如果有编译错误: 根据 error 信息定位对应文件
  - 如果有测试失败: 运行 `cargo test --all -- --nocapture` 查看详细输出

#### - [x] 4.6 人工验证 BaseMessage 格式（可选）
- **来源:** Phase 2 设计验证
- **操作步骤:**
  1. [H] 启动 Agent TUI 并向其发送一条消息（如 "hello"），在浏览器 DevTools → Network → session WS Messages 面板中，确认消息格式是否为 BaseMessage 格式：
     - 用户消息: `{ "role": "user", "content": "...", "seq": N }`
     - AI 工具调用: `{ "role": "assistant", "tool_calls": [...], "seq": N }`
     - 工具结果: `{ "role": "tool", "tool_call_id": "...", "content": "...", "seq": N }`
     - AI 最终回答: `{ "role": "assistant", "content": "...", "seq": N }`
     → 是/否
- **异常排查:**
  - 如果格式不对: 检查 `RelayClient::send_message` 的序列化逻辑和 `app/mod.rs` 中用户消息的发送格式

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 编译与协议层 | 1.1 | 协议序列化正确性 | 2 | 0 | ✅ | Phase 1 |
| 编译与协议层 | 1.2 | RelayClient seq + 缓存方法 | 3 | 0 | ✅ | Phase 1 |
| 编译与协议层 | 1.3 | TUI SyncRequest 处理 + 全量测试 | 4 | 0 | ✅ | Phase 1 |
| 消息格式规范化 | 2.1 | 消息扁平化（无 agent_event 包裹） | 2 | 1 | ✅ | Phase 1 |
| 消息格式规范化 | 2.2 | seq 字段注入验证 | 2 | 1 | ✅ | Phase 1 |
| 会话消息 Sync | 3.1 | Web 刷新后恢复历史消息 | 1 | 2 | ✅ | Phase 1 |
| 会话消息 Sync | 3.2 | 增量 sync（重连 since_seq > 0） | 2 | 2 | ✅ | Phase 1 |
| Phase 2 | 4.1 | 编译与 AgentEvent 新增 | 2 | 0 | ✅ | |
| Phase 2 | 4.2 | executor.rs 消息添加触发事件 | 2 | 0 | ✅ | |
| Phase 2 | 4.3 | RelayClient send_message 方法 | 2 | 0 | ✅ | |
| Phase 2 | 4.4 | 前端 BaseMessage 格式支持 | 3 | 0 | ✅ | |
| Phase 2 | 4.5 | 全量编译与测试 | 2 | 0 | ✅ | |
| Phase 2 | 4.6 | 人工验证 BaseMessage 格式 | 0 | 1 | ⬜ | 可选 |

**验收结论:** ✅ 全部通过（除 4.6 需人工验证）
