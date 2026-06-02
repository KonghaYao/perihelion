# ExecutorEvent LlmCallStart Arc 消息共享 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `LlmCallStart.messages` 从 `Vec<BaseMessage>` 改为 `Arc<Vec<BaseMessage>>`，消除每步 LLM 调用的全量消息克隆。长对话每轮可省 6 MB+。

**Architecture:** `LlmCallStart` 有 3 个消费者：Langfuse tracer（只读 `&[BaseMessage]`，内部仍会 `to_vec()` 存 HashMap）、TUI 映射（丢弃）、ACP mapper（丢弃）。改为 Arc 后 emit 侧只做一次 `Arc::from(to_vec())`，消费侧零改动（`Arc<Vec<T>>` 实现了 `Deref<Target=[T]>`）。Langfuse 启用时 tracer 内部仍有一次深拷贝（不可消除），但 channel 传递和丢弃路径的克隆完全消除。

**Tech Stack:** Rust, serde `rc` feature

---

## File Structure

| 文件 | 职责 | 改动类型 |
|------|------|----------|
| `Cargo.toml` (workspace root) | serde 依赖配置 | 修改（加 `rc` feature） |
| `peri-agent/src/agent/events.rs` | AgentEvent 枚举定义 | 修改（字段类型） |
| `peri-agent/src/agent/executor/llm_step.rs` | LlmCallStart emit 侧 | 修改 |
| `peri-acp/src/event/mapper_test.rs` | mapper 测试 | 修改 |
| `peri-agent/src/agent/events_test.rs` | events 测试 | 无需改动（无 LlmCallStart 测试） |

**不需要改动的消费侧文件**（`Arc<Vec<T>>` 自动 deref 为 `&[T]`）：
- `peri-acp/src/session/executor.rs:271-277` — `tracer.lock().on_llm_start(*step, messages, tools)` 签名 `&[BaseMessage]`，自动兼容
- `peri-acp/src/langfuse/tracer.rs:339-354` — `on_llm_start(&mut self, step, messages: &[BaseMessage], ...)` 自动兼容
- `peri-tui/src/app/agent.rs:110` — `ExecutorEvent::LlmCallStart { .. } => return None` 用 `{ .. }` 忽略字段
- `peri-acp/src/event/mapper.rs:223` — `ExecutorEvent::LlmCallStart { .. }` 用 `{ .. }` 忽略字段

---

### Task 1: 启用 serde rc feature

**Files:**
- Modify: `Cargo.toml:39`

- [ ] **Step 1: 修改 workspace serde 依赖，添加 `rc` feature**

```toml
# Before:
serde = { version = "1.0", features = ["derive"] }

# After:
serde = { version = "1.0", features = ["derive", "rc"] }
```

`rc` feature 让 serde 能序列化/反序列化 `Arc<T>` / `Rc<T>`，透明地与内部类型保持一致。`AgentEvent` derive 了 `Serialize, Deserialize`，`LlmCallStart.messages` 改为 `Arc<Vec<BaseMessage>>` 后需要此 feature。

- [ ] **Step 2: 验证编译通过**

Run: `cargo build 2>&1 | tail -5`
Expected: 编译成功（`rc` feature 向后兼容，不破坏现有代码）

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: enable serde rc feature for Arc serialization

Required for upcoming LlmCallStart.messages Arc<Vec<BaseMessage>> change.
The rc feature is backward-compatible and transparent to existing types."
```

---

### Task 2: 修改 LlmCallStart 字段类型和 emit 侧

**Files:**
- Modify: `peri-agent/src/agent/events.rs:96-100`
- Modify: `peri-agent/src/agent/executor/llm_step.rs:27-31`

- [ ] **Step 4: 修改 AgentEvent::LlmCallStart 的 messages 字段类型**

```rust
// Before (events.rs:96-100):
LlmCallStart {
    step: usize,
    messages: Vec<crate::messages::BaseMessage>,
    tools: Vec<crate::tools::ToolDefinition>,
},

// After:
LlmCallStart {
    step: usize,
    /// Arc 共享引用——Clone AgentEvent 时为浅拷贝（引用计数 +1），不产生独立副本
    messages: std::sync::Arc<Vec<crate::messages::BaseMessage>>,
    tools: Vec<crate::tools::ToolDefinition>,
},
```

注意：`events.rs` 没有顶层的 `use std::sync::Arc;`（使用 `crate::` 完整路径风格）。`std::sync::Arc` 完整路径与文件现有风格一致。

- [ ] **Step 5: 修改 llm_step.rs emit 侧**

```rust
// Before (llm_step.rs:27-31):
agent.emit(AgentEvent::LlmCallStart {
    step,
    messages: state.messages().to_vec(),
    tools: tool_refs.iter().map(|t| t.definition()).collect(),
});

