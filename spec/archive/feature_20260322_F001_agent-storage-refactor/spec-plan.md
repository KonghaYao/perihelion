# Agent Storage Refactor 执行计划

**目标:** 用 SQLite 替代 JSONL，解决 crash-safe 问题，实现消息双向转换

**技术栈:** rusqlite, tokio::spawn_blocking

**设计文档:** ./spec-design.md

---

### Task 1: 添加 rusqlite 依赖

**涉及文件:**
- 新建: 无
- 修改: `rust-create-agent/Cargo.toml`

**执行步骤:**
- [x] 在 `rust-create-agent/Cargo.toml` 的 `[dependencies]` 下添加 `rusqlite = { version = "0.31", features = ["bundled"] }`
  - `bundled` 特性编译 SQLite，无需系统安装
- [x] 添加 `parking_lot = "0.12"`（提供 `Mutex<Connection>`，串行化读-计算-写操作）
  - `rusqlite::Connection` 不实现 `Send`，必须用 Mutex 保护并在 `spawn_blocking` 中访问

**检查步骤:**
- [x] 验证 Cargo.toml 依赖正确添加
  - `grep -n "rusqlite\|parking_lot" /Users/konghayao/code/ai/perihelion/rust-create-agent/Cargo.toml`
  - 预期: 包含 `rusqlite` 和 `parking_lot` 条目
- [x] 验证 `cargo build -p rust-create-agent` 编译通过
  - `cd /Users/konghayao/code/ai/perihelion && cargo build -p rust-create-agent 2>&1 | tail -10`
  - 预期: 无编译错误（只有警告）

---

### Task 2: 创建 SqliteThreadStore 实现

**涉及文件:**
- 新建: `rust-create-agent/src/thread/sqlite_store.rs`
- 修改: `rust-create-agent/src/thread/mod.rs`

**执行步骤:**
- [x] 创建 `rust-create-agent/src/thread/sqlite_store.rs`
  - 实现 `SqliteThreadStore` 结构体，内部 `conn: parking_lot::Mutex<rusqlite::Connection>`
  - `new(db_path)` 打开连接，执行 `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`
  - `default_path()` 使用 `dirs_next::home_dir().join(".zen-core/threads/threads.db")`，自动创建父目录
  - `init_schema()` 执行 CREATE TABLE IF NOT EXISTS（线程安全，幂等）
- [x] 实现 `ThreadStore` trait 的 7 个方法：
  - `create_thread`: 事务插入 threads 表
  - `append_messages`: 在持有 `parking_lot::Mutex<Connection>` 锁的单个事务内执行：
    1. `SELECT COALESCE(MAX(seq),0) FROM messages WHERE thread_id=?` 获取当前最大 seq
    2. 对每条消息递增 seq，使用 `INSERT OR IGNORE INTO messages ...` 插入（幂等，重复 seq 静默跳过）
    3. 提交事务
  - `load_messages`: `SELECT * FROM messages WHERE thread_id=? ORDER BY seq ASC`，每行反序列化完整 `BaseMessage` JSON
  - `load_meta`: `SELECT * FROM threads WHERE id=?`
  - `update_meta`: 事务更新 threads 表
  - `list_threads`: `SELECT * FROM threads ORDER BY updated_at DESC`
  - `delete_thread`: 事务删除 messages + threads 表（CASCADE）
- [x] `BaseMessage` 序列化：`content` 列存储完整 `BaseMessage` 的 `serde_json::to_string(&msg)`（含 role tag），`role` 列单独冗余存储用于查询过滤

**检查步骤:**
- [x] `cargo test -p rust-create-agent thread::sqlite_store 2>&1` 编译通过
  - 预期: 编译成功（测试可在 Task 6 中补充）
- [x] 检查 Schema 正确性
  - 在代码中确认 SQL 字符串包含 `CREATE TABLE IF NOT EXISTS threads` 和 `CREATE TABLE IF NOT EXISTS messages`
  - 确认 `idx_messages_thread_seq` 索引存在

---

### Task 3: 创建 AnthropicMessages 双向转换模块

**涉及文件:**
- 新建: `rust-create-agent/src/messages/adapters/mod.rs`
- 新建: `rust-create-agent/src/messages/adapters/openai.rs`
- 新建: `rust-create-agent/src/messages/adapters/anthropic.rs`
- 修改: `rust-create-agent/src/messages/mod.rs`

**执行步骤:**
- [x] 创建 `rust-create-agent/src/messages/adapters/mod.rs`
  - 定义 `MessageAdapter` trait
  - 导出 `OpenAiAdapter` 和 `AnthropicAdapter`
- [x] 创建 `openai.rs`
  - `OpenAiAdapter` 实现 `MessageAdapter`
  - `from_base_messages`: `BaseMessage[]` → `serde_json::Value`（JSON 数组，每条含 role/content/tool_calls）
  - `to_base_message`: `&serde_json::Value` → `BaseMessage`，返回 `anyhow::Result<BaseMessage>`
- [x] 创建 `anthropic.rs`
  - `AnthropicAdapter` 实现 `MessageAdapter`
  - `from_base_messages`: `BaseMessage[]` → `serde_json::Value`（Anthropic messages 数组）
  - `to_base_message`: `&serde_json::Value` → `BaseMessage`，处理 tool_use/tool_result blocks
- [x] 修改 `rust-create-agent/src/messages/mod.rs`，添加 `pub mod adapters;`

**检查步骤:**
- [x] `cargo build -p rust-create-agent 2>&1 | grep -E "error|warning" | head -20`
  - 预期: 无 error（适配器模块可能因为没用到而 unused 是正常的）
