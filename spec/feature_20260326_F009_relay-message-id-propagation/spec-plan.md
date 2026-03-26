# relay-message-id-propagation 执行计划

**目标:** 为 `ExecutorEvent::TextChunk`、`ToolStart`、`ToolEnd` 注入所属 AI 消息的 `message_id`，使 Relay Web 前端可按 ID 做 update-in-place 渲染

**技术栈:** Rust 2021, serde, uuid (MessageId / Copy)

**设计文档:** `./spec-design.md`

---

### Task 1: AgentEvent 枚举变更

**涉及文件:**
- 修改: `rust-create-agent/src/agent/events.rs`

**执行步骤:**
- [x] 将 `TextChunk(String)` 改为结构体变体 `TextChunk { message_id: MessageId, chunk: String }`
  - 在文件顶部 `use` 块引入 `crate::messages::MessageId`（如未 glob 导入）
- [x] 为 `ToolStart` 增加 `message_id: MessageId` 字段（放在第一位，与 TextChunk 风格一致）
  - `ToolStart { message_id: MessageId, tool_call_id: String, name: String, input: serde_json::Value }`
- [x] 为 `ToolEnd` 增加 `message_id: MessageId` 字段
  - `ToolEnd { message_id: MessageId, tool_call_id: String, name: String, output: String, is_error: bool }`

**检查步骤:**
- [x] 枚举定义编译通过（此时调用方会报错，属于预期）
  - `cargo check -p rust-create-agent 2>&1 | grep "error\[" | head -20`
  - 预期: 只出现 `executor.rs` 和 `agent.rs` 的调用方错误，events.rs 本身无 error
- [x] TextChunk JSON 序列化包含 message_id 字段
  - `grep -A5 'TextChunk' rust-create-agent/src/agent/events.rs`
  - 预期: 看到 `message_id: MessageId` 和 `chunk: String` 两个字段

---

### Task 2: executor.rs 捕获 AI 消息 ID 并注入事件

**涉及文件:**
- 修改: `rust-create-agent/src/agent/executor.rs`

**执行步骤:**
- [x] **路径一（有工具调用）**：在 `state.add_message(ai_msg)` 之前捕获 `ai_msg_id`
  - 在 `ai_msg` 创建后、移入 `state.add_message()` 之前，新增一行：`let ai_msg_id = ai_msg.id();`
  - 由于原有内层代码块 `{}` 使 `ai_msg_id` 不可在后续的 `ToolStart`/`ToolEnd` emit 中访问，需将该行提到代码块外（或移除内层 block）
  - 具体：把 `let tc_reqs`、`let ai_msg`、`let ai_msg_id = ai_msg.id()`、`let ai_msg_clone`、`state.add_message`、`self.emit(MessageAdded)` 全部移到内层 block 外
- [x] 更新路径一中 **拒绝分支** 的 `ToolStart`/`ToolEnd` emit（executor.rs ~line 208-218）
  - 原: `AgentEvent::ToolStart { tool_call_id, name, input }`
  - 改: `AgentEvent::ToolStart { message_id: ai_msg_id, tool_call_id, name, input }`
  - 同样更新 `ToolEnd`
- [x] 更新路径一中 **正常工具** 的 `ToolStart` emit（~line 234）和 `ToolEnd` emit（~line 317）
  - 注入 `message_id: ai_msg_id`
- [x] **路径二（最终答案）**：在 `state.add_message(ai_msg)` 之前捕获 `ai_msg_id`
  - 在 `let ai_msg = reasoning.source_message...` 之后新增 `let ai_msg_id = ai_msg.id();`
  - 更新 `TextChunk` emit：`AgentEvent::TextChunk { message_id: ai_msg_id, chunk: answer }`

**检查步骤:**
- [x] rust-create-agent 全量编译无 error
  - `cargo check -p rust-create-agent 2>&1 | grep "^error" | head -10`
  - 预期: 无输出（只剩 TUI 侧 pattern 报错）
- [x] 单元测试通过（executor 测试不匹配 AgentEvent 变体，应零改动通过）
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: `test result: ok. N passed; 0 failed`

---