// After:
agent.emit(AgentEvent::LlmCallStart {
    step,
    messages: Arc::new(state.messages().to_vec()),
    tools: tool_refs.iter().map(|t| t.definition()).collect(),
});
```

`llm_step.rs:1` 已有 `use std::sync::Arc;`，无需新增 import。

- [ ] **Step 6: 验证编译通过**

Run: `cargo build 2>&1 | tail -10`
Expected: 编译成功。消费侧 `&[BaseMessage]` 签名通过 `Deref` 自动兼容 `Arc<Vec<BaseMessage>>`。

---

### Task 3: 更新 mapper 测试

**Files:**
- Modify: `peri-acp/src/event/mapper_test.rs:510-512`

- [ ] **Step 7: 修改 test_llm_call_start_produces_no_output 中的 LlmCallStart 构造**

```rust
// Before (mapper_test.rs:510-512):
&ExecutorEvent::LlmCallStart {
    step: 1,
    messages: vec![BaseMessage::human("hello")],
    tools: vec![ToolDefinition {

// After:
&ExecutorEvent::LlmCallStart {
    step: 1,
    messages: std::sync::Arc::new(vec![BaseMessage::human("hello")]),
    tools: vec![ToolDefinition {
```

检查 mapper_test.rs 文件顶部是否已有 `use std::sync::Arc`。如果没有，需要添加 import 或使用完整路径。如果该文件的 imports 使用了 peri_agent 的 re-export，可用 `peri_agent::AgentEvent` 对应的类型。

- [ ] **Step 8: 搜索其他测试中是否有 LlmCallStart 构造**

Run: `grep -rn "LlmCallStart" --include='*_test.rs' --include='*test*.rs'`
Expected: 仅 `mapper_test.rs:510` 一处需要修改。events_test.rs 中无 LlmCallStart 测试。

**⚠️ 并行 plan 冲突检查**：检查 `docs/superpowers/plans/2026-05-29-h1-mapper-test-coverage.md` 是否已执行。该 plan 的 Task 3 Step 3 也会在 `mapper_test.rs` 中新增 `LlmCallStart` 构造（使用 `messages: vec![]` 而非 `Arc::new(vec![])`）。如果该 plan 已执行，本 Step 7 需覆盖其新增的测试代码（将 `vec![]` 改为 `Arc::new(vec![])`）。如果未执行，则该 plan 执行时需感知本改动。

- [ ] **Step 9: 运行测试验证**

Run: `cargo test -p peri-acp --lib -- llm_call_start 2>&1`
Expected: PASS

Run: `cargo test -p peri-agent --lib -- events 2>&1`
Expected: 所有 events 测试 PASS（serde roundtrip 测试中不涉及 LlmCallStart）

---

### Task 4: 最终验证 + Commit

- [ ] **Step 10: 全量编译 + 测试**

Run: `cargo build && cargo test 2>&1 | tail -10`
Expected: BUILD SUCCEEDED + 所有测试通过

- [ ] **Step 11: Commit**

```bash
git add peri-agent/src/agent/events.rs peri-agent/src/agent/executor/llm_step.rs peri-acp/src/event/mapper_test.rs
git commit -m "perf: change LlmCallStart.messages to Arc<Vec<BaseMessage>>

Replace per-step full history clone with Arc shared reference. For
long conversations (200+ messages), this saves ~6MB/turn by avoiding
repeated Vec<BaseMessage> allocations on every LLM call step.

Consumers are unaffected: Arc<Vec<T>> derefs to &[T], matching
existing &[BaseMessage] signatures in Langfuse tracer, TUI mapper,
and ACP mapper (which discards the event entirely).

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Self-Review

### 1. Spec coverage
- ✅ LlmCallStart.messages 改为 Arc — Task 2 Step 4
- ✅ emit 侧适配 — Task 2 Step 5
- ✅ serde 兼容 — Task 1 Step 1 (rc feature)
- ✅ 测试更新 — Task 3 Step 7
- ✅ Arc 共享语义文档注释 — Task 2 Step 4
- ✅ 并行 plan 冲突检查 — Task 3 Step 8

### 2. Placeholder scan
- 无 TBD / TODO / "implement later"
- 所有代码步骤包含完整代码

### 3. Type consistency
- `events.rs` 定义 `Arc<Vec<BaseMessage>>` ← Step 4
- `llm_step.rs` 构造 `Arc::new(state.messages().to_vec())` ← Step 5
- `mapper_test.rs` 构造 `Arc::new(vec![...])` ← Step 7
- 消费侧 `on_llm_start(step, messages, tools)` 签名 `&[BaseMessage]` — 自动通过 Deref 兼容，无需改动
- ✅ 类型一致

### 4. 已知限制
- Langfuse tracer 启用时，`tracer.rs:350` 的 `messages.to_vec()` 仍有一次深拷贝。这是不可消除的（tracer 需要独立副本用于异步序列化）。Arc 优化仅消除 channel 传递路径的克隆。
- `AgentEvent::Clone` 语义从深拷贝变为浅拷贝（仅 `LlmCallStart.messages` 字段）。当前无代码依赖 clone 后独立修改 messages，但未来开发者需注意 Arc 共享语义（已通过文档注释标注）。
