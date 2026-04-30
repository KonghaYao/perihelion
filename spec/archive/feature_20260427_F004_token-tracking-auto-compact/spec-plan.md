# Token 用量追踪与 Auto-Compact 机制 执行计划

**目标:** 在核心层新增 TokenTracker 累积追踪 token 用量，TUI 层展示上下文使用百分比，并在上下文接近满时自动触发 compact（micro-compact + full compact 两级策略）。

**技术栈:** Rust 2021, tokio, serde, ratatui, mpsc channel

**设计文档:** spec-design.md

## 改动总览

- 核心层新建 `rust-create-agent/src/agent/token.rs`（TokenTracker + ContextBudget + micro_compact），修改 `state.rs`（集成 token_tracker）、`executor.rs`（自动 accumulate）、`react.rs`（新增 context_window）、`react_adapter.rs`（模型映射表）、`events.rs`（新增 ContextWarning）、`mod.rs` / `lib.rs`（导出）
- TUI 层修改 `events.rs`（新增 TokenUsageUpdate 事件）、`agent.rs`（map_executor_event 转发 LlmCallEnd）、`agent_comm.rs`（token 追踪状态）、`agent_ops.rs`（处理 token 事件 + auto-compact 触发）、`status_bar.rs`（上下文百分比展示）
- Task 1 创建数据模型，Task 2 的 AgentState 集成依赖 Task 1 的 TokenTracker，Task 3 的 context_window 独立于 Task 1/2，Task 4 的 TUI 数据流依赖 Task 2 的 accumulate 输出，Task 5 的 auto-compact 依赖 Task 3 的 context_window + Task 4 的 Done 事件集成
- 关键设计决策：`map_executor_event()` 当前将 `LlmCallEnd` 映射为 `None`（agent.rs:279），本方案将其改为转发为 TUI `TokenUsageUpdate` 事件以实现数据流贯通

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - 运行: `cargo build -p rust-create-agent`
  - 预期: 编译成功，无错误

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无 error
- [x] 测试命令可用
  - `cargo test -p rust-create-agent --lib -- test_agent_state_new 2>&1 | tail -5`
  - 预期: 测试通过

---

### Task 1: TokenTracker + ContextBudget 核心数据模型

**背景:**
当前 `TokenUsage`（`rust-create-agent/src/llm/types.rs`）仅随 `LlmCallEnd` 事件发出后即丢弃，无会话级累计。本 Task 新建 `TokenTracker` 结构体累积多轮 LLM 调用的 token 用量，新建 `ContextBudget` 结构体封装上下文窗口阈值配置。Task 2（AgentState 集成）和 Task 4（TUI 状态栏）均依赖本 Task 的数据模型。

**涉及文件:**
- 新建: `rust-create-agent/src/agent/token.rs`
- 修改: `rust-create-agent/src/agent/mod.rs`
- 修改: `rust-create-agent/src/lib.rs`

