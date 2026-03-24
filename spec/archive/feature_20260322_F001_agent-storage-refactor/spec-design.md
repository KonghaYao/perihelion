# Feature: 20260322_F001 - agent-storage-refactor

## 需求背景

当前 agent 存储存在严重问题：

1. **消息丢失/重复**：Done 时 `StateSnapshot` + `persist_pending_messages` 双重写，导致消息重复或丢失
2. **崩溃后索引损坏**：JSONL 追加写不是原子操作，进程崩溃时 `index.json` 和 `messages.jsonl` 不一致
3. **恢复后 tool_call 错乱**：`open_thread` 加载时 `tool_call_id` 与显示名称映射丢失
4. **JSONL 行级损坏**：单行格式错误导致后续全部消息无法读取
5. **缺少 Provider 格式转换**：消息格式无法在存储层和 LLM Provider 之间双向转换

依据 `docs/data-architect.md` 的架构设计，重构整个存储层。

## 目标

- [ ] 用 SQLite 替代 JSONL，解决 crash-safe 和原子性问题
- [ ] 统一 StateSnapshot 驱动的增量持久化，消除双写
- [ ] 实现 AnthropicMessages 双向转换（BaseMessage ↔ Provider 格式）
- [ ] Thread 切换逻辑正确，恢复后消息完整、tool_call_id 一致
- [ ] 向后兼容：不迁移旧 JSONL 数据

## 方案设计

### 架构概览

![架构概览](./images/01-architecture.png)

### 1. SQLite 存储层

#### 1.1 数据库文件

路径：`~/.zen-core/threads/threads.db`

使用 SQLite WAL 模式，保证并发读写安全。

#### 1.2 Schema

```sql
-- Thread 元数据表
CREATE TABLE threads (
    id            TEXT PRIMARY KEY,
    cwd           TEXT NOT NULL,
    title         TEXT,
    message_count INTEGER DEFAULT 0,
    created_at    TEXT NOT NULL,  -- ISO 8601
    updated_at    TEXT NOT NULL   -- ISO 8601
    -- 不保留 meta 列：ThreadMeta struct 中无对应字段，避免序列化遗漏
);

-- 消息表
CREATE TABLE messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id    TEXT NOT NULL,
    seq          INTEGER NOT NULL,  -- 同 thread 内的自增序号
    role         TEXT NOT NULL,     -- user / assistant / system / tool
    content      TEXT NOT NULL,     -- MessageContent JSON
    tool_call_id TEXT,              -- tool 消息的工具调用 ID
    is_error     INTEGER DEFAULT 0,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
    UNIQUE(thread_id, seq)
);

CREATE INDEX idx_messages_thread_seq ON messages(thread_id, seq);
```

#### 1.3 ThreadStore Trait

位于 `rust-create-agent/src/thread/store.rs`（现有 trait 不变）。

实现类 `SqliteThreadStore`：

```rust
pub struct SqliteThreadStore {
    /// 用 Mutex 串行化所有读-计算-写操作，消除 seq 并发竞争条件
    conn: Arc<parking_lot::Mutex<rusqlite::Connection>>,
}

impl SqliteThreadStore {
    pub fn new(db_path: PathBuf) -> Result<Self> { ... }
    pub fn default_path() -> Result<Self> { ... }
}

#[async_trait]
impl ThreadStore for SqliteThreadStore {
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> { ... }
    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()> { ... }
    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> { ... }
    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta> { ... }
    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()> { ... }
    async fn list_threads(&self) -> Result<Vec<ThreadMeta>> { ... }
    async fn delete_thread(&self, id: &ThreadId) -> Result<()> { ... }
}
```

**事务写入：** 所有写操作（create_thread、append_messages、update_meta、delete_thread）在事务内执行，crash safe。

**幂等追加：** `append_messages` 在持有 `Mutex<Connection>` 锁的单个事务内完成"查 MAX(seq) → 递增插入"，消除并发竞争。INSERT 语句使用 `INSERT OR IGNORE INTO messages` 以保证幂等性（重复 seq 时静默跳过而非报错丢失消息）。

**消息加载：** `load_messages` 按 `ORDER BY seq ASC` 返回，保证顺序一致。

### 2. TUI 集成重构

#### 2.1 消息流程

![TUI 消息流程](./images/02-tui-flow.png)

