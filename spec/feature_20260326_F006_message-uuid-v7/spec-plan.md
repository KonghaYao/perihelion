# message-uuid-v7 执行计划

**目标:** 为 `BaseMessage` 增加 UUID v7 唯一标识符，SQLite Schema 重建

**技术栈:** Rust, uuid v7, rusqlite, serde

**设计文档:** `spec-design.md`

---

### Task 1: 添加 uuid serde 依赖

**涉及文件:**
- 修改: `rust-create-agent/Cargo.toml`

**执行步骤:**
- [x] 在 `uuid` 依赖中添加 `"serde"` feature
  ```toml
  uuid = { version = "1", features = ["v7", "serde"] }
  ```
  注意：`"v7"` feature 已存在，只需追加 `"serde"`。

**检查步骤:**
- [x] 验证 uuid serde feature 已添加
  - `grep -n 'uuid.*serde' rust-create-agent/Cargo.toml`
  - 预期: 行包含 `"serde"`

---

### Task 2: 定义 MessageId 类型

**涉及文件:**
- 修改: `rust-create-agent/src/messages/mod.rs`
- 修改: `rust-create-agent/src/messages/message.rs`

**执行步骤:**
- [x] 在 `messages/mod.rs` 中定义 `MessageId` 类型
- [x] 在 `messages/mod.rs` 中导出 `MessageId`
- [x] 在 `message.rs` 中将 `MessageId` 引入 scope

**检查步骤:**
- [x] `MessageId` 在 `messages` 模块可访问

---

### Task 3: BaseMessage 四个变体增加 id 字段

**涉及文件:**
- 修改: `rust-create-agent/src/messages/message.rs`

**执行步骤:**
- [x] 为 `BaseMessage` 的四个变体增加 `id: MessageId` 字段
- [x] 更新所有构造器，自动填充 `id: MessageId::new()`
- [x] 新增 `id()` 访问器方法

**检查步骤:**
- [x] 编译通过（类型检查）

---

### Task 4: 修复所有 BaseMessage 模式匹配点（struct literal）

**涉及文件:**（共 17 个文件，54 处出现）

**执行步骤:**
- [x] 按依赖顺序修复所有文件的 struct literal 模式匹配（添加 `..`）
- [x] 若某文件使用 `BaseMessage::Human { content }`（不带 `..`），改为 `BaseMessage::Human { content, .. }`

**检查步骤:**
- [x] 全量编译通过
- [x] 全量 clippy 无 BaseMessage 相关 warning

---

### Task 5: SQLite Schema 重建

**涉及文件:**
- 修改: `rust-create-agent/src/thread/sqlite_store.rs`

**执行步骤:**
- [x] 更新 `init_schema` 方法：messages 表主键改为 `message_id TEXT PRIMARY KEY`，移除 `seq` 列
- [x] 更新 `append_messages`：写入 `message_id`（来自 `msg.id().as_uuid().to_string()`），移除 seq 逻辑
- [x] 更新 `load_messages` SELECT：移除 `ORDER BY seq`，不再 SELECT seq

**检查步骤:**
- [x] SQLite schema 正确 — `test_create_append_load` 通过
- [x] `messages` 表含 `message_id` 列 — roundtrip 测试通过

---

### Task 6: 更新单元测试

**涉及文件:**
- 修改: `rust-create-agent/src/messages/message.rs` 中的 `#[cfg(test)]`

**执行步骤:**
- [x] 新增 `test_message_id_generated` 测试：验证不同消息 id 不同，序列化/反序列化后 id 一致
- [x] 所有单测通过

**检查步骤:**
- [x] 所有单测通过 — 39 passed, 0 failed

---

### Task 7: TUI Headless 测试

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/headless.rs`

**执行步骤:**
- [x] `rust-agent-tui` 全量编译通过（base message struct literal 无需修改）
- [x] 跳过 headless 测试（耗时过长，用户要求跳过）

**检查步骤:**
- [x] `cargo build --all` 零错误

---

### Task 8: Acceptance — message-uuid-v7 全流程验收

**End-to-end verification:**

1. `BaseMessage::human("x").id()` 返回有效 UUID v7
   - `cargo test -p rust-create-agent --lib -- message_id` ✅
2. SQLite roundtrip（写入 → 读取 → id 一致）
   - `cargo test -p rust-create-agent --lib -- test_create_append_load` ✅
3. Provider 适配层序列化无 `id` 字段
   - `cargo test -p rust-create-agent --lib -- test_from_base_messages` ✅（5 tests passed）
4. 全 crate 编译零错误
   - `cargo build --all` ✅
5. ThreadStore roundtrip
   - `cargo test -p rust-create-agent --lib -- test_message_order_after_multiple_appends` ✅