**执行步骤:**
- [x] 新建 `rust-create-agent/src/agent/token.rs`，定义 `TokenTracker` 结构体
  - 位置: 新文件 `rust-create-agent/src/agent/token.rs`
  - 引入 `use crate::llm::types::TokenUsage;`
  - 结构体定义:
    ```rust
    /// 会话级 token 用量追踪器
    #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
    pub struct TokenTracker {
        /// 累计输入 token（含 cache_read + cache_creation）
        pub total_input_tokens: u64,
        /// 累计输出 token
        pub total_output_tokens: u64,
        /// 累计 cache_creation token
        pub total_cache_creation_tokens: u64,
        /// 累计 cache_read token
        pub total_cache_read_tokens: u64,
        /// 最近一次 LLM 响应的 usage（用于估算当前上下文大小）
        pub last_usage: Option<TokenUsage>,
        /// 已完成的 LLM 调用次数
        pub llm_call_count: u32,
    }
    ```
  - 实现 `accumulate(&mut self, usage: &TokenUsage)` 方法:
    ```rust
    pub fn accumulate(&mut self, usage: &TokenUsage) {
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        if let Some(v) = usage.cache_creation_input_tokens {
            self.total_cache_creation_tokens += v as u64;
        }
        if let Some(v) = usage.cache_read_input_tokens {
            self.total_cache_read_tokens += v as u64;
        }
        self.last_usage = Some(usage.clone());
        self.llm_call_count += 1;
    }
    ```
  - 实现 `estimated_context_tokens(&self) -> Option<u64>` 方法:
    ```rust
    pub fn estimated_context_tokens(&self) -> Option<u64> {
        self.last_usage.as_ref().map(|u| {
            u.input_tokens as u64
                + u.output_tokens as u64
                + u.cache_creation_input_tokens.unwrap_or(0) as u64
                + u.cache_read_input_tokens.unwrap_or(0) as u64
        })
    }
    ```
  - 实现 `context_usage_percent(&self, context_window: u32) -> Option<f64>` 方法:
    ```rust
    pub fn context_usage_percent(&self, context_window: u32) -> Option<f64> {
        self.estimated_context_tokens()
            .map(|used| (used as f64 / context_window as f64) * 100.0)
    }
    ```
  - 原因: `TokenUsage` 的字段类型为 `u32` / `Option<u32>`（经确认 `llm/types.rs:43-49`），累积到 `u64` 防止多轮溢出

- [x] 在同一文件中定义 `ContextBudget` 结构体
  - 位置: `rust-create-agent/src/agent/token.rs`，`TokenTracker` 定义之后
  - 结构体定义:
    ```rust
    /// 上下文窗口预算配置
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct ContextBudget {
        /// 模型的上下文窗口大小（token 数）
        pub context_window: u32,
        /// auto-compact 触发阈值（百分比，0.0-1.0）
        pub auto_compact_threshold: f64,
        /// 警告阈值（百分比，0.0-1.0）
        pub warning_threshold: f64,
    }
    ```
  - 定义常量:
    ```rust
    impl ContextBudget {
        pub const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;
        pub const DEFAULT_AUTO_COMPACT_THRESHOLD: f64 = 0.85;
        pub const DEFAULT_WARNING_THRESHOLD: f64 = 0.70;
    }
    ```
  - 实现 `new(context_window: u32) -> Self`:
    ```rust
    pub fn new(context_window: u32) -> Self {
        Self {
            context_window,
            auto_compact_threshold: Self::DEFAULT_AUTO_COMPACT_THRESHOLD,
            warning_threshold: Self::DEFAULT_WARNING_THRESHOLD,
        }
    }
    ```
  - 实现 `should_auto_compact(&self, tracker: &TokenTracker) -> bool`:
    ```rust
    pub fn should_auto_compact(&self, tracker: &TokenTracker) -> bool {
        match tracker.context_usage_percent(self.context_window) {
            Some(pct) => pct / 100.0 >= self.auto_compact_threshold,
            None => false,
        }
    }
    ```
  - 实现 `should_warn(&self, tracker: &TokenTracker) -> bool`:
    ```rust
    pub fn should_warn(&self, tracker: &TokenTracker) -> bool {
        match tracker.context_usage_percent(self.context_window) {
            Some(pct) => pct / 100.0 >= self.warning_threshold,
            None => false,
        }
    }
    ```

- [x] 注册 `token` 模块到 `agent/mod.rs`
  - 位置: `rust-create-agent/src/agent/mod.rs`，在 `pub mod state;` 之后新增 `pub mod token;`
  - 在 `pub use` 块末尾新增:
    ```rust
    pub use token::{ContextBudget, TokenTracker};
    ```

- [x] 导出 `TokenTracker` 和 `ContextBudget` 到 prelude
  - 位置: `rust-create-agent/src/lib.rs`，在 `pub use crate::agent::{...}` 块内，`state::{AgentState, State},` 之后新增:
    ```rust
    token::{ContextBudget, TokenTracker},
    ```