#### 2.2 移除双写

**删除 `persist_pending_messages` 函数**。持久化完全由 `StateSnapshot` 事件驱动。

#### 2.3 `poll_agent` 中的 StateSnapshot 处理

```rust
Ok(AgentEvent::StateSnapshot(msgs)) => {
    // 增量追加：从 agent_state_messages.len() 之后的所有新消息
    let start = self.agent_state_messages.len();
    self.agent_state_messages.extend(msgs);

    if let Some(id) = self.current_thread_id.clone() {
        let new_msgs: Vec<_> = self.agent_state_messages[start..]
            .iter()
            .filter(|m| !matches!(m, BaseMessage::System { .. }))
            .cloned()
            .collect();
        if !new_msgs.is_empty() {
            let store = self.thread_store.clone();
            let tid = id.clone();
            tokio::spawn(async move {
                let _ = store.append_messages(&tid, &new_msgs).await;
            });
        }
    }
    updated = true;
}
```

#### 2.4 `open_thread` 加载

```rust
pub fn open_thread(&mut self, thread_id: ThreadId) {
    let store = self.thread_store.clone();
    let tid = thread_id.clone();
    let base_msgs = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(store.load_messages(&tid))
            .unwrap_or_default()
    });
    self.messages.clear();
    self.agent_state_messages = base_msgs.clone();
    for msg in base_msgs {
        // Tool 消息：tool_call_id = 工具名，content = display_name（持久化时写入）
        // 必须还原 display_name，否则 TUI 工具调用行显示为空
        let (tool_name, display_name) = if let BaseMessage::Tool {
            ref tool_call_id,
            ref content,
            ..
        } = msg {
            let name = tool_call_id.clone();
            let text = content.text_content();
            let display = if text.is_empty() { name.clone() } else { text };
            (Some(name), Some(display))
        } else {
            (None, None)
        };
        self.messages.push(ChatMessage {
            inner: msg,
            display_name,
            tool_name,
        });
    }
    self.persisted_count = self.messages.len();
    self.current_thread_id = Some(thread_id);
    self.thread_browser = None;
}
```

#### 2.5 `new_thread`

```rust
pub fn new_thread(&mut self) {
    self.messages.clear();
    self.agent_state_messages.clear();
    self.current_thread_id = None;
    self.persisted_count = 0;
    self.todo_message_index = None;
    self.thread_browser = None;
}
```

### 3. AnthropicMessages 双向转换

#### 3.1 模块结构

在 `rust-create-agent/src/messages/adapters/` 下新增：

```
rust-create-agent/src/messages/
  adapters/
    mod.rs
    openai.rs       # OpenAI 格式转换
    anthropic.rs    # Anthropic 格式转换
```

#### 3.2 Trait 定义

```rust
/// 消息格式适配 trait
/// ProviderMessage 用 serde_json::Value 表示 Provider 原生 JSON 格式，
/// 避免为 OpenAI/Anthropic 各自定义独立类型造成 trait 签名不统一。
pub trait MessageAdapter: Send + Sync {
    /// 将 BaseMessage 列表序列化为 Provider 原生 JSON 格式
    fn from_base_messages(&self, msgs: &[BaseMessage]) -> serde_json::Value;

    /// 将 Provider 原生 JSON 消息还原为 BaseMessage
    fn to_base_message(&self, msg: &serde_json::Value) -> anyhow::Result<BaseMessage>;

    /// 获取 Provider 名称（"openai" | "anthropic"）
    fn provider_name(&self) -> &'static str;
}
```

#### 3.3 OpenAI 适配器

```rust
// OpenAI 格式：
// - user/assistant/system/tool 角色
// - content: string 或 ContentBlock[]
// - tool_calls: ToolCall[]
// - tool_call_id: string

impl MessageAdapter for OpenAiAdapter {
    fn from_base_messages(&self, msgs: &[BaseMessage]) -> serde_json::Value {
        // 返回 [{"role": "user", "content": "..."}, ...] 格式的 JSON 数组
        serde_json::Value::Array(msgs.iter().map(|m| self.serialize_message(m)).collect())
    }

    fn to_base_message(&self, msg: &serde_json::Value) -> anyhow::Result<BaseMessage> {
        // 从 {"role": "...", "content": "...", "tool_calls": [...]} 还原
        // ...
    }

    fn provider_name(&self) -> &'static str { "openai" }
}
```

