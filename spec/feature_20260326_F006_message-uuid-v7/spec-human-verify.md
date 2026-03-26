# message-uuid-v7 人工验收清单

**生成时间:** 2026-03-26
**关联计划:** `spec-plan.md`
**关联设计:** `spec-design.md`

> 注：所有验收项均为纯自动化验证，无需人工交互。

---

## 验收前准备

### 环境要求
- [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [AUTO] 确认工作目录为项目根: `test -d rust-create-agent && test -d rust-agent-tui`

---

## 验收项目

### 场景 1：MessageId 类型定义与 BaseMessage 字段

#### - [x] 1.1 MessageId 类型在 messages 模块可访问
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n 'pub struct MessageId' rust-create-agent/src/messages/message.rs` → 期望: 找到 `pub struct MessageId(uuid::Uuid);` 定义
  2. [A] `grep -n 'MessageId' rust-create-agent/src/messages/mod.rs` → 期望: 包含 `pub use message::{BaseMessage, MessageId, ToolCallRequest};` 导出
- **异常排查:**
  - 如果找不到：检查 Task 2 执行步骤是否完整

#### - [x] 1.2 BaseMessage 四个变体均含 id 字段
- **来源:** Task 3 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -E 'Human|Ai|System|Tool' rust-create-agent/src/messages/message.rs | grep -A1 'BaseMessage::' | grep 'id:'` → 期望: 每个变体都含 `id:` 字段（4 个）
- **异常排查:**
  - 如果只有部分变体含 id：检查 enum 定义是否完整

#### - [x] 1.3 所有构造器自动填充 MessageId::new()
- **来源:** Task 3 / spec-design.md 方案设计 3
- **操作步骤:**
  1. [A] `grep -n 'MessageId::new' rust-create-agent/src/messages/message.rs` → 期望: 至少 7 处（human/ai/ai_with_tool_calls/ai_from_blocks/system/tool_result/tool_error 各一处）
  2. [A] `grep -c 'MessageId::new' rust-create-agent/src/messages/message.rs` → 期望: 输出数字 >= 7
- **异常排查:**
  - 如果数量不足：检查各构造器是否完整更新

#### - [x] 1.4 id() 访问器正确返回 MessageId
- **来源:** Task 6 / spec-design.md 方案设计 3
- **操作步骤:**
  1. [A] `grep -A10 'pub fn id' rust-create-agent/src/messages/message.rs | head -12` → 期望: 包含 `pub fn id(&self) -> MessageId` 方法，四个 match arm 均返回 `*id`
  2. [A] `cargo test -p rust-create-agent --lib -- message_id 2>&1 | tail -5` → 期望: `test messages::message::tests::test_message_id_generated ... ok`

---

### 场景 2：SQLite 持久化 message_id

#### - [x] 2.1 messages 表主键为 message_id（不含 seq）
- **来源:** Task 5 / spec-design.md 方案设计 5
- **操作步骤:**
  1. [A] `grep -A10 'CREATE TABLE IF NOT EXISTS messages' rust-create-agent/src/thread/sqlite_store.rs` → 期望: 第一列为 `message_id TEXT PRIMARY KEY`，无 `seq` 列
  2. [A] `grep 'ORDER BY seq' rust-create-agent/src/thread/sqlite_store.rs` → 期望: 无输出（已移除 ORDER BY seq）
- **异常排查:**
  - 如果仍含 seq：检查 `init_schema` 是否更新

#### - [x] 2.2 append_messages 写入 message_id
- **来源:** Task 5 / spec-design.md 方案设计 5
- **操作步骤:**
  1. [A] `grep 'message_id' rust-create-agent/src/thread/sqlite_store.rs | grep -v '//' | grep 'INSERT'` → 期望: INSERT 语句含 `message_id` 占位符
  2. [A] `grep 'msg.id().as_uuid()' rust-create-agent/src/thread/sqlite_store.rs` → 期望: 找到 `let message_id = msg.id().as_uuid().to_string();`
- **异常排查:**
  - 如果找不到：检查 `append_messages` 是否更新

#### - [x] 2.3 load_messages 还原 BaseMessage 含正确 id（roundtrip 一致性）
- **来源:** Task 5 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_create_append_load 2>&1 | tail -5` → 期望: `test thread::sqlite_store::tests::test_create_append_load ... ok`
  2. [A] `cargo test -p rust-create-agent --lib -- test_message_order_after_multiple_appends 2>&1 | tail -5` → 期望: `test thread::sqlite_store::tests::test_message_order_after_multiple_appends ... ok`

---

### 场景 3：编译、适配层与全量测试

#### - [x] 3.1 全 crate 编译零错误
- **来源:** Task 4 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo build --all 2>&1 | grep '^error'` → 期望: 无输出（零错误）
- **异常排查:**
  - 如果有错误：检查 Task 4 所有 BaseMessage 模式匹配点是否已修复

#### - [x] 3.2 Provider 适配层序列化发给 LLM 的 JSON 无 id 字段
- **来源:** Task 4 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_from_base_messages 2>&1 | tail -10` → 期望: 5 tests passed（Anthropic/OpenAI adapter 序列化测试全部通过）
  2. [A] `grep -E '\.\.\s*\}' rust-create-agent/src/messages/adapters/anthropic.rs | wc -l` → 期望: 大于 0（adapter 使用 `..` 忽略 id 字段）
- **异常排查:**
  - 如果 adapter 测试失败：检查 `match` arms 是否正确使用 `..` 忽略 id

#### - [x] 3.3 所有单测通过（rust-create-agent）
- **来源:** Task 6 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib 2>&1 | tail -5` → 期望: `test result: ok. 39 passed; 0 failed`
- **异常排查:**
  - 如果有失败：检查具体失败的测试名称，对应修复

#### - [x] 3.4 TUI 编译零错误
- **来源:** Task 7 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep '^error'` → 期望: 无输出
- **异常排查:**
  - 如果有错误：检查 rust-agent-tui 中所有 BaseMessage 模式匹配

#### - [ ] 3.5 TUI headless 测试（跳过）
- **来源:** Task 7 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -10` → 期望: `test result: ok`（TUI headless 测试全部通过）
- **异常排查:**
  - 如果测试超时/失败：检查 `rust-agent-tui/src/ui/headless.rs` 中的 BaseMessage 模式匹配是否完整

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | MessageId 类型定义与导出 | 2 | 0 | ⬜ | |
| 场景 1 | 1.2 | BaseMessage 四个变体含 id 字段 | 1 | 0 | ⬜ | |
| 场景 1 | 1.3 | 所有构造器自动填充 id | 2 | 0 | ⬜ | |
| 场景 1 | 1.4 | id() 访问器正确返回 | 2 | 0 | ⬜ | |
| 场景 2 | 2.1 | messages 表主键为 message_id | 2 | 0 | ⬜ | |
| 场景 2 | 2.2 | append_messages 写入 message_id | 2 | 0 | ⬜ | |
| 场景 2 | 2.3 | load_messages 还原含正确 id | 2 | 0 | ⬜ | |
| 场景 3 | 3.1 | 全 crate 编译零错误 | 1 | 0 | ⬜ | |
| 场景 3 | 3.2 | Provider 适配层序列化无 id | 2 | 0 | ⬜ | |
| 场景 3 | 3.3 | 所有单测通过（39 个） | 1 | 0 | ⬜ | |
| 场景 3 | 3.4 | TUI 编译零错误 | 1 | 0 | ⬜ | |
| 场景 3 | 3.5 | TUI headless 测试 | 1 | 0 | ⬜ | 执行阶段已跳过 |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