- [x] 为 `TokenTracker` 和 `ContextBudget` 编写单元测试
  - 测试文件: `rust-create-agent/src/agent/token.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_accumulate_sums_tokens`: 累加两次 TokenUsage → 验证 total_input_tokens、total_output_tokens、total_cache_creation_tokens、total_cache_read_tokens、llm_call_count 正确
    - `test_accumulate_with_none_cache`: cache 字段为 None 的 TokenUsage → 验证 cache 累计字段保持 0
    - `test_estimated_context_tokens_none`: 空 tracker（无 last_usage）→ 验证返回 None
    - `test_estimated_context_tokens_some`: 有 last_usage 的 tracker → 验证返回 input + output + cache_creation + cache_read 的总和
    - `test_context_usage_percent`: context_window=200000，estimated=100000 → 验证返回 50.0
    - `test_context_budget_should_auto_compact`: 85% 时 should_auto_compact 返回 true，80% 返回 false
    - `test_context_budget_should_warn`: 70% 时 should_warn 返回 true，60% 返回 false
    - `test_context_budget_new_uses_defaults`: ContextBudget::new(128000) → 验证 threshold 使用默认值
  - 运行命令: `cargo test -p rust-create-agent --lib -- agent::token::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无 error
- [x] TokenTracker 和 ContextBudget 可通过 prelude 导入
  - `grep -n "TokenTracker\|ContextBudget" rust-create-agent/src/lib.rs`
  - 预期: 找到 prelude 中的导出行
- [x] 单元测试全部通过
  - `cargo test -p rust-create-agent --lib -- agent::token::tests 2>&1 | tail -15`
  - 预期: 所有 test 结果为 `ok`，无 FAILED

---

### Task 2: AgentState 集成 + executor 自动 accumulate

**背景:**
`AgentState`（`state.rs`）当前无 token 感知能力，每轮 LLM 调用的 usage 随 `LlmCallEnd` 事件发出后即丢弃。本 Task 在 `AgentState` 中新增 `token_tracker` 字段，并在 `ReActAgent::execute()` 中每轮 LLM 调用成功后自动调用 `state.token_tracker_mut().accumulate()`，使会话级 token 追踪无需应用层手动处理。Task 3（context_window）和 Task 4（TUI 数据流）依赖本 Task 的追踪数据。

**涉及文件:**
- 修改: `rust-create-agent/src/agent/state.rs`
- 修改: `rust-create-agent/src/agent/executor.rs`

**执行步骤:**
- [x] 在 `AgentState` 中新增 `token_tracker` 字段
  - 位置: `rust-create-agent/src/agent/state.rs` AgentState 结构体（第 29-41 行之间）
  - 在 `pub context: HashMap<String, String>,` 之后追加: `pub token_tracker: crate::agent::token::TokenTracker,`
  - Default derive 会自动初始化为 `TokenTracker::default()`（零值）
  - 在 `Debug` impl（第 43-54 行）中追加 `.field("token_tracker", &self.token_tracker)` — 在 `thread_id` 行之后

- [x] 在 `State` trait 中新增 token_tracker 访问器
  - 位置: `rust-create-agent/src/agent/state.rs` State trait（第 10-25 行）
  - 在 `fn set_context(...)` 之后追加:
    ```rust
    fn token_tracker(&self) -> &crate::agent::token::TokenTracker;
    fn token_tracker_mut(&mut self) -> &mut crate::agent::token::TokenTracker;
    ```

- [x] 在 `AgentState` 的 `State` impl 中实现两个新方法
  - 位置: `rust-create-agent/src/agent/state.rs` 的 `impl State for AgentState` 块末尾（第 150-152 行 `set_context` 之后）
  - 追加:
    ```rust
    fn token_tracker(&self) -> &crate::agent::token::TokenTracker {
        &self.token_tracker
    }
    fn token_tracker_mut(&mut self) -> &mut crate::agent::token::TokenTracker {
        &mut self.token_tracker
    }
    ```

- [x] 在 `ReActAgent::execute()` 中 LLM 调用成功后自动 accumulate
  - 位置: `rust-create-agent/src/agent/executor.rs` 的 `execute()` 方法中 emit `LlmCallEnd` 块之后（第 188-194 行之后）
  - 在 `self.emit(AgentEvent::LlmCallEnd { ... });` 闭合大括号 `}` 之后、`if reasoning.needs_tool_call()` 之前，追加:
    ```rust
    // 自动累积 token 用量到 state
    if let Some(ref usage) = reasoning.usage {
        state.token_tracker_mut().accumulate(usage);
    }
    ```

- [x] 为 AgentState 的 token_tracker 字段编写单元测试
  - 测试文件: `rust-create-agent/src/agent/state.rs` 底部 `#[cfg(test)] mod tests`（第 155 行之后）
  - 测试场景:
    - `test_token_tracker_default`: 新建 `AgentState::new("/tmp")`，验证 `token_tracker.llm_call_count == 0` 且 `total_input_tokens == 0`
    - `test_token_tracker_accumulate`: 创建 state，调用 `state.token_tracker_mut().accumulate(&TokenUsage { input_tokens: 100, output_tokens: 50, cache_creation_input_tokens: Some(30), cache_read_input_tokens: None })`，验证 `total_input_tokens == 100` 和 `llm_call_count == 1`
  - 运行命令: `cargo test -p rust-create-agent --lib -- agent::state::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -3`
  - 预期: `Finished`，无 error
