# Feature: 20260326_F009 - relay-message-id-propagation

## 需求背景

F006（message-uuid-v7）已为每条 `BaseMessage` 分配了 UUID v7 的唯一 ID，并在 `MessageAdded` 事件中通过完整 `BaseMessage` 对象传递给 Relay。

然而，紧随其后的流式增量事件（`TextChunk`、`ToolStart`、`ToolEnd`）目前**不携带 message_id**，导致 Web 前端无法将这些事件关联到对应的 AI 消息，进而无法实现 **update-in-place** 渲染（按 ID 原地更新消息，而非依赖不稳定的顺序假设）。

## 目标

- `ExecutorEvent::TextChunk` 携带所属 AI 消息的 `message_id`
- `ExecutorEvent::ToolStart` / `ToolEnd` 携带所属 AI 消息的 `message_id`
- Relay Server 无需修改，JSON 自动透传新字段
- Web 前端可按 `message_id` 对消息列表做 update-in-place 渲染
- TUI 侧枚举不变，仅更新 pattern 匹配（兼容性零破坏）

## 方案设计

### 1. AgentEvent 枚举变更

**文件：`rust-create-agent/src/agent/events.rs`**

```rust
// 变更前
TextChunk(String),
ToolStart { tool_call_id: String, name: String, input: serde_json::Value },
ToolEnd   { tool_call_id: String, name: String, output: String, is_error: bool },

// 变更后
TextChunk { message_id: MessageId, chunk: String },
ToolStart { message_id: MessageId, tool_call_id: String, name: String, input: serde_json::Value },
ToolEnd   { message_id: MessageId, tool_call_id: String, name: String, output: String, is_error: bool },
```

`MessageId` 是 `Copy` 类型（封装 `uuid::Uuid`），追加字段无额外分配开销。

### 2. executor.rs 事件发射点更新

**文件：`rust-create-agent/src/agent/executor.rs`**

ReAct 循环中有两条发射路径，均需捕获当前 AI 消息的 `message_id`：

**路径一：有工具调用（ToolStart/ToolEnd 注入 message_id）**

```rust
let ai_msg = reasoning.source_message.clone()
    .unwrap_or_else(|| BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs));
let ai_msg_id = ai_msg.id();  // ← 捕获 message_id（Copy，零开销）
state.add_message(ai_msg);
self.emit(AgentEvent::MessageAdded(ai_msg_clone));
// ...
// ToolStart 和 ToolEnd 均注入 ai_msg_id：
self.emit(AgentEvent::ToolStart {
    message_id: ai_msg_id,
    tool_call_id: ..., name: ..., input: ...
});
self.emit(AgentEvent::ToolEnd {
    message_id: ai_msg_id,
    tool_call_id: ..., name: ..., output: ..., is_error: ...
});
```

**路径二：最终答案（TextChunk 注入 message_id）**

```rust
let ai_msg = reasoning.source_message.unwrap_or_else(|| BaseMessage::ai(answer.as_str()));
let ai_msg_id = ai_msg.id();  // ← 捕获 message_id
state.add_message(ai_msg);
self.emit(AgentEvent::MessageAdded(ai_msg_clone));
self.emit(AgentEvent::TextChunk { message_id: ai_msg_id, chunk: answer });
```

### 3. TUI agent.rs 映射层更新

**文件：`rust-agent-tui/src/app/agent.rs`**

TUI 侧的 `FnEventHandler` 中更新 pattern 解构，用 `..` 忽略新字段，TUI 自身的 `AgentEvent` 枚举**不变**：

**Langfuse hook 更新：**

```rust
// 变更前
ExecutorEvent::TextChunk(text) => t.on_text_chunk(text),
ExecutorEvent::ToolStart { tool_call_id, name, input } => t.on_tool_start(...),
ExecutorEvent::ToolEnd { tool_call_id, is_error, output, .. } => t.on_tool_end(...),

// 变更后（用 .. 忽略 message_id）
ExecutorEvent::TextChunk { chunk, .. } => t.on_text_chunk(&chunk),
ExecutorEvent::ToolStart { tool_call_id, name, input, .. } => t.on_tool_start(tool_call_id, name, input),
ExecutorEvent::ToolEnd { tool_call_id, is_error, output, .. } => t.on_tool_end(tool_call_id, output, *is_error),
```

