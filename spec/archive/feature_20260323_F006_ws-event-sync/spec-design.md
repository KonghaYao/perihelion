# Feature: 20260323_F006 - ws-event-sync

## 需求背景

F004（远程控制访问）实现后，WebSocket 协议存在以下问题：

1. **消息格式不统一**：`RelayMessage::AgentEvent` 产生双层嵌套 `{ "type": "agent_event", "event": { "type": "text_chunk", ... } }`，Web 端需 `msg.event || msg` 兼容层
2. **无会话 Sync 机制**：Web 客户端刷新页面或断线重连后，无法恢复已产生的历史消息
3. **无事件序列号**：无法检测消息丢失，也无法支持增量同步
4. **Agent 重连后历史丢失**：同名 Agent 重连时，Web 端保留旧消息但新连接无法提供历史验证

## 目标

- **Phase 1：消息格式扁平化**：所有从 Relay 发往 Web 的事件消息统一为 `{ "type": "<event_type>", "seq": N, ...字段 }`，消除双层嵌套
- **Phase 1：引入序列号机制**：每个 Agent 会话的事件按递增 `seq` 标记，支持增量同步
- **Phase 1：Web 主动拉取历史**：Web 端连接 session WS 时自动发送 `sync_request`，从 Agent 侧拉取历史消息
- **Phase 1：Agent 断线后历史可恢复**：Agent 重连时已有历史消息由内存缓存保留，Web 可增量拉取
- **Phase 2：Relay 传输最小单元改为 BaseMessage**：relay 传输的不再是视图层事件（text_chunk/tool_start/tool_end），而是语义层消息（Human/Ai/System/Tool），前端自行决定渲染方式

## 方案设计

### 架构总览

![WebSocket 事件 Sync 数据流](./images/01-flow.png)

整体数据流如下：

1. Agent 产生 `AgentEvent` → `RelayClient` 扁平化序列化 + 注入 `seq` → 发往 Relay → 转发给所有 Web session 订阅者
2. Web 首次连接 session WS → 自动发送 `sync_request { since_seq: 0 }` → Relay 转发给 Agent → Agent 返回 `sync_response { events: [...] }` → Web 批量回放历史
3. Web 重连时使用 `since_seq = 当前最大已知 seq`，实现增量同步

---

### 消息格式统一化

#### 旧格式（废弃）

```json
// Agent 发往 Web — 双层嵌套
{ "type": "agent_event", "event": { "type": "text_chunk", "0": "hello" } }
```

#### 新格式（统一）

```json
// Agent 发往 Web — 扁平化 + seq
{ "type": "text_chunk", "seq": 42, "0": "hello" }
{ "type": "tool_start", "seq": 43, "name": "bash", "input": "ls" }
{ "type": "approval_needed", "seq": 44, "items": [...] }
{ "type": "todo_update", "seq": 45, "items": [...] }
```

**规则：**
- 所有 `AgentEvent`、`ApprovalNeeded`、`AskUserBatch`、`TodoUpdate` 消息携带 `seq`
- 协议类消息（`session_id`、`ping`、广播消息 `agent_online/offline` 等）**不携带** `seq`
- `sync_response` 本身不携带 `seq`，但其内嵌的 events 数组中每条消息携带原始 `seq`

---

### 协议变更

#### 新增 `WebMessage` 变体

```rust
// Web → Relay → Agent
pub enum WebMessage {
    // ...现有变体保持不变...
    SyncRequest {
        since_seq: u64,  // Web 当前已知最大 seq，初次连接填 0
    },
}
```

#### 新增 `RelayMessage` 变体（Agent → Relay → Web）

```rust
pub enum RelayMessage {
    // ...现有变体保持不变...
    SyncResponse {
        events: Vec<serde_json::Value>,  // 原始扁平化事件 JSON，含 seq 字段
    },
}
```

**注意**：`RelayMessage::AgentEvent` 变体**废弃**，不再用于实时推送。实时事件直接以扁平化 JSON 发送（不封装为 RelayMessage 枚举）。

---

### Agent 侧：RelayClient 变更

`rust-relay-server/src/client/mod.rs` 中 `RelayClient` 新增：

```rust
pub struct RelayClient {
    tx: mpsc::UnboundedSender<String>,
    pub session_id: Arc<tokio::sync::RwLock<Option<String>>>,
    connected: Arc<AtomicBool>,
    _tasks: Vec<JoinHandle<()>>,
    // 新增
    seq: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<(u64, String)>>>,  // (seq, event_json)
}
```

**序列号分配与历史缓存：**