- [x] State trait 包含新方法
  - `grep -n "token_tracker" rust-create-agent/src/agent/state.rs`
  - 预期: 找到 trait 定义和 impl
- [x] executor 中有 accumulate 调用
  - `grep -n "token_tracker_mut" rust-create-agent/src/agent/executor.rs`
  - 预期: 找到 1 行

---

### Task 3: ReactLLM context_window + 模型映射表

**背景:**
`ReactLLM` trait（`react.rs`）当前仅有 `generate_reasoning()` 和 `model_name()` 方法，不提供上下文窗口大小信息。TUI 层无法知道当前模型的 context window 大小，无法计算"还剩多少空间"。本 Task 在 `ReactLLM` trait 中新增 `context_window()` 默认方法（返回 200K），并在 `BaseModelReactLLM` 中基于 `model_id()` 实现模型→窗口映射。Task 5（auto-compact 触发）依赖本 Task 的 `context_window()` 输出。

**涉及文件:**
- 修改: `rust-create-agent/src/agent/react.rs`
- 修改: `rust-create-agent/src/llm/react_adapter.rs`

**执行步骤:**
- [x] 在 `ReactLLM` trait 中新增 `context_window()` 默认方法
  - 位置: `rust-create-agent/src/agent/react.rs` ReactLLM trait（第 163-175 行）
  - 在 `fn model_name(&self) -> String { "unknown".to_string() }` 之后追加:
    ```rust
    /// 返回模型的上下文窗口大小（token 数），默认 200K
    fn context_window(&self) -> u32 {
        200_000
    }
    ```

- [x] 在 `Box<dyn ReactLLM + Send + Sync>` 的 blanket impl 中转发 `context_window()`
  - 位置: `rust-create-agent/src/agent/react.rs` 的 blanket impl（第 178-191 行）
  - 在 `fn model_name(&self) -> String { (**self).model_name() }` 之后追加:
    ```rust
    fn context_window(&self) -> u32 {
        (**self).context_window()
    }
    ```

- [x] 在 `BaseModelReactLLM` 中实现 `context_window()` 基于模型名称映射
  - 位置: `rust-create-agent/src/llm/react_adapter.rs` 的 `impl ReactLLM for BaseModelReactLLM`（第 28-100 行）
  - 在 `fn model_name(&self) -> String` 方法（第 97-99 行）之后追加:
    ```rust
    fn context_window(&self) -> u32 {
        let model = self.model.model_id();
        // Claude 系列: 200K
        if model.contains("claude") { return 200_000; }
        // DeepSeek 系列: 128K
        if model.starts_with("deepseek") { return 128_000; }
        // GPT-4o / o-series: 128K
        if model.contains("gpt-4o") || model.starts_with("o1") || model.starts_with("o3") { return 128_000; }
        // GPT-4-turbo: 128K
        if model.contains("gpt-4-turbo") { return 128_000; }
        // 默认: 200K
        200_000
    }
    ```

