# Feature: 20260326_F006 - message-uuid-v7

## 需求背景

当前 `BaseMessage`（Human/Ai/System/Tool）没有唯一标识符，在以下场景存在痛点：

- **SubAgent 委派**：子 agent 引用/追踪父 agent 的特定消息无稳定 ID
- **Relay 传输**：事件转发中无法精确定位某条消息
- **消息引用**：未来如果需要 `reply_to`/`parent_id` 等引用关系，无法实现

SQLite 历史数据可直接删除重建，无需迁移。

## 目标

- 每条 `BaseMessage` 在加入 state 时自动生成全局唯一 ID（UUID v7）
- SQLite 持久化存储 `message_id` 列，Schema 重建
- 不破坏现有 API（`add_message` 调用点无需修改）
- Provider 适配层序列化时不发送无意义的 `id` 字段

## 方案设计

### 1. 数据模型：`MessageId` 类型

新增 `MessageId` 类型封装 `uuid::Uuid`，使用 v7 版本：

- **UUID v7**：基于时间戳排序（适合数据库索引），跨进程安全
- 依赖：`uuid = { version = "1", features = ["v7", "serde"] }`

```rust
// rust-create-agent/src/messages/mod.rs

/// 消息唯一标识符 — UUID v7（时间有序，跨进程安全）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(uuid::Uuid);

impl MessageId {
    /// 生成新的 UUID v7
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub fn as_uuid(&self) -> uuid::Uuid { self.0 }
}

impl Default for MessageId {
    fn default() -> Self { Self::new() }
}
```

### 2. BaseMessage 增加 `id` 字段

`BaseMessage` 的四个变体全部增加 `id: MessageId` 字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum BaseMessage {
    #[serde(rename = "user")]
    Human { id: MessageId, content: MessageContent },

    #[serde(rename = "assistant")]
    Ai {
        id: MessageId,
        content: MessageContent,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCallRequest>,
    },

    #[serde(rename = "system")]
    System { id: MessageId, content: MessageContent },

    #[serde(rename = "tool")]
    Tool {
        id: MessageId,
        tool_call_id: String,
        content: MessageContent,
        #[serde(default)]
        is_error: bool,
    },
}
```

**序列化注意**：serde `#[serde(tag = "role")]` 会把 `id` 序列化到 JSON 中，如 `{"role":"user","id":"...","content":"..."}`。发给 LLM Provider 时需要手动序列化（跳过 `id` 字段）。

### 3. 构造器更新

所有现有构造器自动填充 `id`：

```rust
impl BaseMessage {
    pub fn human(content: impl Into<MessageContent>) -> Self {
        Self::Human { id: MessageId::new(), content: content.into() }
    }

    pub fn ai(content: impl Into<MessageContent>) -> Self {
        Self::Ai { id: MessageId::new(), content: content.into(), tool_calls: Vec::new() }
    }

    pub fn ai_with_tool_calls(content: impl Into<MessageContent>, tool_calls: Vec<ToolCallRequest>) -> Self {
        Self::Ai { id: MessageId::new(), content: content.into(), tool_calls }
    }

    pub fn ai_from_blocks(blocks: Vec<ContentBlock>) -> Self {
        // 从 blocks 提取 tool_calls 逻辑不变
        Self::Ai { id: MessageId::new(), content: MessageContent::Blocks(blocks), tool_calls }
    }

    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self::System { id: MessageId::new(), content: content.into() }
    }

    pub fn tool_result(id: impl Into<String>, content: impl Into<MessageContent>) -> Self {
        // 注意：tool_result 接收的是 tool_call_id（用于关联 ToolUse），不是 MessageId
        Self::Tool { id: MessageId::new(), tool_call_id: id.into(), content: content.into(), is_error: false }
    }

    pub fn tool_error(id: impl Into<String>, error: impl Into<MessageContent>) -> Self {
        Self::Tool { id: MessageId::new(), tool_call_id: id.into(), content: error.into(), is_error: true }
    }

    /// 获取消息 ID
    pub fn id(&self) -> MessageId {
        match self {
            Self::Human { id, .. } => *id,
            Self::Ai { id, .. } => *id,
            Self::System { id, .. } => *id,
            Self::Tool { id, .. } => *id,
        }
    }
}
```

### 4. `add_message` 自动生成 ID（State 层）

现有 `State::add_message` 签名不变，在内部自动注入 ID：

```rust
impl State for AgentState {
    fn add_message(&mut self, message: BaseMessage) {
        // 构造器已填充 id，此处无需额外处理
        self.messages.push(message);
    }
}
```