```rust
fn send_with_seq(&self, event_json: serde_json::Value) {
    let seq = self.seq.fetch_add(1, Ordering::Relaxed);
    let mut val = event_json;
    if let Some(obj) = val.as_object_mut() {
        obj.insert("seq".into(), seq.into());
    }
    let json = serde_json::to_string(&val).unwrap_or_default();

    // 缓存（最多 1000 条，超出丢弃最旧）
    let mut hist = self.history.lock().unwrap();
    if hist.len() >= 1000 { hist.pop_front(); }
    hist.push_back((seq, json.clone()));
    drop(hist);

    let _ = self.tx.send(json);
}

pub fn get_history_since(&self, since_seq: u64) -> Vec<String> {
    let hist = self.history.lock().unwrap();
    hist.iter()
        .filter(|(seq, _)| *seq > since_seq)
        .map(|(_, json)| json.clone())
        .collect()
}
```

**`send_agent_event` 改为扁平化：**

```rust
pub fn send_agent_event(&self, event: &AgentEvent) {
    if !self.connected.load(Ordering::Relaxed) { return; }
    if let Ok(val) = serde_json::to_value(event) {
        self.send_with_seq(val);
    }
}
```

**`send_raw` 保持原有行为（用于 ApprovalNeeded / TodoUpdate 等，这些消息已是扁平结构，也注入 seq）。**

---

### Relay 侧：`handle_web_session_ws` 变更

收到 Web 发送的 `sync_request` 时，透传给 Agent：

```rust
Message::Text(text) => {
    // 透传所有 WebMessage 给 Agent（含 sync_request）
    let _ = entry.agent_tx.send(text.to_string());
}
```

（Relay 无需感知 `sync_request` 语义，保持透明转发即可）

---

### TUI 侧：`poll_relay` 处理 SyncRequest

`rust-agent-tui/src/app/mod.rs` 中 `poll_relay` 新增对 `WebMessage::SyncRequest` 的处理：

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
        if let Ok(json) = serde_json::to_string(&response) {
            relay.send_raw(&json);
        }
    }
}
```

---

### Web 端：`app.js` 变更

#### 1. session WS 连接时自动发送 `sync_request`

```javascript
ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'sync_request', since_seq: 0 }));
};
```

#### 2. `handleAgentEvent` 消除兼容层

```javascript
function handleAgentEvent(sessionId, msg) {
    // 旧: const event = msg.event || msg;
    // 新: 直接使用 msg（格式已扁平化）
    const eventType = msg.type;
    // ...
}
```

#### 3. 处理 `sync_response`

```javascript
case 'sync_response':
    (msg.events || []).forEach(ev => handleAgentEvent(sessionId, ev));
    renderMessages();
    renderTodoPanel();
    break;
```

#### 4. 记录已知最大 seq（用于重连时增量 sync）

```javascript
// agents map 中的 agent 对象新增字段
{ ..., maxSeq: 0 }

// handleAgentEvent 中更新
if (msg.seq && msg.seq > agent.maxSeq) agent.maxSeq = msg.seq;

// 重连时 onopen
ws.send(JSON.stringify({ type: 'sync_request', since_seq: agent.maxSeq }));
```

---

### 数据流时序图

```
Web 首次连接                Agent                 Relay
    |                          |                     |
    |------ session WS open -->|                     |
    |<-- (Relay 建立连接) ----->|                     |
    |--- sync_request(0) ----->|--- 透传 ----------->|
    |                          |<-- SyncRequest(0) --|
    |                          |-- sync_response --->|
    |<-- sync_response --------|                     |
    |  (batch 回放历史事件)    |                     |
    |                          |                     |
    |       [实时事件流]        |                     |
    |<-- { type, seq, ... } ---|                     |