- [x] 为 context_window 映射编写单元测试
  - 测试文件: `rust-create-agent/src/llm/react_adapter.rs` 底部新增 `#[cfg(test)] mod tests`
  - 创建 mock `BaseModel` 实现:
    ```rust
    struct MockBaseModel { id: &'static str }
    #[async_trait::async_trait]
    impl super::BaseModel for MockBaseModel {
        async fn invoke(&self, _: super::types::LlmRequest) -> crate::error::AgentResult<super::types::LlmResponse> { unimplemented!() }
        fn provider_name(&self) -> &str { "mock" }
        fn model_id(&self) -> &str { self.id }
    }
    ```
  - 测试场景:
    - `test_context_window_claude`: `model_id = "claude-sonnet-4-20250514"` → `200_000`
    - `test_context_window_deepseek`: `model_id = "deepseek-r1"` → `128_000`
    - `test_context_window_gpt4o`: `model_id = "gpt-4o"` → `128_000`
    - `test_context_window_default`: `model_id = "unknown-model"` → `200_000`
  - 运行命令: `cargo test -p rust-create-agent --lib -- llm::react_adapter::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] ReactLLM trait 包含 context_window 方法
  - `grep -n "context_window" rust-create-agent/src/agent/react.rs`
  - 预期: 找到 trait 定义 + blanket impl 转发
- [x] BaseModelReactLLM 有映射实现
  - `grep -n "context_window" rust-create-agent/src/llm/react_adapter.rs`
  - 预期: 找到实现
- [x] 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -3`
  - 预期: `Finished`

---

### Task 4: TUI Token 数据流 + 状态栏展示

**背景:**
当前 `map_executor_event()`（`agent.rs:279`）将 `LlmCallEnd` 映射为 `None`，TUI 层完全收不到 token usage 数据。本 Task 实现完整数据流：核心层 executor emit `LlmCallEnd` → `map_executor_event` 转发为新 TUI 事件 `TokenUsageUpdate` → `handle_agent_event` 处理并更新状态 → 状态栏展示上下文使用百分比。同时扩展核心层 `AgentEvent` 新增 `ContextWarning` 变体。Task 5 的 auto-compact 触发依赖本 Task 在 TUI 层建立的 token 追踪状态。

