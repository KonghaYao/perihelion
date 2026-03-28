# H2: ThreadStore 与 AgentState 合并 执行计划

**目标:** 给 `AgentState` 增加可选持久化后端，`add_message` 时自动触发写入，消除 TUI 中手动调用 `append_messages` 的隐式契约

**技术栈:** Rust / async_trait / tokio / rust-create-agent / rust-agent-tui

**设计文档:** Plan-H2-thread-store.md（项目根目录）

---

### Task 1: ThreadStore 新增 append_message 默认方法

**涉及文件:**
- 修改: `rust-create-agent/src/thread/store.rs`

**执行步骤:**
- [x] 在 `ThreadStore` trait 中新增 `append_message`（单条）异步默认方法：
  - 签名：`async fn append_message(&self, thread_id: &ThreadId, message: BaseMessage) -> Result<()>`
  - 默认实现：调用 `self.append_messages(thread_id, &[message])`，复用现有批量追加逻辑，无需各具体实现重写

**检查步骤:**
- [x] 核心库编译无报错
  - `cargo build -p rust-create-agent 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] append_message 方法存在于 store.rs
  - `grep -n "append_message" rust-create-agent/src/thread/store.rs`
  - 预期: 找到至少 1 处

---

### Task 2: AgentState 支持自动持久化

**涉及文件:**
- 修改: `rust-create-agent/src/agent/state.rs`

**执行步骤:**
- [x] 在 `AgentState` 结构体中新增两个字段（用 `#[serde(skip)]` 标注，不参与序列化）：
  ```rust
  use std::sync::Arc;
  use crate::thread::store::ThreadStore;
  use crate::thread::types::ThreadId;

  #[serde(skip)]
  store: Option<Arc<dyn ThreadStore>>,
  #[serde(skip)]
  thread_id: Option<ThreadId>,
  ```
  - `Default` derive 仍可用（`Option` 默认为 `None`）
- [x] 新增 `with_persistence` builder 方法：
  ```rust
  pub fn with_persistence(mut self, store: Arc<dyn ThreadStore>, thread_id: impl Into<String>) -> Self {
      self.store = Some(store);
      self.thread_id = Some(thread_id.into());
      self
  }
  ```
- [x] 修改 `State::add_message` 实现，在 `messages.push` 之后添加 fire-and-forget 持久化：
  ```rust
  if let (Some(store), Some(tid)) = (self.store.clone(), self.thread_id.clone()) {
      let msg = message.clone(); // message 已在 push 前 clone
      tokio::spawn(async move {
          if let Err(e) = store.append_message(&tid, msg).await {
              tracing::warn!("auto-persist message failed: {e}");
          }
      });
  }
  ```
  - 注意：`message.clone()` 放在 `push` 之前，`push` 继续使用原值

**检查步骤:**
- [x] 核心库编译无报错
  - `cargo build -p rust-create-agent 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] with_persistence 方法存在
  - `grep -n "with_persistence" rust-create-agent/src/agent/state.rs`
  - 预期: 找到至少 1 处
- [x] 现有测试全部通过
  - `cargo test -p rust-create-agent 2>&1 | grep -E "FAILED|test result"`
  - 预期: 所有 `test result: ok`，无 `FAILED`

---

### Task 3: TUI 接入 with_persistence 并删除手动同步

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 在 `AgentRunConfig` 中新增两个字段：
  ```rust
  pub thread_store: Arc<dyn rust_create_agent::thread::store::ThreadStore>,
  pub thread_id: rust_create_agent::thread::types::ThreadId,
  ```
- [x] 在 `run_universal_agent` 中，创建 `AgentState` 时链式调用 `with_persistence`：
  ```rust
  let mut state = AgentState::with_messages(cwd, history)
      .with_persistence(Arc::clone(&thread_store), thread_id.clone());
  ```
  - 同时从 `AgentRunConfig` 解构时包含 `thread_store` 和 `thread_id`
- [x] 在 `agent_ops.rs` 的 `submit_message` 函数中，构造 `AgentRunConfig` 时补充两个新字段：
  ```rust
  thread_store: self.thread_store.clone(),
  thread_id: thread_id.clone(),
  ```
  - `thread_id` 已由 `ensure_thread_id()` 返回
- [x] 在 `agent_ops.rs` 的 `AgentEvent::StateSnapshot` 处理分支中，删除手动持久化代码块（`if let Some(id) = self.current_thread_id.clone() { ... append_messages ... }` 整个块），只保留 `self.agent_state_messages.extend(msgs);`
- [x] 在 `mod.rs` 中删除 `persisted_count: usize` 字段（仅写不读，已为死代码）及其所有初始化/赋值处（`agent_ops.rs` 和 `thread_ops.rs` 中的赋值语句一并删除）

**检查步骤:**
- [x] TUI 编译无报错
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] 手动 append_messages 调用已删除
  - `grep -rn "append_messages" rust-agent-tui/src/`
  - 预期: 无输出
- [x] persisted_count 已删除
  - `grep -rn "persisted_count" rust-agent-tui/src/`
  - 预期: 无输出
- [x] with_persistence 调用存在于 agent.rs
  - `grep -n "with_persistence" rust-agent-tui/src/app/agent.rs`
  - 预期: 找到 1 处
- [x] 全量编译无报错
  - `cargo build 2>&1 | grep -E "^error"`
  - 预期: 无输出

---

### Task 4: H2 Thread Store Acceptance

**前置条件:**
- 全量构建: `cargo build 2>&1 | grep -E "^error"`（应无输出）
- 全量测试: `cargo test -p rust-create-agent -p rust-agent-middlewares -p rust-agent-tui 2>&1 | grep -E "FAILED|test result"`

**端到端验证:**

1. **append_message 默认实现正确调用 append_messages**
   - `grep -n "append_message\b" rust-create-agent/src/thread/store.rs`
   - Expected: 找到方法定义，默认实现调用 `append_messages`
   - On failure: 检查 Task 1

2. **AgentState 具备 with_persistence 且 store/thread_id 字段标注 serde(skip)**
   - `grep -n "serde(skip)\|with_persistence\|store:\|thread_id:" rust-create-agent/src/agent/state.rs`
   - Expected: 找到 `#[serde(skip)]`、`with_persistence`、`store`、`thread_id` 各至少 1 处
   - On failure: 检查 Task 2

3. **AgentState::add_message 包含 tokio::spawn 自动持久化逻辑**
   - `grep -n "tokio::spawn\|append_message" rust-create-agent/src/agent/state.rs`
   - Expected: 找到 `tokio::spawn` 和 `append_message` 调用各 1 处
   - On failure: 检查 Task 2

4. **TUI 不再有手动 append_messages 调用**
   - `grep -rn "append_messages" rust-agent-tui/src/`
   - Expected: 无输出
   - On failure: 检查 Task 3

5. **全量测试无回归**
   - `cargo test -p rust-create-agent -p rust-agent-middlewares -p rust-agent-tui 2>&1 | grep -E "FAILED|test result"`
   - Expected: 所有 `test result: ok`，无 `FAILED`
   - On failure: 根据失败 crate 对应检查 Task 1-3
