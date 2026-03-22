# Agent Storage Refactor 人工验收清单

**生成时间:** 2026-03-22 21:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 检查 sqlite3 命令行工具: `sqlite3 --version`
- [ ] [AUTO] 编译 rust-create-agent: `cargo build -p rust-create-agent 2>&1 | tail -3`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | tail -3`

### 测试数据准备
- [ ] [AUTO] 确认项目根目录正确: `test -f /Users/konghayao/code/ai/perihelion/Cargo.toml && echo OK`
- [ ] [MANUAL] 准备至少一个配置了 API Key 的环境（ANTHROPIC_API_KEY 或 OPENAI_API_KEY），用于场景 2 的端到端测试

---

## 验收项目

### 场景 1：SQLite 存储结构与依赖

#### - [x] 1.1 rusqlite 和 parking_lot 依赖正确添加
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "rusqlite\|parking_lot" /Users/konghayao/code/ai/perihelion/rust-create-agent/Cargo.toml` → 期望: 输出包含 `rusqlite = { version = "0.31", features = ["bundled"] }` 和 `parking_lot = "0.12"` 两行
  2. [A] `cargo build -p rust-create-agent 2>&1 | grep -c "^error"` → 期望: 输出 `0`（无编译错误）
- **异常排查:**
  - 如果 grep 无输出: 检查 `rust-create-agent/Cargo.toml` 的 `[dependencies]` 节点是否正确写入

#### - [x] 1.2 SQLite Schema 代码结构正确
- **来源:** Task 2 检查步骤 / spec-design.md 1.2
- **操作步骤:**
  1. [A] `grep -c "CREATE TABLE IF NOT EXISTS threads" /Users/konghayao/code/ai/perihelion/rust-create-agent/src/thread/sqlite_store.rs` → 期望: `1`
  2. [A] `grep -c "CREATE TABLE IF NOT EXISTS messages" /Users/konghayao/code/ai/perihelion/rust-create-agent/src/thread/sqlite_store.rs` → 期望: `1`
  3. [A] `grep -c "idx_messages_thread_seq" /Users/konghayao/code/ai/perihelion/rust-create-agent/src/thread/sqlite_store.rs` → 期望: `1`（索引定义存在）
- **异常排查:**
  - 如果任何 grep 返回 `0`: 查看 `sqlite_store.rs` 中 `init_schema()` 函数，确认 SQL 字符串完整

#### - [x] 1.3 SqliteThreadStore 实现所有 7 个 ThreadStore 方法
- **来源:** Task 2 / spec-design.md 1.3 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent thread::sqlite_store 2>&1 | grep -E "test result|FAILED"` → 期望: `test result: ok` 且无 `FAILED`
  2. [A] `grep -c "async fn " /Users/konghayao/code/ai/perihelion/rust-create-agent/src/thread/sqlite_store.rs` → 期望: `>= 7`（7 个 async 方法实现）
- **异常排查:**
  - 如果测试失败: 查看失败的具体测试名，定位到 `sqlite_store.rs` 对应方法实现

---

### 场景 2：消息持久化端到端正确性

#### - [x] 2.1 新建会话后消息写入 SQLite
- **来源:** Task 7 端到端验证 1
- **操作步骤:**
  1. [H] 在终端运行 `cargo run -p rust-agent-tui`，等待 TUI 界面加载完成，发送一条消息（如 "你好"），等待回复出现，然后关闭 TUI（按 Ctrl+C 或 Ctrl+Q）→ 是/否（TUI 正常启动并收到回复？）
  2. [A] `sqlite3 ~/.zen-core/threads/threads.db "SELECT COUNT(*) FROM messages;"` → 期望: 输出数字 `>= 2`（用户消息 + assistant 消息）
- **异常排查:**
  - 如果 TUI 启动失败: 运行 `cargo run -p rust-agent-tui 2>&1 | head -30` 查看错误
  - 如果 messages 表为空: 检查 `poll_agent` 中 StateSnapshot 分支是否触发，查看 `RUST_LOG=debug cargo run -p rust-agent-tui` 日志中是否有 `received StateSnapshot in poll_agent`

#### - [x] 2.2 多轮对话 seq 严格递增、role 正确交替
- **来源:** Task 7 端到端验证 2 / spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在同一个 TUI 会话中再发送第二条消息，等待回复后关闭 TUI → 是/否（成功发送并收到第二条回复？）
  2. [A] `sqlite3 ~/.zen-core/threads/threads.db "SELECT seq, role FROM messages WHERE thread_id=(SELECT id FROM threads ORDER BY updated_at DESC LIMIT 1) ORDER BY seq;"` → 期望: seq 从 1 开始严格递增，role 列值按顺序为 user, assistant, user, assistant...（或包含 tool）
- **异常排查:**
  - 如果 seq 不连续: 检查 `append_messages` 中 `SELECT COALESCE(MAX(seq),0)` 逻辑是否正确
  - 如果消息重复: 确认 `persist_pending_messages` 函数已被删除（`grep "persist_pending" rust-agent-tui/src/app/mod.rs` 应无输出）

#### - [x] 2.3 StateSnapshot 无双写（persist_pending_messages 已删除）
- **来源:** Task 5 检查步骤 / spec-design.md 2.2
- **操作步骤:**
  1. [A] `grep -c "persist_pending" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs` → 期望: `0`（函数已删除）
  2. [A] `grep -n "StateSnapshot" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs` → 期望: 输出中包含 `append_messages` 调用（增量持久化逻辑保留）
- **异常排查:**
  - 如果 grep 返回非 0: `persist_pending_messages` 未完全删除，需检查 `mod.rs` 所有引用点

---

### 场景 3：MessageAdapter 双向转换

#### - [x] 3.1 OpenAiAdapter 单元测试全部通过
- **来源:** Task 3 / Task 6 / spec-design.md 3.3
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent messages::adapters::openai 2>&1 | grep -E "test result|FAILED"` → 期望: `test result: ok` 且无 `FAILED`
  2. [A] `grep -c "fn test_" /Users/konghayao/code/ai/perihelion/rust-create-agent/src/messages/adapters/openai.rs` → 期望: `>= 4`（至少 4 个测试函数覆盖 Human/Ai/Tool/System）