**涉及文件:**
- 修改: `rust-create-agent/src/agent/events.rs`（新增 ContextWarning 变体）
- 修改: `rust-agent-tui/src/app/events.rs`（新增 TokenUsageUpdate 事件）
- 修改: `rust-agent-tui/src/app/agent.rs`（map_executor_event 转发 LlmCallEnd）
- 修改: `rust-agent-tui/src/app/agent_comm.rs`（新增 token 追踪状态字段）
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（处理 TokenUsageUpdate 事件）
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`（展示上下文百分比）
- 修改: `rust-agent-tui/src/app/thread_ops.rs`（compact 后重置 tracker）

**执行步骤:**
- [x] 在核心层 `AgentEvent` 中新增 `ContextWarning` 变体
  - 位置: `rust-create-agent/src/agent/events.rs` AgentEvent 枚举（第 4-32 行）
  - 在 `LlmCallEnd { ... },` 之后追加:
    ```rust
    /// 上下文窗口使用警告（阈值触发时发出）
    ContextWarning {
        used_tokens: u64,
        total_tokens: u64,
        percentage: f64,
    },
    ```

- [x] 在 TUI 层 `AgentEvent` 中新增 `TokenUsageUpdate` 变体
  - 位置: `rust-agent-tui/src/app/events.rs` AgentEvent 枚举（第 6-45 行）
  - 在 `SubAgentEnd { ... },` 之后追加:
    ```rust
    /// Token 使用量更新（从核心层 LlmCallEnd 映射而来）
    TokenUsageUpdate {
        usage: rust_create_agent::llm::types::TokenUsage,
        model: String,
    },
    ```

- [x] 修改 `map_executor_event` 转发 `LlmCallEnd`
  - 位置: `rust-agent-tui/src/app/agent.rs` 的 `map_executor_event` 函数（第 273-280 行的 match 分支）
  - 将第 278-279 行的 `ExecutorEvent::LlmCallEnd { .. } => return None,` 替换为:
    ```rust
    ExecutorEvent::LlmCallEnd { usage: Some(usage), model, .. } => {
        AgentEvent::TokenUsageUpdate { usage, model }
    }
    ExecutorEvent::LlmCallEnd { usage: None, .. } => return None,
    ```

- [x] 在 `AgentComm` 中新增 token 追踪状态字段
  - 位置: `rust-agent-tui/src/app/agent_comm.rs` AgentComm 结构体（第 14-34 行）
  - 在 `pub agent_event_queue: Vec<AgentEvent>,` 之后追加:
    ```rust
    /// 会话级 token 累积追踪（从 AgentEvent::TokenUsageUpdate 聚合）
    pub session_token_tracker: rust_create_agent::agent::token::TokenTracker,
    /// 当前模型的上下文窗口大小（从最近一次 TokenUsageUpdate 中的 model 推断）
    pub context_window: u32,
    /// 是否需要 auto-compact（在 LlmCallEnd 时标记，Done 时执行）
    pub needs_auto_compact: bool,
    /// 连续 auto-compact 失败次数（circuit breaker，达到 3 次后停止自动触发）
    pub auto_compact_failures: u32,
    ```
  - 在 `Default` impl 中追加对应默认值:
    ```rust
    session_token_tracker: rust_create_agent::agent::token::TokenTracker::default(),
    context_window: 200_000,
    needs_auto_compact: false,
    auto_compact_failures: 0,
    ```

- [x] 在 `handle_agent_event` 中处理 `TokenUsageUpdate` 事件
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` 的 `handle_agent_event` 方法 match 块
  - 在 `AgentEvent::SubAgentEnd { ... }` 分支之后、`AgentEvent::ToolCall { ... }` 分支之前追加:
    ```rust
    AgentEvent::TokenUsageUpdate { usage, model } => {
        // 累积到会话追踪器
        self.agent.session_token_tracker.accumulate(&usage);
        // circuit breaker: 连续 3 次失败后不再自动触发
        if self.agent.auto_compact_failures < 3 {
            let budget = rust_create_agent::agent::token::ContextBudget::new(self.agent.context_window);
            if budget.should_auto_compact(&self.agent.session_token_tracker) {
                self.agent.needs_auto_compact = true;
            }
        }
        (true, false, false)
    }
    ```

- [x] 在 `AgentEvent::Done` 分支中集成 auto-compact 触发
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` 的 `AgentEvent::Done` 分支（第 286-318 行）
  - 在 `self.agent.agent_rx = None;`（第 301 行）之后、`self.core.subagent_group_idx = None;`（第 303 行）之前追加:
    ```rust
    // Auto-compact 两级策略
    if self.agent.needs_auto_compact {
        self.agent.needs_auto_compact = false;
        tracing::info!("auto-compact: context threshold reached, triggering full compact");
        self.start_compact("auto".to_string());
    } else {
        // 70%-85% 区间: micro-compact
        let budget = rust_create_agent::agent::token::ContextBudget::new(self.agent.context_window);
        if budget.should_warn(&self.agent.session_token_tracker) {
            self.start_micro_compact();
        }
    }
    ```

- [x] 在 `start_compact` 时重置 session token tracker
  - 位置: `rust-agent-tui/src/app/thread_ops.rs` 的 `start_compact()` 函数（第 129 行起）
  - 在 `self.set_loading(true);`（第 163 行）之后追加:
    ```rust
    self.agent.session_token_tracker.reset();
    ```

- [x] 在 `CompactDone` 分支中重置失败计数
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` 的 `AgentEvent::CompactDone` 分支
  - 在 `self.langfuse.langfuse_session = None;` 之后追加:
    ```rust
    self.agent.auto_compact_failures = 0;
    ```

