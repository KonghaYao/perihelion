# App 结构体拆分重构 执行计划

**目标:** 将 `rust-agent-tui` 的 `App` 结构体（40+ 字段）拆分为 4 个职责单一的子结构体，保持对外 API 不变

**技术栈:** Rust, ratatui, tokio mpsc channels

**设计文档:** spec-design.md

---

### Task 1: 定义 AgentComm 子结构体

**涉及文件:**
- 新建: `rust-agent-tui/src/app/agent_comm.rs`

**执行步骤:**
- [x] 创建 `agent_comm.rs`，定义 `AgentComm` 结构体，包含以下 10 个字段：
  - `agent_rx: Option<mpsc::Receiver<AgentEvent>>`
  - `interaction_prompt: Option<InteractionPrompt>`
  - `pending_hitl_items: Option<Vec<String>>`
  - `pending_ask_user: Option<bool>`
  - `agent_state_messages: Vec<BaseMessage>`
  - `agent_id: Option<String>`
  - `cancel_token: Option<AgentCancellationToken>`
  - `task_start_time: Option<std::time::Instant>`
  - `last_task_duration: Option<std::time::Duration>`
  - `agent_event_queue: Vec<AgentEvent>`
- [x] 为 `AgentComm` 实现 `Default` trait，所有字段为 None/空 Vec
- [x] 在 `mod.rs` 中添加 `mod agent_comm;` 声明

**检查步骤:**
- [x] 验证 `agent_comm.rs` 编译通过（仅定义结构体，暂不迁移 App 字段）
  - `cargo build -p rust-agent-tui 2>&1 | grep agent_comm`
  - 预期: 无 agent_comm 相关错误

---

### Task 2: 定义 RelayState 子结构体

**涉及文件:**
- 新建: `rust-agent-tui/src/app/relay_state.rs`

**执行步骤:**
- [x] 创建 `relay_state.rs`，定义 `RelayState` 结构体，包含以下 4 个字段：
  - `relay_client: Option<Arc<RelayClient>>`
  - `relay_event_rx: Option<RelayEventRx>`
  - `relay_params: Option<(String, String, Option<String>, String)>`
  - `relay_reconnect_at: Option<std::time::Instant>`
- [x] 为 `RelayState` 实现 `Default` trait
- [x] 在 `mod.rs` 中添加 `mod relay_state;` 声明

**检查步骤:**
- [x] 验证新增文件编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep relay_state`
  - 预期: 无 relay_state 相关错误

---

### Task 3: 定义 LangfuseState 子结构体

**涉及文件:**
- 新建: `rust-agent-tui/src/app/langfuse_state.rs`

**执行步骤:**
- [x] 创建 `langfuse_state.rs`，定义 `LangfuseState` 结构体，包含以下 3 个字段：
  - `langfuse_session: Option<Arc<crate::langfuse::LangfuseSession>>`
  - `langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>`
  - `langfuse_flush_handle: Option<tokio::task::JoinHandle<()>>`
- [x] 为 `LangfuseState` 实现 `Default` trait
- [x] 在 `mod.rs` 中添加 `mod langfuse_state;` 声明

**检查步骤:**
- [x] 验证新增文件编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep langfuse_state`
  - 预期: 无 langfuse_state 相关错误

---

### Task 4: 定义 AppCore 子结构体

**涉及文件:**
- 新建: `rust-agent-tui/src/app/core.rs`

**执行步骤:**
- [x] 创建 `core.rs`，定义 `AppCore` 结构体，包含以下 ~20 个字段：
  - `view_messages: Vec<MessageViewModel>`
  - `textarea: TextArea<'static>`
  - `loading: bool`
  - `scroll_offset: u16`
  - `scroll_follow: bool`
  - `show_tool_messages: bool`
  - `pending_messages: Vec<String>`
  - `subagent_group_idx: Option<usize>`
  - `render_tx: mpsc::UnboundedSender<RenderEvent>`
  - `render_cache: Arc<RwLock<RenderCache>>`
  - `render_notify: Arc<Notify>`
  - `last_render_version: u64`
  - `command_registry: CommandRegistry`
  - `command_help_list: Vec<(String, String)>`
  - `skills: Vec<SkillMetadata>`
  - `hint_cursor: Option<usize>`
  - `pending_attachments: Vec<PendingAttachment>`
  - `model_panel: Option<ModelPanel>`
  - `agent_panel: Option<AgentPanel>`
  - `thread_browser: Option<ThreadBrowser>`