#### 3.4 Anthropic 适配器

```rust
// Anthropic 格式：
// - role: user / assistant
// - content: ContentBlock[]（text/tool_use/tool_result/image）
// - system 消息作为顶层 system 参数，不在 messages 数组中

impl MessageAdapter for AnthropicAdapter {
    fn from_base_messages(&self, msgs: &[BaseMessage]) -> serde_json::Value {
        // 返回 [{"role": "user"/"assistant", "content": [...]}, ...] 格式
        // Tool 消息合并到前一条 user 消息的 content blocks 中
        // ...
    }

    fn to_base_message(&self, msg: &serde_json::Value) -> anyhow::Result<BaseMessage> {
        // 从 {"role": "...", "content": [...blocks...]} 还原
        // ContentBlock::ToolUse → BaseMessage::Ai（ai_from_blocks）
        // ContentBlock::ToolResult → BaseMessage::Tool
        // ...
    }

    fn provider_name(&self) -> &'static str { "anthropic" }
}
```

#### 3.5 使用场景

LLM 请求发送时，通过 `MessageAdapter` 将 `BaseMessage` 转为 Provider 格式；接收响应后，通过 `to_base_message` 还原为 `BaseMessage`，存入 SQLite。

### 4. 向后兼容

- 首次启动检测 `threads.db` 是否存在，不存在则创建
- 不迁移旧 JSONL 数据（`index.json`、`messages.jsonl`）
- 旧数据保留在 `~/.zen-core/threads/` 下，用户可手动处理

### 5. 文件变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `rust-create-agent/src/thread/store.rs` | 不变 | ThreadStore trait 保持不变 |
| `rust-create-agent/src/thread/mod.rs` | 修改 | 导出 `SqliteThreadStore` |
| `rust-create-agent/src/thread/sqlite_store.rs` | 新增 | SQLite 实现 |
| `rust-create-agent/src/messages/adapters/mod.rs` | 新增 | 模块入口 |
| `rust-create-agent/src/messages/adapters/openai.rs` | 新增 | OpenAI 适配器 |
| `rust-create-agent/src/messages/adapters/anthropic.rs` | 新增 | Anthropic 适配器 |
| `rust-agent-tui/src/thread/mod.rs` | 修改 | 导出 `SqliteThreadStore` |
| `rust-agent-tui/src/app/mod.rs` | 修改 | 移除 persist_pending_messages，重构 StateSnapshot 处理 |
| `rust-agent-tui/src/app/agent.rs` | 修改 | 确保 thread_id 在 agent 执行前创建 |

## 实现要点

1. **SQLite 异步封装**：`SqliteThreadStore` 内部使用 `tokio::task::spawn_blocking` 调用同步 SQLite API；`parking_lot::Mutex<Connection>` 在 blocking 线程中持锁，串行化所有写操作
2. **WAL 模式初始化**：连接时执行 `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`
3. **seq 原子性保证**：`append_messages` 在同一个 `Mutex` 锁范围内执行"SELECT MAX(seq) → 递增 → INSERT OR IGNORE"，防止并发竞争导致 seq 冲突
4. **display_name 还原**：`open_thread` 从 Tool 消息的 `content` 字段还原 `display_name`（持久化时 display_name 写入 content），保持 TUI 工具调用行正确显示
5. **BaseMessage JSON 兼容性**：SQLite 的 `content TEXT` 列存储完整 `BaseMessage` JSON（含 role tag），`load_messages` 直接反序列化为 `BaseMessage`

## 验收标准

- [ ] `SqliteThreadStore` 实现 `ThreadStore` trait，所有方法通过测试
- [ ] `append_messages` 在事务内执行，crash 后数据不丢失
- [ ] `load_messages` 按 seq 顺序返回，加载结果与写入一致
- [ ] `open_thread` 加载后 `agent_state_messages` 与 SQLite 数据一致
- [ ] `poll_agent` 中 StateSnapshot 只触发一次增量持久化，无双写
- [ ] `OpenAiAdapter` 和 `AnthropicAdapter` 实现双向转换，测试覆盖
- [ ] 旧 JSONL 文件存在时程序正常启动（新数据库为空）
- [ ] `cargo test` 全量通过