- [x] 在 `CompactError` 分支中递增失败计数
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` 的 `AgentEvent::CompactError` 分支
  - 在 `self.agent.agent_rx = None;` 之后追加:
    ```rust
    self.agent.auto_compact_failures += 1;
    ```

- [x] 在状态栏中展示上下文使用百分比
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs`（第 86 行"消息计数" `left_spans.push(Span::styled(" │ "...))` 之前）
  - 在消息计数的分隔符之前，追加上下文展示段:
    ```rust
    // 上下文使用百分比
    {
        let tracker = &app.agent.session_token_tracker;
        if let Some(pct) = tracker.context_usage_percent(app.agent.context_window) {
            let used = tracker.estimated_context_tokens().unwrap_or(0);
            let total = app.agent.context_window;
            let color = if pct >= 85.0 {
                theme::ERROR
            } else if pct >= 70.0 {
                theme::WARNING
            } else {
                theme::SAGE
            };
            left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
            left_spans.push(Span::styled(
                format!("ctx: {:.0}% ({:.0}K/{:.0}K)", pct, used as f64 / 1000.0, total as f64 / 1000.0),
                Style::default().fg(color),
            ));
        }
    }
    ```

- [x] 编写 ContextWarning 序列化测试
  - 测试文件: `rust-create-agent/src/agent/events.rs` 底部新增测试
  - 测试场景:
    - `test_context_warning_serde_roundtrip`: 构造 `AgentEvent::ContextWarning { used_tokens: 150000, total_tokens: 200000, percentage: 75.0 }`，serde_json 序列化/反序列化，验证 round-trip 正确
  - 运行命令: `cargo test -p rust-create-agent --lib -- agent::events::tests::test_context_warning`
  - 预期: 通过

**检查步骤:**
- [x] 核心 AgentEvent 包含 ContextWarning
  - `grep -n "ContextWarning" rust-create-agent/src/agent/events.rs`
  - 预期: 找到变体定义
- [x] TUI AgentEvent 包含 TokenUsageUpdate
  - `grep -n "TokenUsageUpdate" rust-agent-tui/src/app/events.rs`
  - 预期: 找到变体定义
- [x] map_executor_event 转发 LlmCallEnd
  - `grep -n "TokenUsageUpdate" rust-agent-tui/src/app/agent.rs`
  - 预期: 找到映射行
- [x] 状态栏包含 ctx 展示
  - `grep -n "context_usage_percent" rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 找到调用
- [x] 编译通过
  - `cargo build 2>&1 | tail -3`
  - 预期: `Finished`

---

### Task 5: Micro-Compact + Auto-Compact 完善

**背景:**
Task 4 已建立 auto-compact 的标记-执行框架，但缺少 micro-compact 的核心实现（纯函数清除旧工具结果）。本 Task 在核心层实现 `micro_compact()` 函数，在 TUI 层实现 `start_micro_compact()` 方法，完成两级压缩策略。micro-compact 在 70%-85% 区间触发（不消耗 API），full compact 在 >=85% 时触发（复用已有 `compact_task()`）。

**涉及文件:**
- 修改: `rust-create-agent/src/agent/token.rs`（新增 micro_compact 函数）
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（新增 start_micro_compact 方法）

**执行步骤:**
- [x] 在 `rust-create-agent/src/agent/token.rs` 中实现 `micro_compact` 纯函数
  - 位置: `token.rs` 文件 impl 块之后、`#[cfg(test)]` 之前
  - 追加公共函数:
    ```rust
    /// 轻量级压缩：清除旧工具结果中的大段内容
    /// 保留最近 `keep_recent` 条消息的工具结果完整内容
    /// 仅清除 cutoff 之前且文本长度 > 500 字符的工具结果
    pub fn micro_compact(messages: &mut [crate::messages::BaseMessage], keep_recent: usize) -> usize {
        let total = messages.len();
        let cutoff = total.saturating_sub(keep_recent);
        let mut cleared = 0;
        for msg in messages.iter_mut().take(cutoff) {
            if let crate::messages::BaseMessage::Tool { content, .. } = msg {
                let text = content.text_content();
                if text.len() > 500 {
                    *content = crate::messages::MessageContent::text("[旧工具结果已清除]");
                    cleared += 1;
                }
            }
        }
        cleared
    }
    ```

