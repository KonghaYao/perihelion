# Feature: 20260327_F002 - relay-command-sync

## 需求背景

Relay Web 前端与 Agent TUI 之间存在两个双向同步缺失的问题：

1. **前端命令发送缺失**：Web 前端输入 `/clear` 或 `/compact` 等命令时，`/compact` 尚未转化为结构化消息发给 Agent；`/clear` 虽有处理但实现待完善。
2. **Agent 侧状态变更未同步**：当 Agent TUI 通过 `/clear` 清空对话、通过 `/history` 切换历史、或 `/compact` 完成压缩后，Web 前端的消息列表无法感知变化，仍停留在旧状态。

## 目标

- 支持从 Web 前端发送 `/clear`（清空线程）和 `/compact`（压缩上下文）命令到 Agent
- 当 Agent 侧发生 thread 状态变更时（清空/切换历史/压缩完成），Web 前端自动更新消息列表

## 方案设计

### 整体数据流

![命令与状态同步数据流](./images/01-flow.png)

数据流分两个方向：

**Web → Agent（命令发送）**：
- `/clear` → `WebMessage::ClearThread` （已有）→ Agent 调用 `new_thread()` + 发 `ThreadReset{[]}`
- `/compact` → 新增 `WebMessage::CompactThread` → Agent 调用 `start_compact("")` + 完成后发 `ThreadReset{msgs}`

**Agent → Web（状态同步）**：
- 任何 thread 状态变更 → 新增 `RelayMessage::ThreadReset { messages }` → Web 前端替换消息列表

### 协议扩展（rust-relay-server/src/protocol.rs）

新增两个消息变体：

```rust
// WebMessage 枚举（Web → Agent）：新增
CompactThread,

// RelayMessage 枚举（Agent → Web）：新增
ThreadReset {
    messages: Vec<serde_json::Value>,  // BaseMessage JSON 数组，空表示清空
},
```

序列化形式：
- `{"type":"compact_thread"}`
- `{"type":"thread_reset","messages":[...]}`

**设计决策**：`ThreadReset` 使用 `send_raw`（不注入 seq、不进历史缓存），因为它是状态重置控制消息，不应参与增量 SyncRequest 回放。新接入的 Web 客户端通过 `SyncRequest{since_seq:0}` 获取历史消息，`ThreadReset` 不在回放序列中。

### RelayClient 新增方法（rust-relay-server/src/client/mod.rs）

```rust
pub fn send_thread_reset(&self, messages: &[BaseMessage]) {
    let msgs: Vec<serde_json::Value> = messages.iter()
        .filter_map(|m| serde_json::to_value(m).ok())
        .collect();
    let json = serde_json::json!({ "type": "thread_reset", "messages": msgs });
    if let Ok(s) = serde_json::to_string(&json) {
        self.send_raw(&s);   // 不进历史缓存
    }
}
```

### Agent TUI 改动

**relay_ops.rs** — 处理新命令 + ClearThread 追加 ThreadReset 发送：

| 事件 | 现有行为 | 新增行为 |
|------|---------|---------|
| `WebMessage::ClearThread` | `clear_history()` + `new_thread()` | 追加 `relay.send_thread_reset(&[])` |
| `WebMessage::CompactThread`（新） | — | `self.start_compact(String::new())` |

**thread_ops.rs** — thread 状态变更时通知 Web：

| 函数 | 触发场景 | 新增行为 |
|------|---------|---------|
| `new_thread()` | TUI `/clear` 命令 | `relay.send_thread_reset(&[])` |
| `open_thread()` | TUI `/history` 切换历史 | `relay.clear_history()` + `relay.send_thread_reset(&base_msgs)` |

**agent_ops.rs** — Compact 完成后同步：

在 `AgentEvent::CompactDone` 分支处理完成、重建 view_messages 后，追加：
```rust
if let Some(ref relay) = self.relay_client {
    relay.send_thread_reset(&self.agent_state_messages);
}
```

### Web 前端改动

![前端命令与状态处理流程](./images/02-flow.png)

**render.js — 命令解析扩展**：

```js
const doSend = () => {
    const text = inputEl.value.trim();
    if (!text) return;
    if (text === '/clear') {
        sendMessage(sessionId, { type: 'clear_thread' });
        // 本地立即清空，ThreadReset 到达后再次确认
        agent.messages = []; agent.todos = []; agent.maxSeq = 0;
        renderMessages(paneId, agent);
    } else if (text === '/compact') {
        sendMessage(sessionId, { type: 'compact_thread' });
    } else {
        sendMessage(sessionId, { type: 'user_input', text });
    }
    inputEl.value = '';
};
```

**events.js — 处理 `thread_reset` 事件**（在 `handleLegacyEvent` 中新增）：

```js
case 'thread_reset': {
    agent.messages = [];
    agent.maxSeq = 0;   // ThreadReset 不带 seq，重置追踪
    (event.messages || []).forEach(m => handleBaseMessage(agent, m));
    break;
}
```

- `messages` 为空数组时 → 消息列表清空（对应 `/clear`）
- `messages` 有内容时 → 重建消息列表（对应历史切换 / compact 完成）

## 实现要点

1. **`send_raw` 而非 `send_with_seq`**：`ThreadReset` 是控制消息不应被历史缓存，新连接 Web 客户端走 SyncRequest 拿历史，不需要重放 ThreadReset。
2. **open_thread 中先 clear_history**：切换历史前先清空 relay 历史缓存，再推 `ThreadReset`，确保后续 SyncRequest 返回的是新 thread 的内容（但实际上 open_thread 只推 ThreadReset，SyncRequest 结果依赖 relay history 是否被重新填充——此处选择只发 ThreadReset，不填充历史，SyncRequest 对新 thread 返回空）。
3. **compact 与 view_messages 的关系**：Compact 替换的是 `agent_state_messages`（LLM 上下文），ThreadReset 推送的也是 `agent_state_messages`，确保前端与 LLM 实际上下文一致。
4. **多 Web 客户端广播**：`send_raw` 最终通过 `relay_client.tx` 发给 relay server，relay server 通过 `forward_to_web()` 广播到所有该 session 的 Web 客户端，无需额外处理。
5. **前端 /clear 双保险**：render.js 本地立即清空 + 等待 `thread_reset` 事件确认，避免网络延迟导致用户感知到消息残留。

## 约束一致性

- 与 `constraints.md` 一致：Relay Server 使用 axum WebSocket，新消息类型沿用 `serde(tag = "type", rename_all = "snake_case")` 约定
- 与 `architecture.md` 一致：不打破现有 Agent→Relay→Web 数据流方向，新增的 RelayMessage 类型通过现有 `send_raw` 发送路径转发
- `send_raw` 绕过历史缓存是有意设计，不违反架构约束（历史缓存仅用于 SyncRequest 回放，控制消息不需要回放）

## 验收标准

- [ ] Web 前端输入 `/clear`：Agent 侧 `new_thread()` 被触发，前端消息列表清空
- [ ] Web 前端输入 `/compact`：Agent 侧 `start_compact()` 被触发，compact 完成后前端消息列表替换为压缩后内容
- [ ] Agent TUI 输入 `/clear`：Web 前端在 1 秒内自动清空消息列表
- [ ] Agent TUI 通过 `/history` 切换历史：Web 前端消息列表替换为所选历史的完整消息
- [ ] Compact 完成后（无论从 TUI 还是 Web 触发）：Web 前端正确显示压缩后消息
- [ ] 多个 Web 客户端同时连接同一 session：ThreadReset 对所有客户端均生效