> 构造器已经自动填充 ID，无需在 `add_message` 额外处理。

### 5. SQLite Schema 重建

历史数据直接删除重建，不做迁移。新 Schema：

```sql
CREATE TABLE IF NOT EXISTS messages (
    message_id  TEXT PRIMARY KEY,
    thread_id   TEXT NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_thread_id
    ON messages (thread_id ASC);
```

- 主键从 `(thread_id, seq)` 改为 `message_id TEXT PRIMARY KEY`
- 移除 `seq` 列（不再需要手动序号，消息顺序由数据库查询时的 `ORDER BY` 或消息内时间戳决定）
- `append_messages` 写入时增加 `message_id` 字段

### 6. MessageAdapter 兼容

Provider 适配层在序列化发给 LLM 时跳过 `id` 字段：

```rust
// OpenAiAdapter::messages_to_provider
fn messages_to_provider(&self, messages: &[BaseMessage]) -> serde_json::Value {
    serde_json::json!(messages.iter().map(|m| {
        // 手动构建，跳过 id
        match m {
            BaseMessage::Human { content, .. } => json!({ "role": "user", "content": content }),
            BaseMessage::Ai { content, tool_calls, .. } => json!({ "role": "assistant", "content": content, "tool_calls": tool_calls }),
            BaseMessage::System { content, .. } => json!({ "role": "system", "content": content }),
            BaseMessage::Tool { tool_call_id, content, is_error, .. } => json!({ "role": "tool", "tool_call_id": tool_call_id, "content": content, "is_error": is_error }),
        }
    }).collect::<Vec<_>>())
}
```

反序列化从 LLM 响应时直接生成新 ID：

```rust
// OpenAiAdapter::message_from_provider
fn message_from_provider(&self, role: &str, content: Value) -> BaseMessage {
    match role {
        "user" => BaseMessage::human(content),
        "assistant" => BaseMessage::ai(content), // 自动生成 id
        "system" => BaseMessage::system(content),
        "tool" => { /* 提取 tool_call_id */ }
        _ => BaseMessage::human(content),
    }
}
```

### 7. ThreadStore 接口不变

`ThreadStore` trait 和 `SqliteThreadStore` 实现保持接口兼容：

- `append_messages` 内部写入 `message_id`
- `load_messages` 读取时还原 `MessageId`
- 对外 API 无感知变化

## 实现要点

| 模块 | 变更 |
|------|------|
| `rust-create-agent/Cargo.toml` | 新增 `uuid = { version = "1", features = ["v7", "serde"] }` |
| `rust-create-agent/src/messages/mod.rs` | 新增 `MessageId` 导出 |
| `rust-create-agent/src/messages/message.rs` | 四个变体增加 `id: MessageId`，更新所有构造器，增加 `id()` 访问器 |
| `rust-create-agent/src/messages/adapters/` | `OpenAiAdapter` / `AnthropicAdapter` 序列化跳过 `id`，反序列化生成新 ID |
| `rust-create-agent/src/thread/sqlite_store.rs` | Schema 重建（移除 seq，主键改 message_id），写入/读取 message_id |
| `rust-create-agent/src/thread/types.rs` | `ThreadMeta` 增加 `message_count` 字段（从 messages 表 COUNT） |
| `rust-agent-tui` | 编译通过即可，消息渲染直接用 `msg.id()` |
| `rust-agent-middlewares` | 各 middleware `prepend_message` 的系统消息自动获得 ID |
| 所有 `#[cfg(test)]` | 更新单测中的 `BaseMessage` 字面量构造 |

## 约束一致性

- 符合 **消息不可变历史**：`id` 在构造时固定，加入 state 后不修改
- 符合 **Middleware Chain 模式**：ID 生成不侵入 ReAct 执行器，仅修改数据模型层
- 符合 **异步优先**：SQLite 操作仍在 `spawn_blocking` 中执行

## 验收标准

- [ ] `BaseMessage::human("x").id()` 返回有效的 UUID v7
- [ ] `BaseMessage::ai_from_blocks([...]).id()` 自动生成
- [ ] SQLite `messages.message_id` 列存在，主键为 `message_id`
- [ ] `load_messages` 还原的 `BaseMessage` 含正确 ID（与写入时一致）
- [ ] Provider 适配层序列化发给 LLM 的 JSON 不含 `id` 字段
- [ ] 所有单测通过：`cargo test -p rust-create-agent --lib`
- [ ] TUI headless 测试通过：`cargo test -p rust-agent-tui`
