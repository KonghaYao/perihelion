# relay-message-id-propagation 人工验收清单

**生成时间:** 2026-03-26
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 全项目构建通过: `cargo build 2>&1 | grep "^error" | wc -l`

---

## 验收项目

### 场景 1：代码结构验证

#### - [x] 1.1 events.rs 枚举字段结构正确
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -A6 'TextChunk' rust-create-agent/src/agent/events.rs` → 期望: 输出包含 `message_id: crate::messages::MessageId` 和 `chunk: String` 两个字段
  2. [A] `grep -A2 'ToolStart {' rust-create-agent/src/agent/events.rs | head -6` → 期望: 输出首行包含 `message_id`
  3. [A] `grep -A2 'ToolEnd {' rust-create-agent/src/agent/events.rs | head -6` → 期望: 输出首行包含 `message_id`
- **异常排查:**
  - 若未找到 `message_id` 字段: 检查 `rust-create-agent/src/agent/events.rs` 第 7-12 行是否完成变更

#### - [x] 1.2 TUI AgentEvent 枚举未受影响（兼容性验证）
- **来源:** spec-design.md 约束一致性
- **操作步骤:**
  1. [A] `grep -n 'AssistantChunk\|ToolCall {' rust-agent-tui/src/app/events.rs` → 期望: 输出包含 `AssistantChunk(String)` 和 `ToolCall {`（无 message_id 字段）
- **异常排查:**
  - 若 TUI AgentEvent 含 message_id: 说明 events.rs 被误改，检查 `rust-agent-tui/src/app/events.rs`

#### - [x] 1.3 全项目编译无 error、无新增 warning
- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep "^error"` → 期望: 无输出（空）
  2. [A] `cargo build 2>&1 | grep "^warning" | wc -l` → 期望: 数量不超过正常基线（通常 ≤ 5 个，均为已知 unused 警告）
- **异常排查:**
  - 若有 `error[E0063]: missing field message_id`: 检查 executor.rs 中所有 `ToolStart`/`ToolEnd`/`TextChunk` emit 点是否全部注入了 `message_id`
  - 若有 `error[E0308]: mismatched types`（TextChunk 相关）: 检查 `agent.rs` 映射层 pattern 是否已更新为 `TextChunk { chunk, .. }`

---

### 场景 2：单元测试验证

#### - [x] 2.1 TextChunk 携带的 message_id 与 MessageAdded(Ai) 的 id 一致
- **来源:** Task 4 端到端验证 #1 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_text_chunk_message_id --nocapture 2>&1 | tail -5` → 期望: 输出包含 `test agent::executor::tests::test_text_chunk_message_id ... ok`
- **异常排查:**
  - 若 assertion failed `TextChunk.message_id 应与 MessageAdded(Ai).id 相同`: 检查 executor.rs 路径二（最终答案分支）的 `let ai_msg_id = ai_msg.id()` 是否在 `state.add_message(ai_msg)` 之前捕获

#### - [x] 2.2 ToolStart/ToolEnd 携带的 message_id 与 MessageAdded(Ai) 的 id 一致
- **来源:** Task 4 端到端验证 #2 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_tool_message_id --nocapture 2>&1 | tail -5` → 期望: 输出包含 `test agent::executor::tests::test_tool_message_id ... ok`
- **异常排查:**
  - 若 `ToolStart.message_id 应与 MessageAdded(Ai).id 相同` 失败: 检查 executor.rs 路径一工具调用分支中 `ai_msg_id` 是否已移到内层 block 外部
  - 若 `ToolEnd.message_id` 失败: 检查 ToolEnd emit 是否注入了 `message_id: ai_msg_id`（含拒绝分支和正常分支）

#### - [x] 2.3 TextChunk/ToolStart/ToolEnd 序列化后 JSON 含 message_id 字段
- **来源:** Task 4 端到端验证 #3 / spec-design.md Relay 透传验证
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_agent_event_message_id_serialization --nocapture 2>&1 | tail -5` → 期望: 输出包含 `test agent::executor::tests::test_agent_event_message_id_serialization ... ok`
  2. [A] `cargo test -p rust-create-agent --lib -- message_id 2>&1 | grep -E "FAILED|ok"` → 期望: 全部为 `ok`，无 `FAILED`
- **异常排查:**
  - 若序列化断言失败: 检查 `MessageId` serde 派生（在 `rust-create-agent/src/messages/message.rs` 中确认 `#[derive(Serialize, Deserialize)]`）

---

### 场景 3：回归测试与构建质量

#### - [x] 3.1 rust-create-agent 全量单元测试通过
- **来源:** Task 2、Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib 2>&1 | tail -3` → 期望: 最后一行包含 `test result: ok` 且 `0 failed`
  2. [A] `cargo test -p rust-create-agent --lib 2>&1 | grep "FAILED"` → 期望: 无输出
- **异常排查:**
  - 若有 test FAILED: 运行 `cargo test -p rust-create-agent --lib -- --nocapture 2>&1 | grep -A5 "FAILED"` 查看详情

#### - [x] 3.2 TUI headless 测试通过
- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -3` → 期望: 最后一行包含 `test result: ok` 且 `0 failed`
- **异常排查:**
  - 若 compile error 涉及 `TextChunk`: 检查 `rust-agent-tui/src/app/agent.rs` 中所有 `TextChunk` pattern 是否已改为 `TextChunk { chunk, .. }`
  - 若 runtime FAILED: 运行 `cargo test -p rust-agent-tui -- --nocapture 2>&1 | grep -A10 "FAILED"` 查看详情

#### - [x] 3.3 全量测试回归（rust-create-agent + rust-agent-middlewares）
- **来源:** Task 4 端到端验证 #4
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib 2>&1 | grep "test result"` → 期望: `test result: ok. 42 passed; 0 failed`（或 ≥42 passed，允许新增测试）
  2. [A] `cargo test -p rust-agent-middlewares --lib 2>&1 | grep "test result"` → 期望: `test result: ok. N passed; 0 failed`
  3. [A] `cargo build --workspace 2>&1 | grep "^error"` → 期望: 无输出
- **异常排查:**
  - 若 rust-agent-middlewares 有失败: 检查中间件是否有使用 `AgentEvent::TextChunk` / `ToolStart` / `ToolEnd` pattern 匹配的代码（当前版本应无此类使用）
  - 若 workspace build 失败: 运行 `cargo build --workspace 2>&1 | grep "^error" | head -20` 查看完整错误

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 代码结构 | 1.1 | events.rs 枚举字段结构 | 3 | 0 | ✅ | |
| 代码结构 | 1.2 | TUI AgentEvent 枚举不变 | 1 | 0 | ✅ | |
| 代码结构 | 1.3 | 全项目编译无 error/warning | 2 | 0 | ✅ | |
| 单元测试 | 2.1 | TextChunk message_id 一致性 | 1 | 0 | ✅ | |
| 单元测试 | 2.2 | ToolStart/ToolEnd message_id 一致性 | 1 | 0 | ✅ | |
| 单元测试 | 2.3 | 序列化 JSON 含 message_id 字段 | 2 | 0 | ✅ | |
| 回归测试 | 3.1 | rust-create-agent 全量单测 | 2 | 0 | ✅ | |
| 回归测试 | 3.2 | TUI headless 测试 | 1 | 0 | ✅ | |
| 回归测试 | 3.3 | 全量测试回归 | 3 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