```

## 实现要点

1. **AtomicU64 序列号**：使用 `Ordering::Relaxed` 即可（单 Agent 单调递增，无跨线程竞争要求）
2. **历史缓存上限**：默认 1000 条，防止内存无限增长；超限时丢弃最旧条目（VecDeque pop_front）
3. **`RelayMessage::AgentEvent` 废弃**：为向后兼容，保留该枚举变体但不再发送；实时事件改为直接发送扁平 JSON
4. **`send_raw` 的 seq 注入**：`ApprovalNeeded`、`AskUserBatch`、`TodoUpdate` 通过 `send_raw` 发送时，也需先 parse 为 Value 并注入 seq，再缓存。考虑将 `send_raw` 改为接收 `serde_json::Value` 参数
5. **Mutex 选型**：历史缓存用 `std::sync::Mutex`（非 async 场景），避免 async context 中的 lock 持有问题
6. **sync_response 消息本身不入历史缓存**，避免递归循环

## 约束一致性

- `RelayClient` 扩展不引入新的外部依赖（仅使用 std::sync::Mutex + VecDeque + AtomicU64）
- 协议消息扩展遵循现有 serde internally-tagged enum 模式
- Web 端 js 变更仅修改 `app.js`，无新依赖
- Relay 透明转发 `sync_request` 无需理解语义，符合"Relay 只做路由"的设计原则

## 验收标准

### Phase 1：扁平事件 + seq + Sync
- [x] Web 刷新页面后可恢复当前 Agent 会话的历史消息
- [x] Web 断线重连后使用增量 sync（since_seq > 0），不重复加载历史
- [x] 所有实时事件均携带递增 `seq` 字段
- [x] `handleAgentEvent` 中移除 `msg.event || msg` 兼容层，直接使用 `msg`
- [x] `sync_response` 正确处理，历史消息按序渲染
- [x] 历史缓存不超过 1000 条（内存安全）
- [x] Agent 离线期间 Web 无法 sync（返回已有缓存内容，不报错）

### Phase 2：BaseMessage 作为 Relay 传输单元
- [x] `AgentEvent::MessageAdded(BaseMessage)` 新增为 relay 传输的最小数据单元
- [x] executor.rs 在 4 个消息添加位置调用 `emit(MessageAdded(...))`
- [x] `RelayClient::send_message(&BaseMessage)` 新增，直接序列化 BaseMessage 为 JSON + seq 发送
- [x] 前端 app.js 支持 BaseMessage 格式（`role` 字段）与旧 AgentEvent 格式（`type` 字段）双格式兼容
- [x] TUI 渲染继续使用现有的 `AgentEvent::ToolCall`/`AssistantChunk` 等事件（不改变 UI 层）
- [ ] 前端人工验证：DevTools 中确认消息格式为 BaseMessage 格式（用户可选验证）

---

## Phase 2：Relay 传输单元改为 BaseMessage

### 背景

Phase 1 实现后，relay 传输的是视图层事件（如 `text_chunk`、`tool_start`、`tool_end`），这些是 UI 渲染用的。

Phase 2 需求：**relay 传输的最小数据单元应该是 `BaseMessage`（Human/Ai/System/Tool）**，表示历史过程的新增，而非视图层的事件。

### 好处

1. **语义更清晰** — relay 传输的是消息历史，不是 UI 事件
2. **前端可自行决定渲染方式** — 前端根据 message type 决定如何展示
3. **StateSnapshot 变得多余** — 增量消息天然包含完整历史

### 架构变更

#### 新增 `AgentEvent::MessageAdded`

```rust
// rust-create-agent/src/agent/events.rs
pub enum AgentEvent {
    // ... 现有变体保留 ...
    /// 增量消息（BaseMessage），relay 传输的最小数据单元
    MessageAdded(crate::messages::BaseMessage),
}
```

#### executor.rs 消息添加时触发事件

| 位置 | 消息类型 | 说明 |
|------|----------|------|
| 用户输入 | `Human` | 用户发送的消息 |
| LLM 工具调用推理 | `Ai` (含 tool_calls) | LLM 决定调用工具 |
| 工具执行结果 | `Tool` | 工具返回的结果 |
| LLM 最终回答 | `Ai` | LLM 生成最终答案 |

```rust
// 示例：用户输入
let human_msg = BaseMessage::human(input.content);
state.add_message(human_msg.clone());
self.emit(AgentEvent::MessageAdded(human_msg));
```

#### RelayClient 新增 `send_message`

```rust
// rust-relay-server/src/client/mod.rs
impl RelayClient {
    /// 发送 BaseMessage 到 relay（序列化为 JSON + seq）
    pub fn send_message(&self, msg: &rust_create_agent::messages::BaseMessage) {
        if !self.connected.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(val) = serde_json::to_value(msg) {
            self.send_with_seq(val);
        }
    }
}
```

#### agent.rs 事件回调处理

```rust
// rust-agent-tui/src/app/agent.rs
let handler: Arc<dyn AgentEventHandler> = Arc::new(FnEventHandler(move |event: ExecutorEvent| {
    // 转发到 Relay
    if let Some(ref relay) = relay_for_handler {
        match &event {
            ExecutorEvent::MessageAdded(msg) => relay.send_message(msg),
            _ => relay.send_agent_event(&event),
        }
    }
    // TUI 渲染继续使用已有 AgentEvent ...
}));
```

#### 前端 app.js 双格式支持

```javascript
// handleSingleEvent 支持两种格式：
// - BaseMessage 格式：{ role: "user"|"assistant"|"tool"|"system", content: "...", tool_calls?: [...] }
// - 旧 AgentEvent 格式：{ type: "text_chunk"|"tool_start"|"...", ... }
function handleSingleEvent(sessionId, event) {
    // BaseMessage 格式（role 字段）
    if (event.role !== undefined) {
        handleBaseMessage(agent, event);
        return;
    }
    // 旧 AgentEvent 格式（type 字段）
    handleLegacyEvent(agent, event);
}
```

#### BaseMessage 序列化格式

```json
// 用户消息
{ "role": "user", "content": "hello", "seq": 1 }

// AI 工具调用消息（无文本内容，只有 tool_calls）
{ "role": "assistant", "content": "", "tool_calls": [{"id":"id1","name":"bash","arguments":"..."}], "seq": 2 }

// AI 最终回答
{ "role": "assistant", "content": "这是回答", "tool_calls": [], "seq": 5 }

// 工具结果
{ "role": "tool", "tool_call_id": "id1", "content": "file listing...", "is_error": false, "seq": 3 }
```