**TUI AgentEvent 映射更新：**

```rust
// 变更前
ExecutorEvent::TextChunk(text) => AgentEvent::AssistantChunk(text),
ExecutorEvent::ToolStart { tool_call_id, name, input } => AgentEvent::ToolCall { ... },
ExecutorEvent::ToolEnd { name, output, is_error: true, .. } => ...,

// 变更后（解构新字段，传 chunk 给 AssistantChunk）
ExecutorEvent::TextChunk { chunk, .. } => AgentEvent::AssistantChunk(chunk),
ExecutorEvent::ToolStart { tool_call_id, name, input, .. } => AgentEvent::ToolCall { ... },
ExecutorEvent::ToolEnd { name, output, is_error: true, .. } => ...,
```

### 4. Relay 数据流（无需修改）

Relay Server 完全透传 JSON，新字段自动出现在 Web 前端收到的事件中：

```jsonc
// ① MessageAdded：创建消息条目（id 字段来自 F006）
{ "role": "assistant", "id": "0196f3a2-...", "content": [...], "seq": 41 }

// ② ToolStart：关联到 AI 消息
{ "type": "tool_start", "message_id": "0196f3a2-...", "tool_call_id": "call_xyz", "name": "bash", "input": {...}, "seq": 42 }

// ③ ToolEnd：同一 AI 消息
{ "type": "tool_end", "message_id": "0196f3a2-...", "tool_call_id": "call_xyz", "name": "bash", "output": "...", "is_error": false, "seq": 43 }

// ④ TextChunk（流式回复）：关联到最终 AI 消息
{ "type": "text_chunk", "message_id": "0196f3b1-...", "chunk": "Hello world", "seq": 44 }
```

**Web 前端 update-in-place 逻辑：**

1. 收到 `MessageAdded(Ai{id})` → 以 `id` 为 key 创建消息条目
2. 收到 `TextChunk { message_id, chunk }` → 找到 `message_id` 对应的 AssistantBubble，追加 chunk
3. 收到 `ToolStart { message_id }` → 关联工具调用到对应 AI 消息（可用于嵌套渲染）

![Relay 事件流与 message_id 传递](./images/01-flow.png)

### 5. SubAgent 场景

`launch_agent` 触发的 `ToolStart` 携带其父 Agent 当前 AI 消息的 `message_id`，行为与普通工具一致，无需特殊处理。

## 实现要点

| 模块 | 变更 |
|------|------|
| `rust-create-agent/src/agent/events.rs` | `TextChunk(String)` → `TextChunk { message_id, chunk }`；`ToolStart`/`ToolEnd` 增加 `message_id` 字段 |
| `rust-create-agent/src/agent/executor.rs` | 捕获 `ai_msg.id()` 并注入上述事件；两个发射路径各加一行 `let ai_msg_id = ai_msg.id();` |
| `rust-agent-tui/src/app/agent.rs` | Langfuse hook + TUI AgentEvent 映射：用 `..` 解构忽略 `message_id` |
| `rust-relay-server` | **无需变更** |
| 所有 `#[cfg(test)]` | 更新用到 `TextChunk` / `ToolStart` / `ToolEnd` 的测试的 pattern 匹配 |

## 约束一致性

- 符合 **消息不可变历史**：`message_id` 在 AI 消息构造时固定，事件只读取不修改
- 符合 **事件驱动 TUI 通信**：新字段通过现有 mpsc channel 传递，无新通道/共享状态
- 符合 **Relay Server WebSocket 协议**：JSON 消息帧向后兼容（新增字段，旧客户端忽略）
- TUI 枚举不变，满足 **Middleware Chain 模式**：变更仅限于 executor 核心和 TUI 适配层边界

## 验收标准

- [ ] `ExecutorEvent::TextChunk { message_id, chunk }` 的 `message_id` 与前一条 `MessageAdded(Ai{id})` 的 `id` 相同
- [ ] `ExecutorEvent::ToolStart { message_id }` 的 `message_id` 与同轮次 `MessageAdded(Ai{id})` 的 `id` 相同
- [ ] Relay 透传：Web 客户端收到的 `text_chunk` JSON 含 `message_id` 字段
- [ ] `cargo test -p rust-create-agent --lib` 全量通过
- [ ] `cargo test -p rust-agent-tui` headless 测试通过
- [ ] `cargo build` 无 warning