- [x] 为 `AppCore` 实现 `Default`（注意 `render_tx`/`render_cache`/`render_notify` 需要渲染线程初始化，Default 暂用 `unimplemented!()` 或手动构造）
- [x] 在 `mod.rs` 中添加 `mod core;` 声明

**检查步骤:**
- [x] 验证新增文件编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep "core\.rs"`
  - 预期: 无 core.rs 相关错误

---

### Task 5: 重构 App 为组合结构 + 转发方法

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 将 `App` 结构体改为持有 4 个子结构体 + 不变字段的组合：
  ```rust
  pub struct App {
      pub core: AppCore,
      pub agent: AgentComm,
      pub relay: RelayState,
      pub langfuse: LangfuseState,
      // 不变字段（跨子结构体的"胶水"字段）
      pub cwd: String,
      pub provider_name: String,
      pub model_name: String,
      pub zen_config: Option<ZenConfig>,
      pub thread_store: Arc<dyn ThreadStore>,
      pub current_thread_id: Option<ThreadId>,
      pub todo_items: Vec<TodoItem>,
      pub relay_panel: Option<RelayPanel>,  // UI 面板，非连接状态
  }
  ```
- [x] 为 App 添加高频访问器转发方法（保持 `app.xxx` 调用方式不变）：
  - `loading()` → `self.core.loading`
  - `set_loading(v)` → 设置 `self.core.loading`，重建 textarea
  - `view_messages` → `self.core.view_messages`（pub 字段直接访问）
  - `textarea` → `self.core.textarea`（pub 字段直接访问）
  - `render_tx` → `self.core.render_tx`（pub 字段直接访问）
  - `scroll_offset` / `scroll_follow` → `self.core.xxx`
  - `interaction_prompt` → `self.agent.interaction_prompt`
  - `agent_id` / `get_agent_id()` / `set_agent_id()` → `self.agent.xxx`
  - `cancel_token` → `self.agent.cancel_token`
  - `relay_client` → `self.relay.relay_client`
  - `langfuse_tracer` / `langfuse_session` / `langfuse_flush_handle` → `self.langfuse.xxx`
- [x] 更新 `App::new()` 构造函数：先构建 4 个子结构体，再组合
- [x] 更新 `new_headless()` 测试构造：同样改为子结构体初始化

**检查步骤:**
- [x] 验证 `App` 结构体顶层字段数 ≤ 12
  - `grep -c 'pub [a-z_]*:' rust-agent-tui/src/app/mod.rs | head`
  - 预期: App 顶层字段 7-12 个（4 个子结构体 + 胶水字段）
- [x] 编译通过，无 warning（ops 文件迁移将在 Task 6 完成）
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "warning|error" | head -20`

---

### Task 6: 迁移 ops 文件内部访问路径

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`
- 修改: `rust-agent-tui/src/app/hitl_ops.rs`
- 修改: `rust-agent-tui/src/app/ask_user_ops.rs`
- 修改: `rust-agent-tui/src/app/relay_ops.rs`
- 修改: `rust-agent-tui/src/app/thread_ops.rs`
- 修改: `rust-agent-tui/src/app/panel_ops.rs`
- 修改: `rust-agent-tui/src/app/hint_ops.rs`
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/popups/*.rs`
- 修改: `rust-agent-tui/src/main.rs`

**执行步骤:**
- [ ] **agent_ops.rs** — 核心迁移（最复杂）：
  - `self.agent_rx` → `self.agent.agent_rx`
  - `self.interaction_prompt` → `self.agent.interaction_prompt`
  - `self.pending_hitl_items` → `self.agent.pending_hitl_items`
  - `self.pending_ask_user` → `self.agent.pending_ask_user`
  - `self.agent_state_messages` → `self.agent.agent_state_messages`
  - `self.agent_id` → `self.agent.agent_id`
  - `self.cancel_token` → `self.agent.cancel_token`
  - `self.task_start_time` → `self.agent.task_start_time`
  - `self.last_task_duration` → `self.agent.last_task_duration`
  - `self.subagent_group_idx` → `self.core.subagent_group_idx`
  - `self.langfuse_tracer` → `self.langfuse.langfuse_tracer`
  - `self.langfuse_session` → `self.langfuse.langfuse_session`
  - `self.langfuse_flush_handle` → `self.langfuse.langfuse_flush_handle`
  - `self.relay_client` → `self.relay.relay_client`