- **异常排查:**
  - 如果测试失败: 查看具体失败的测试，定位到 `openai.rs` 中 `from_base_messages` 或 `to_base_message` 方法

#### - [x] 3.2 AnthropicAdapter 单元测试全部通过
- **来源:** Task 3 / Task 6 / spec-design.md 3.4
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent messages::adapters::anthropic 2>&1 | grep -E "test result|FAILED"` → 期望: `test result: ok` 且无 `FAILED`
  2. [A] `grep -c "fn test_" /Users/konghayao/code/ai/perihelion/rust-create-agent/src/messages/adapters/anthropic.rs` → 期望: `>= 3`（覆盖 tool_use block、往返转换等）
- **异常排查:**
  - 如果测试失败: 重点检查 `to_base_message` 中 `tool_use` block 的解析逻辑

#### - [x] 3.3 Anthropic Tool 消息合并到前一条 user 消息的 content blocks
- **来源:** spec-design.md 3.4 AnthropicAdapter 描述
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent test_from_base_messages_tool_use_merged 2>&1 | grep -E "test result|FAILED"` → 期望: `test result: ok`（专项测试通过，验证 Tool 消息合并行为）
- **异常排查:**
  - 如果失败: 检查 `anthropic.rs` 中 `BaseMessage::Tool` 分支，确认 `should_append` 逻辑将 tool_result block 追加到前一条 user 消息

---

### 场景 4：TUI 集成与模块重构

#### - [x] 4.1 TUI 使用 SqliteThreadStore、无 FilesystemThreadStore 残留
- **来源:** Task 4 检查步骤 / Task 5
- **操作步骤:**
  1. [A] `grep -rn "FilesystemThreadStore" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/` → 期望: 无输出（所有引用已替换）
  2. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^error"` → 期望: 无输出（编译零错误）
- **异常排查:**
  - 如果有 FilesystemThreadStore 残留: 全局搜索 `grep -rn FilesystemThreadStore rust-agent-tui/` 定位残留位置

---

### 场景 5：向后兼容与 TUI 启动

#### - [x] 5.1 旧 JSONL 文件存在时 TUI 正常启动，不产生 panic
- **来源:** Task 7 端到端验证 5 / spec-design.md 4 向后兼容
- **操作步骤:**
  1. [A] `test -f ~/.zen-core/threads/threads.db && echo "db_exists" || echo "no_db"` → 期望: 任意结果均可（验证命令本身可执行）
  2. [H] 如果存在旧的 `~/.zen-core/threads/index.json`，删除 `threads.db`（`rm -f ~/.zen-core/threads/threads.db`），然后运行 `cargo run -p rust-agent-tui`，观察 TUI 是否正常启动（不出现 `panic` 或 `thread main panicked` 字样）→ 是/否
- **异常排查:**
  - 如果出现 panic: 查看错误堆栈，通常是 `SqliteThreadStore::default_path()` 目录创建失败，检查 `~/.zen-core/threads/` 目录权限

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | rusqlite/parking_lot 依赖 | 2 | 0 | ⬜ | |
| 场景 1 | 1.2 | SQLite Schema 代码结构 | 3 | 0 | ⬜ | |
| 场景 1 | 1.3 | ThreadStore 7 个方法测试 | 2 | 0 | ⬜ | |
| 场景 2 | 2.1 | 新建会话消息写入 SQLite | 1 | 1 | ⬜ | 需 API Key |
| 场景 2 | 2.2 | 多轮对话 seq 递增顺序 | 1 | 1 | ⬜ | 需 API Key |
| 场景 2 | 2.3 | StateSnapshot 无双写 | 2 | 0 | ⬜ | |
| 场景 3 | 3.1 | OpenAiAdapter 测试 | 2 | 0 | ⬜ | |
| 场景 3 | 3.2 | AnthropicAdapter 测试 | 2 | 0 | ⬜ | |
| 场景 3 | 3.3 | Anthropic Tool 合并 blocks | 1 | 0 | ⬜ | |
| 场景 4 | 4.1 | TUI 使用 SqliteThreadStore | 2 | 0 | ⬜ | |
| 场景 5 | 5.1 | 旧 JSONL 兼容启动 | 1 | 1 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