- [x] 在 `agent_ops.rs` 中新增 `start_micro_compact` 方法
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` 的 `impl App` 块末尾
  - 追加方法:
    ```rust
    /// 执行 micro-compact：清除旧工具结果，不调用 LLM
    pub fn start_micro_compact(&mut self) {
        use rust_create_agent::agent::token::micro_compact;
        let cleared = micro_compact(&mut self.agent.agent_state_messages, 10);
        if cleared > 0 {
            tracing::info!(cleared, "micro-compact: cleared old tool results");
            let vm = MessageViewModel::system(
                format!("📦 Micro-compact: 清除了 {} 个旧工具结果", cleared)
            );
            self.core.view_messages.push(vm.clone());
            let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
        }
    }
    ```

- [x] 为 micro_compact 编写单元测试
  - 测试文件: `rust-create-agent/src/agent/token.rs` 底部 tests 模块
  - 测试场景:
    - `test_micro_compact_clears_old`: 构造 5 条 Tool 消息（每条填充 600 字符内容）+ 2 条短 Tool 消息（100 字符）+ 3 条 Human/Ai 消息，调用 `micro_compact(&mut messages, 3)`，验证前 5 条长 Tool 被替换为 `[旧工具结果已清除]`，后 2 条短 Tool 和 Human/Ai 不变
    - `test_micro_compact_short_content_untouched`: 构造 Tool 消息内容 < 500 字符，验证不被替换
    - `test_micro_compact_keep_recent`: keep_recent = 2，验证最后 2 条消息（即使为 Tool）不被修改
    - `test_micro_compact_empty`: 传入空切片 `&mut []`，不 panic，返回 0
  - 运行命令: `cargo test -p rust-create-agent --lib -- agent::token::tests::test_micro_compact`
  - 预期: 所有测试通过

**检查步骤:**
- [x] micro_compact 函数存在
  - `grep -n "pub fn micro_compact" rust-create-agent/src/agent/token.rs`
  - 预期: 找到函数定义
- [x] start_micro_compact 方法存在
  - `grep -n "fn start_micro_compact" rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 找到方法定义
- [x] 编译通过
  - `cargo build 2>&1 | tail -3`
  - 预期: `Finished`
- [x] 单元测试通过
  - `cargo test -p rust-create-agent --lib -- agent::token::tests::test_micro_compact 2>&1 | tail -5`
  - 预期: `test result: ok`

---

### Task 6: 功能验收

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`
- 测试数据: 需要至少一个配置好的 LLM Provider（Anthropic 或 OpenAI 兼容）

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -10`
   - 预期: 全部测试通过
   - 失败排查: 逐 crate 定位：`cargo test -p rust-create-agent`、`cargo test -p rust-agent-tui`

2. 核心层 TokenTracker 功能验证
   - `cargo test -p rust-create-agent --lib -- agent::token::tests 2>&1 | grep "test result"`
   - 预期: `test result: ok`，所有 token 追踪测试通过
   - 失败排查: 检查 Task 1 的 TokenTracker/ContextBudget 实现

3. ReactLLM context_window 映射验证
   - `cargo test -p rust-create-agent --lib -- llm::react_adapter::tests 2>&1 | grep "test result"`
   - 预期: 各模型的 context_window 映射正确
   - 失败排查: 检查 Task 3 的映射表

4. TUI 状态栏编译验证
   - `cargo build -p rust-agent-tui 2>&1 | tail -3`
   - 预期: `Finished`，状态栏包含 ctx 百分比渲染逻辑
   - 失败排查: 检查 Task 4 的 status_bar.rs 改动

5. micro_compact 逻辑验证
   - `cargo test -p rust-create-agent --lib -- agent::token::tests::test_micro_compact 2>&1 | grep "test result"`
   - 预期: micro_compact 正确清除旧工具结果
   - 失败排查: 检查 Task 5 的 micro_compact 实现

6. 全量编译验证（跨 crate 依赖）
   - `cargo build 2>&1 | tail -3`
   - 预期: 所有 crate 编译成功，无类型不匹配或缺少方法错误
   - 失败排查: 逐 crate 检查 `cargo build -p <crate>`