### Task 3: TUI agent.rs 映射层 pattern 更新

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 更新 **Langfuse hook** 中三处 pattern（~line 105-112）
  - `ExecutorEvent::ToolStart { tool_call_id, name, input }` → `ExecutorEvent::ToolStart { tool_call_id, name, input, .. }`
  - `ExecutorEvent::ToolEnd { tool_call_id, is_error, output, .. }` — 已有 `..`，无需改动
  - `ExecutorEvent::TextChunk(text)` → `ExecutorEvent::TextChunk { chunk, .. }` + `t.on_text_chunk(&chunk)`
- [x] 更新 **TUI AgentEvent 映射** 中的 pattern（~line 117-170）
  - `ExecutorEvent::AiReasoning(text)` — 保持不变（无结构体化）
  - `ExecutorEvent::TextChunk(text)` → `ExecutorEvent::TextChunk { chunk, .. }` + `AgentEvent::AssistantChunk(chunk)`
  - `ExecutorEvent::ToolStart { name, input, .. } if name == "launch_agent"` — 已有 `..`，无需改动
  - `ExecutorEvent::ToolStart { tool_call_id, name, input }` → `ExecutorEvent::ToolStart { tool_call_id, name, input, .. }`
  - `ExecutorEvent::ToolEnd { name, output, is_error, .. }` if name == "launch_agent" — 已有 `..`，无需改动
  - `ExecutorEvent::ToolEnd { name, output, is_error: false, .. }` if name == "ask_user" — 已有 `..`，无需改动
  - `ExecutorEvent::ToolEnd { name, output, is_error: true, .. }` — 已有 `..`，无需改动
  - `ExecutorEvent::ToolEnd { .. }` — 已有 `..`，无需改动

**检查步骤:**
- [x] 全项目编译无 error，无 warning
  - `cargo build 2>&1 | grep -E "^error|warning\[" | head -20`
  - 预期: 无 error，warning 数量与改动前相同（不新增）
- [x] TUI headless 测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: `test result: ok. N passed; 0 failed`
- [x] rust-create-agent 单元测试通过
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: `test result: ok. N passed; 0 failed`

---

### Task 4: Relay Message ID 验收

**前置条件:**
- 构建命令: `cargo build`
- 确保 `cargo test` 全量通过后再做端到端验证

**端到端验证:**

1. **TextChunk 携带正确 message_id**
   - 在 `executor.rs` 中的 `TextChunk` emit 前后分别取 `ai_msg_id` 和事件中的 `message_id`，通过临时测试断言验证一致性
   - `cargo test -p rust-create-agent --lib -- message_id 2>&1`
   - Expected: 测试通过，或新增验证测试输出 `ok`
   - On failure: 检查 Task 2 路径二的 `ai_msg_id` 捕获
   - ✅ 结果: test_text_chunk_message_id ok

2. **ToolStart/ToolEnd message_id 与 MessageAdded id 一致**
   - 编写/运行单元测试：用 `MockLLM::tool_then_answer()` 驱动执行器，收集 `ToolStart { message_id }` 和前一条 `MessageAdded(Ai{id})`，断言两者相同
   - `cargo test -p rust-create-agent --lib -- tool_message_id 2>&1`
   - Expected: 测试通过
   - On failure: 检查 Task 2 路径一的 `ai_msg_id` 捕获（拒绝分支 / 正常分支）
   - ✅ 结果: test_tool_message_id ok

3. **Relay JSON 透传：text_chunk 含 message_id 字段**
   - `cargo test -p rust-create-agent --lib 2>&1 | grep -E "ok|FAILED"`
   - 补充：序列化断言 `serde_json::to_value(&AgentEvent::TextChunk { message_id: MessageId::new(), chunk: "x".into() }).unwrap()["message_id"].is_string()`
   - Expected: `true`
   - On failure: 检查 Task 1 枚举定义中 MessageId 的 serde Serialize 推导
   - ✅ 结果: test_agent_event_message_id_serialization ok

4. **全量测试回归**
   - `cargo test 2>&1 | tail -10`
   - Expected: 所有 test crate 均 `ok`，0 failed
   - On failure: 按 Task 编号逐一排查编译 error 或 pattern 不匹配
   - ✅ 结果: 42 passed; 0 failed（rust-create-agent）