- [ ] **hitl_ops.rs** — `self.interaction_prompt` → `self.agent.interaction_prompt`，`self.pending_hitl_items` → `self.agent.pending_hitl_items`，`self.relay_client` → `self.relay.relay_client`
- [ ] **ask_user_ops.rs** — `self.interaction_prompt` → `self.agent.interaction_prompt`，`self.pending_ask_user` → `self.agent.pending_ask_user`，`self.relay_client` → `self.relay.relay_client`
- [ ] **relay_ops.rs** — `self.relay_client` → `self.relay.relay_client`，`self.relay_event_rx` → `self.relay.relay_event_rx`，`self.relay_params` → `self.relay.relay_params`，`self.relay_reconnect_at` → `self.relay.relay_reconnect_at`
- [ ] **thread_ops.rs** — `self.agent_rx` → `self.agent.agent_rx`，`self.agent_state_messages` → `self.agent.agent_state_messages`，`self.langfuse_session` → `self.langfuse.langfuse_session`，`self.relay_client` → `self.relay.relay_client`
- [ ] **panel_ops.rs** — `self.agent_id` → `self.agent.agent_id`，`self.relay_client` → `self.relay.relay_client`，`self.relay_params` → `self.relay.relay_params`
- [ ] **hint_ops.rs** — 无需修改（只访问 `self.core` 字段，通过 pub 直接访问已兼容）
- [ ] **event.rs** — `app.interaction_prompt` → `app.agent.interaction_prompt`，`app.agent_id` → `app.agent.agent_id`（如有直接访问），`app.relay_client` → `app.relay.relay_client`（如有直接访问）
- [ ] **ui/main_ui.rs** — `app.view_messages` / `app.loading` / `app.textarea` / `app.scroll_offset` / `app.scroll_follow` 等通过转发方法访问保持不变；`app.interaction_prompt` → `app.agent.interaction_prompt`（如有直接字段访问）
- [ ] **ui/main_ui/status_bar.rs** — `app.get_agent_id()` / `app.get_current_task_duration()` 保持不变（转发方法）
- [ ] **ui/main_ui/popups/*.rs** — `app.interaction_prompt` → 通过转发方法或 `app.agent.interaction_prompt`
- [ ] **main.rs** — `app.langfuse_flush_handle` → `app.langfuse.langfuse_flush_handle`

**检查步骤:**
- [ ] 全量编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error
- [ ] 全量编译无新 warning
  - `cargo build -p rust-agent-tui 2>&1 | grep -c "warning"`
  - 预期: warning 数量不增加（或减少）
- [ ] 全量测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 所有测试通过
- [ ] Headless 测试通过
  - `cargo test -p rust-agent-tui --features headless 2>&1 | tail -10`
  - 预期: 所有 headless 测试通过

---

### Task 7: App Refactor Acceptance

**Prerequisites:**
- Start command: `cargo build -p rust-agent-tui`
- Test command: `cargo test -p rust-agent-tui`

**End-to-end verification:**

1. App 结构体顶层字段数验证
   - `grep -E '^\s+pub [a-z_]+:' rust-agent-tui/src/app/mod.rs | wc -l`
   - Expected: ≤ 12（4 个子结构体 + 8 个胶水字段）
   - On failure: 检查 Task 5 的字段归属

2. 每个子结构体字段数验证
   - `grep -E '^\s+pub [a-z_]+:' rust-agent-tui/src/app/core.rs | wc -l`
   - `grep -E '^\s+pub [a-z_]+:' rust-agent-tui/src/app/agent_comm.rs | wc -l`
   - `grep -E '^\s+pub [a-z_]+:' rust-agent-tui/src/app/relay_state.rs | wc -l`
   - `grep -E '^\s+pub [a-z_]+:' rust-agent-tui/src/app/langfuse_state.rs | wc -l`
   - Expected: core ≤ 20, agent_comm ≤ 15, relay_state ≤ 6, langfuse_state ≤ 5
   - On failure: 检查 Task 1-4 的字段列表

3. 全量测试通过
   - `cargo test 2>&1 | tail -5`
   - Expected: "test result: ok" 且无失败
   - On failure: 检查 Task 6 的字段迁移是否遗漏

4. 编译无新 warning
   - `cargo build -p rust-agent-tui 2>&1 | grep "warning" | wc -l`
   - Expected: 无新增 warning（与重构前对比）
   - On failure: 检查是否有未使用的转发方法或字段

5. Headless 测试通过
   - `cargo test -p rust-agent-tui --features headless 2>&1 | grep -E "test result|FAILED"`
   - Expected: 所有测试通过，无 FAILED
   - On failure: 检查 `new_headless()` 构造是否正确初始化子结构体