- [x] 验证 trait 在 mod.rs 中导出
  - `grep -n "MessageAdapter" rust-create-agent/src/messages/adapters/mod.rs`
  - 预期: 找到 trait 定义

---

### Task 4: 重构 TUI Thread 模块

**涉及文件:**
- 修改: `rust-agent-tui/src/thread/mod.rs`

**执行步骤:**
- [x] 修改 `rust-agent-tui/src/thread/mod.rs`
  - 将导出从 `FilesystemThreadStore` 改为 `SqliteThreadStore`
  - 保持 `ThreadBrowser`、`ThreadStore`、`ThreadId`、`ThreadMeta` 不变

**检查步骤:**
- [x] `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无编译错误（如果有 FilesystemThreadStore 引用残留，在此阶段修复）

---

### Task 5: 重构 TUI App 模块（核心：移除双写）

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在 `rust-agent-tui/src/app/mod.rs` 中：
  - **删除** `persist_pending_messages` 函数
  - **重构** `poll_agent` 中的 `StateSnapshot` 分支：增量持久化逻辑保持，但移除对 `persist_pending_messages` 的调用
  - **重构** `Done` 分支：移除 `self.persist_pending_messages()` 调用
  - **重构** `Error` 分支：移除 `self.persist_pending_messages()` 调用
  - 保留 `self.agent_state_messages` 管理逻辑不变
- [x] 在 `rust-agent-tui/src/app/agent.rs` 中：
  - `submit_message` 中的 `ensure_thread_id()` 保持不变（thread 在发消息时创建）
  - `run_universal_agent` 参数中的 `_thread_id` 目前未使用，后续可连接 SQLite thread 创建

**检查步骤:**
- [x] 确认 `persist_pending_messages` 已删除
  - `grep -n "persist_pending" rust-agent-tui/src/app/mod.rs`
  - 预期: 无匹配
- [x] 确认 `poll_agent` 中 StateSnapshot 分支逻辑正确
  - `grep -n "StateSnapshot" rust-agent-tui/src/app/mod.rs`
  - 预期: 找到 StateSnapshot 分支处理代码

---

### Task 6: 集成测试与验收

**涉及文件:**
- 修改: `rust-create-agent/src/thread/sqlite_store.rs`（添加单元测试）
- 修改: `rust-create-agent/src/messages/adapters/openai.rs`（添加单元测试）
- 修改: `rust-create-agent/src/messages/adapters/anthropic.rs`（添加单元测试）

**执行步骤:**
- [x] 在 `sqlite_store.rs` 中添加集成测试：
  - 测试 `create_thread` → `append_messages` → `load_messages` 完整流程
  - 测试 `list_threads` 按 updated_at 降序
  - 测试 `delete_thread` CASCADE 删除消息
  - 测试 `open_thread` 后消息顺序一致
- [x] 在 `openai.rs` 中添加测试：
  - `from_base_message`: Human/Ai/Tool/System 转换正确
  - `to_base_message`: OpenAI → BaseMessage → OpenAI 往返一致
- [x] 在 `anthropic.rs` 中添加测试：
  - `from_base_message`: 处理 tool_use block
  - `to_base_message`: Anthropic ContentBlock → BaseMessage 正确
- [x] 运行全量测试
  - `cargo test -p rust-create-agent 2>&1 | tail -20`
- [x] 运行 TUI 编译验证
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`

**检查步骤:**
- [x] `cargo test -p rust-create-agent 2>&1 | grep -E "test result|FAILED|passed|failed"`
  - 预期: `test result: ok` 且无 FAILED
- [x] 确认旧 JSONL 存在时程序正常启动
  - 在已有 `~/.zen-core/threads/` 的机器上运行 `cargo build -p rust-agent-tui`
  - 预期: 编译通过，运行时创建 `threads.db` 不报错

---

### Task 7: agent-storage-refactor Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 测试数据准备: 任意项目目录，确保无 API Key 时能展示"未配置"提示而非 panic

**End-to-end verification:**

1. **新建会话正常持久化**
   - 启动 TUI，发送一条消息，等待 Done
   - `sqlite3 ~/.zen-core/threads/threads.db "SELECT COUNT(*) FROM messages;"`
   - Expected: `>= 2`（至少用户消息 + assistant 消息；实际数量取决于工具调用次数）
   - On failure: check Task 2 [SqliteThreadStore.append_messages]

2. **加载历史会话后消息顺序一致**
   - 发送第二条消息，触发 Done
   - `sqlite3 ~/.zen-core/threads/threads.db "SELECT seq, role FROM messages ORDER BY seq;"`
   - Expected: seq 递增，role 交替（user → assistant → ...）
   - On failure: check Task 2 [load_messages ORDER BY seq]

3. **OpenAI 消息格式转换正确**
   - 启动带 OPENAI_API_KEY 的 TUI，发送消息
   - Expected: 消息正常发送和接收，BaseMessage → OpenAI 格式无 panic
   - On failure: check Task 3 [OpenAiAdapter.from_base_message]

4. **Anthropic 消息格式转换正确**
   - 启动带 ANTHROPIC_API_KEY 的 TUI，发送消息
   - Expected: 消息正常发送和接收，BaseMessage → Anthropic 格式无 panic
   - On failure: check Task 3 [AnthropicAdapter.from_base_message]

5. **TUI 启动无 panic（无旧 JSONL 迁移）**
   - 删除 `~/.zen-core/threads/threads.db`，保留旧 `index.json`
   - `cargo run -p rust-agent-tui 2>&1 | head -20`
   - Expected: 正常启动，无文件找不到错误
   - On failure: check Task 4 [SqliteThreadStore.default_path]
