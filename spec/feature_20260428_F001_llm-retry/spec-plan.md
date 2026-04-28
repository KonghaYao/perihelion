# LLM 重试机制 执行计划

**目标:** 在 ReactLLM 层实现 LLM 调用失败自动重试，对 executor 零改动，通过装饰器模式透明包装

**技术栈:** Rust 2021 / tokio 1.x / async-trait / thiserror 2.0 / rand 0.8（新增）

**设计文档:** spec/feature_20260428_F001_llm-retry/spec-design.md

## 改动总览

本次改动涉及 10 个文件（3 个 crate），按分层依赖自底向上推进：先在核心框架层 `rust-create-agent` 添加错误类型和重试基础设施，再修改 LLM 适配层的错误分类，最后在应用层 `rust-agent-tui` 接入事件处理和 UI 显示。Task 1-2 为基础层（无交叉依赖），Task 3 依赖 Task 1，Task 4 依赖 Task 1+2，Task 5 依赖 Task 2+4。事件流经过 Core AgentEvent → `map_executor_event()` → TUI AgentEvent 三层映射。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 Cargo 构建工具可用
  - 运行命令: `cargo build -p rust-create-agent 2>&1 | tail -5`
- [x] 验证测试工具可用
  - 运行命令: `cargo test -p rust-create-agent --lib 2>&1 | tail -10`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-create-agent 2>&1 | grep -E "error|Finished"`
  - 预期: 输出包含 "Finished"，无 "error"
- [x] 测试命令可用
  - `cargo test -p rust-create-agent --lib 2>&1 | grep -E "test result|error"`
  - 预期: 输出包含 "test result: ok"，无编译错误

---

### Task 1: 错误类型改造

**背景:**
当前 `AgentError::LlmError(String)` 丢失了 HTTP 状态码，无法区分 429 限流（可重试）和 401 认证失败（不可重试）。新增 `LlmHttpError` 变体携带 status code，配合 `is_retryable()` 方法为 Task 4 的重试逻辑提供判断基础。经代码分析确认：代码库中没有任何对 `AgentError::LlmError` 的 pattern match 和 `_` 通配符，新增变体不会破坏编译。

**涉及文件:**
- 修改: `rust-create-agent/src/error.rs`
- 修改: `rust-create-agent/Cargo.toml`

**执行步骤:**
- [x] 在 `Cargo.toml` 添加 `rand` 依赖（重试抖动计算需要随机数）
  - 位置: `rust-create-agent/Cargo.toml` ~L33（`parking_lot` 行之后）
  - 添加: `rand = "0.8"`
- [x] 在 `AgentError` enum 新增 `LlmHttpError` 变体
  - 位置: `rust-create-agent/src/error.rs` ~L15（`LlmError` 变体之后）
  - 添加:
    ```rust
    #[error("LLM HTTP 错误 ({status}): {message}")]
    LlmHttpError { status: u16, message: String },
    ```
- [x] 在 `AgentError` impl 块新增 `is_retryable()` 方法
  - 位置: `error.rs` 文件末尾（`pub type AgentResult<T>` 之前）
  - 添加 impl 块:
    ```rust
    impl AgentError {
        /// 判断错误是否可重试（用于 LLM 调用重试机制）
        pub fn is_retryable(&self) -> bool {
            match self {
                Self::LlmHttpError { status, .. } => {
                    matches!(status, 408 | 429 | 500..=599)
                }
                Self::LlmError(msg) => {
                    let msg_lower = msg.to_lowercase();
                    msg_lower.contains("connection")
                        || msg_lower.contains("timeout")
                        || msg_lower.contains("dns")
                }
                _ => false,
            }
        }
    }
    ```
- [x] 为 `is_retryable()` 编写单元测试
  - 测试文件: `rust-create-agent/src/error.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `LlmHttpError { status: 429, .. }` → `is_retryable() == true`
    - `LlmHttpError { status: 503, .. }` → `is_retryable() == true`
    - `LlmHttpError { status: 408, .. }` → `is_retryable() == true`
    - `LlmHttpError { status: 400, .. }` → `is_retryable() == false`
    - `LlmHttpError { status: 401, .. }` → `is_retryable() == false`
    - `LlmHttpError { status: 404, .. }` → `is_retryable() == false`
    - `LlmError("connection refused")` → `is_retryable() == true`
    - `LlmError("reqwest timeout exceeded")` → `is_retryable() == true`
    - `LlmError("parse error")` → `is_retryable() == false`
    - `ToolNotFound("x")` → `is_retryable() == false`
  - 运行命令: `cargo test -p rust-create-agent --lib -- error::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 编译无错误
  - `cargo build -p rust-create-agent 2>&1 | grep -E "error|Finished"`
  - 预期: 输出包含 "Finished"，无 "error"
- [x] 测试全部通过
  - `cargo test -p rust-create-agent --lib -- error::tests 2>&1 | grep "test result"`
  - 预期: "test result: ok"

---

### Task 2: 事件扩展

**背景:**
重试过程需要通知 TUI 层显示状态。事件流经过 Core `AgentEvent` → `map_executor_event()` → TUI `AgentEvent` → `handle_agent_event()` 四个环节，需要在每个环节都添加 `LlmRetrying` 支持。经代码分析：Core 事件在 `rust-create-agent/src/agent/events.rs`，TUI 事件在 `rust-agent-tui/src/app/events.rs`，映射在 `agent.rs:234` 的 `map_executor_event()` 函数。

**涉及文件:**
- 修改: `rust-create-agent/src/agent/events.rs`（Core 事件）
- 修改: `rust-agent-tui/src/app/events.rs`（TUI 事件）
- 修改: `rust-agent-tui/src/app/agent.rs`（映射函数）

**执行步骤:**
- [x] 在 Core `AgentEvent` 新增 `LlmRetrying` 变体
  - 位置: `rust-create-agent/src/agent/events.rs` ~L37（`ContextWarning` 变体之后，`}` 闭合括号之前）
  - 添加:
    ```rust
    /// LLM 调用重试中
    LlmRetrying {
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
        error: String,
    },
    ```
- [x] 在 TUI `AgentEvent` 新增 `LlmRetrying` 变体
  - 位置: `rust-agent-tui/src/app/events.rs` ~L49（`TokenUsageUpdate` 变体之后，`}` 闭合括号之前）
  - 添加:
    ```rust
    /// LLM 调用重试中（从核心层 LlmRetrying 映射而来）
    LlmRetrying {
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
        error: String,
    },
    ```
- [x] 在 `map_executor_event()` 添加映射分支
  - 位置: `rust-agent-tui/src/app/agent.rs` ~L284（`ContextWarning` 分支之前）
  - 添加:
    ```rust
    ExecutorEvent::LlmRetrying { attempt, max_attempts, delay_ms, error } => {
        AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error }
    }
    ```
- [x] 验证 `LlmRetrying` 事件序列化
  - 测试文件: `rust-create-agent/src/agent/events.rs` 底部 `#[cfg(test)] mod tests`
  - 在 `test_context_warning_serde_roundtrip` 之后新增测试:
    ```rust
    #[test]
    fn test_llm_retrying_serde_roundtrip() {
        let ev = AgentEvent::LlmRetrying {
            attempt: 2,
            max_attempts: 5,
            delay_ms: 2000,
            error: "API 错误 503: Service Unavailable".to_string(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
        if let AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error } = deserialized {
            assert_eq!(attempt, 2);
            assert_eq!(max_attempts, 5);
            assert_eq!(delay_ms, 2000);
            assert_eq!(error, "API 错误 503: Service Unavailable");
        } else {
            panic!("Deserialized to wrong variant");
        }
    }
    ```
  - 运行命令: `cargo test -p rust-create-agent --lib -- events::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 全 workspace 编译通过
  - `cargo build 2>&1 | grep -E "error|Finished"`
  - 预期: 输出包含 "Finished"，无 "error"
- [x] 核心事件测试通过
  - `cargo test -p rust-create-agent --lib -- events::tests 2>&1 | grep "test result"`
  - 预期: "test result: ok"

---

### Task 3: LLM 实现层 HTTP 错误分类

**背景:**
OpenAI 和 Anthropic 的 `invoke()` 方法中，HTTP 非 2xx 响应统一用 `LlmError` 包装，丢失了 status code。改为 `LlmHttpError` 后，Task 4 的 `is_retryable()` 才能根据 status code 精确判断。经代码分析确认两处改动点位置完全对称。

**涉及文件:**
- 修改: `rust-create-agent/src/llm/openai.rs`
- 修改: `rust-create-agent/src/llm/anthropic.rs`

**执行步骤:**
- [x] OpenAI: 将 API 错误从 `LlmError` 改为 `LlmHttpError`
  - 位置: `rust-create-agent/src/llm/openai.rs` ~L365
  - 将 `return Err(AgentError::LlmError(format!("API 错误 {status}: {msg}")));` 替换为:
    ```rust
    return Err(AgentError::LlmHttpError {
        status: status.as_u16(),
        message: format!("API 错误 {status}: {msg}"),
    });
    ```
  - 原因: 保留 status code 用于重试判断
- [x] Anthropic: 将 API 错误从 `LlmError` 改为 `LlmHttpError`
  - 位置: `rust-create-agent/src/llm/anthropic.rs` ~L464
  - 将 `return Err(AgentError::LlmError(format!("API 错误 {status}: {msg}")));` 替换为:
    ```rust
    return Err(AgentError::LlmHttpError {
        status: status.as_u16(),
        message: format!("API 错误 {status}: {msg}"),
    });
    ```
  - 原因: 与 OpenAI 保持一致的错误分类
- [x] 确认编译通过（无新增测试，改动仅影响错误变体）
  - 运行命令: `cargo build 2>&1 | grep -E "error|Finished"`
  - 预期: 编译成功，无错误。网络错误（L308-316 OpenAI / L417 Anthropic）和响应解析错误（L344 OpenAI / L445 Anthropic）保持 `LlmError` 不变，不受影响。

**检查步骤:**
- [x] Grep 确认改动正确
  - `grep -n "LlmHttpError" rust-create-agent/src/llm/openai.rs rust-create-agent/src/llm/anthropic.rs`
  - 预期: 各有 1 处 `LlmHttpError`，且所在行包含 `status.as_u16()`
- [x] 全 workspace 编译通过
  - `cargo build 2>&1 | grep -E "error|Finished"`
  - 预期: "Finished"

---

### Task 4: 重试核心机制

**背景:**
这是整个功能的核心——`RetryableLLM<L>` 装饰器包装任意 `ReactLLM` 实现，在 `generate_reasoning` 失败时根据 `is_retryable()` 决定是否重试，采用指数退避 + 25% 随机抖动策略。对 executor 代码零改动，仅在组装点将原始 LLM 包装即可。经代码分析确认组装点在 `rust-agent-tui/src/app/agent.rs:170`（主 agent）和 `:147`（SubAgent llm_factory），两处都需要包装。

**涉及文件:**
- 新建: `rust-create-agent/src/llm/retry.rs`
- 修改: `rust-create-agent/src/llm/mod.rs`（导出 retry 模块）
- 修改: `rust-agent-tui/src/app/agent.rs`（组装点包装）

**执行步骤:**
- [x] 创建 `retry.rs` 文件，实现 `RetryConfig` 和 `RetryableLLM<L>`
  - 位置: `rust-create-agent/src/llm/retry.rs`（新建）
  - 内容:
    ```rust
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use rand::Rng;

    use crate::agent::events::{AgentEvent, AgentEventHandler};
    use crate::agent::react::{ReactLLM, Reasoning};
    use crate::error::AgentResult;
    use crate::messages::BaseMessage;
    use crate::tools::BaseTool;

    /// 重试配置
    #[derive(Debug, Clone)]
    pub struct RetryConfig {
        pub max_retries: usize,
        pub base_delay_ms: u64,
        pub max_delay_ms: u64,
    }

    impl Default for RetryConfig {
        fn default() -> Self {
            Self {
                max_retries: 5,
                base_delay_ms: 500,
                max_delay_ms: 32_000,
            }
        }
    }

    impl RetryConfig {
        pub fn with_max_retries(mut self, n: usize) -> Self { self.max_retries = n; self }
        pub fn with_base_delay_ms(mut self, ms: u64) -> Self { self.base_delay_ms = ms; self }
        pub fn with_max_delay_ms(mut self, ms: u64) -> Self { self.max_delay_ms = ms; self }

        /// 指数退避 + 25% 随机抖动
        pub fn exponential_delay(&self, attempt: usize) -> u64 {
            let base = (self.base_delay_ms as f64 * 2f64.powi(attempt as i32))
                .min(self.max_delay_ms as f64);
            let mut rng = rand::thread_rng();
            let jitter = rng.gen_range(0.0..0.25) * base;
            (base + jitter) as u64
        }
    }

    /// ReactLLM 装饰器：在调用失败时自动重试
    pub struct RetryableLLM<L: ReactLLM> {
        inner: L,
        config: RetryConfig,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
    }

    impl<L: ReactLLM> RetryableLLM<L> {
        pub fn new(inner: L, config: RetryConfig) -> Self {
            Self { inner, config, event_handler: None }
        }

        pub fn with_event_handler(mut self, handler: Arc<dyn AgentEventHandler>) -> Self {
            self.event_handler = Some(handler);
            self
        }

        fn emit(&self, event: AgentEvent) {
            if let Some(h) = &self.event_handler {
                h.on_event(event);
            }
        }
    }

    #[async_trait]
    impl<L: ReactLLM> ReactLLM for RetryableLLM<L> {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            tools: &[&dyn BaseTool],
        ) -> AgentResult<Reasoning> {
            let mut last_error = None;
            for attempt in 0..=self.config.max_retries {
                match self.inner.generate_reasoning(messages, tools).await {
                    Ok(r) => return Ok(r),
                    Err(e) if e.is_retryable() && attempt < self.config.max_retries => {
                        let delay = self.config.exponential_delay(attempt);
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            delay_ms = delay,
                            error = %e,
                            "LLM 调用失败，准备重试"
                        );
                        self.emit(AgentEvent::LlmRetrying {
                            attempt: attempt + 1,
                            max_attempts: self.config.max_retries,
                            delay_ms: delay,
                            error: e.to_string(),
                        });
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        last_error = Some(e);
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(last_error.unwrap())
        }

        fn model_name(&self) -> String {
            self.inner.model_name()
        }

        fn context_window(&self) -> u32 {
            self.inner.context_window()
        }
    }
    ```
- [x] 在 `llm/mod.rs` 导出 retry 模块
  - 位置: `rust-create-agent/src/llm/mod.rs` ~L5（`mod react_adapter;` 之后）
  - 添加: `pub mod retry;`
  - 在 pub use 区域（~L24）添加: `pub use retry::{RetryConfig, RetryableLLM};`
- [x] 调整 `agent.rs` 中 handler 和 model 的创建顺序（关键！）
  - 经代码分析确认：`handler`（L111）在 `model`（L70）之后创建，但 `RetryableLLM` 需要 handler 来发射 `LlmRetrying` 事件。handler 的所有依赖（`tx`、`cwd`、`langfuse_tracer`、`provider_name`）在 L70 之前都已可用。
  - 位置: `rust-agent-tui/src/app/agent.rs` L106-134（handler 创建块）
  - 将 L106-134 的 handler 创建代码块移动到 L70 之前（`let model = ...` 之前）
  - 原因: handler 需要在 model 之前创建，以便传给 RetryableLLM
- [x] 在主 agent 组装点包装 `RetryableLLM`
  - 位置: `rust-agent-tui/src/app/agent.rs` 原 L70（handler 移动后的位置）
  - 将 `let model = BaseModelReactLLM::new(provider.into_model());` 替换为:
    ```rust
    let model = rust_create_agent::llm::RetryableLLM::new(
        BaseModelReactLLM::new(provider.into_model()),
        rust_create_agent::llm::RetryConfig::default(),
    ).with_event_handler(Arc::clone(&handler));
    ```
  - 原因: 主 agent LLM 调用需要重试保护，且需要通过 handler 发射重试事件
- [x] 在 SubAgent llm_factory 中包装 `RetryableLLM`（不传 handler）
  - 位置: `rust-agent-tui/src/app/agent.rs` llm_factory 闭包内
  - 将两个 `Box::new(BaseModelReactLLM::new(...))` 替换为:
    ```rust
    Box::new(rust_create_agent::llm::RetryableLLM::new(
        BaseModelReactLLM::new(p.into_model()),
        rust_create_agent::llm::RetryConfig::default(),
    ))
    ```
    和
    ```rust
    Box::new(rust_create_agent::llm::RetryableLLM::new(
        BaseModelReactLLM::new(provider_clone.clone().into_model()),
        rust_create_agent::llm::RetryConfig::default(),
    ))
    ```
  - 注意: SubAgent 的 RetryableLLM 不传 handler，重试时静默重试（SubAgent 执行结果由父 agent 汇总）
- [x] 编写重试逻辑单元测试
  - 测试文件: `rust-create-agent/src/llm/retry.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - **全部重试成功**: 前两次返回 `LlmHttpError { status: 503 }`，第三次返回 `Ok(reasoning)` → 最终返回 Ok
    - **不可重试错误立即返回**: 第一次返回 `LlmHttpError { status: 400 }` → 立即返回 Err，不重试
    - **重试耗尽**: 所有次都返回 `LlmHttpError { status: 429 }` → 返回最后一次错误
    - **网络错误可重试**: 返回 `LlmError("connection refused")` → 触发重试
    - **退避延迟范围**: 验证 `RetryConfig::exponential_delay()` 返回值在 `base*2^attempt` 到 `base*2^attempt * 1.25` 范围内
  - 实现: 使用 `AtomicUsize` + 脚本模式模拟失败序列（与现有 `MockLLM` 模式一致，但返回 `Err`）
  - 运行命令: `cargo test -p rust-create-agent --lib -- llm::retry::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] retry.rs 文件存在且导出正确
  - `grep -n "pub mod retry\|pub use retry" rust-create-agent/src/llm/mod.rs`
  - 预期: 两行输出，分别包含 `pub mod retry` 和 `pub use retry`
- [x] 组装点已包装
  - `grep -n "RetryableLLM" rust-agent-tui/src/app/agent.rs`
  - 预期: 至少 3 处（主 agent + llm_factory 两处）
- [x] 全 workspace 编译通过
  - `cargo build 2>&1 | grep -E "error|Finished"`
  - 预期: "Finished"
- [x] 重试测试通过
  - `cargo test -p rust-create-agent --lib -- llm::retry::tests 2>&1 | grep "test result"`
  - 预期: "test result: ok"

---

### Task 5: TUI 集成

**背景:**
重试事件需要在 TUI 界面可见。TUI 的 `handle_agent_event()` 使用 exhaustive match，需在 match 中添加 `LlmRetrying` 分支。重试状态存入 App 子结构体 `AgentComm` 的 `retry_status` 字段，在 status_bar 第一行渲染显示。收到下一个 `ToolCall`/`AssistantChunk`/`Done` 事件时清除重试状态。经代码分析确认 `AgentComm` 定义在 `rust-agent-tui/src/app/agent_comm.rs`。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_comm.rs`（新增 `retry_status` 字段）
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（处理 `LlmRetrying` 事件）
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`（渲染重试状态）
- 修改: `rust-agent-tui/src/ui/headless.rs`（测试适配）

**执行步骤:**
- [x] 新增 `RetryStatus` 结构体和 `AgentComm.retry_status` 字段
  - 位置: `rust-agent-tui/src/app/agent_comm.rs` ~L1（use 语句之后）
  - 添加:
    ```rust
    /// LLM 重试状态（由 AgentEvent::LlmRetrying 更新）
    pub struct RetryStatus {
        pub attempt: usize,
        pub max_attempts: usize,
        pub delay_ms: u64,
    }
    ```
  - 位置: `rust-agent-tui/src/app/agent_comm.rs` `AgentComm` struct 内（`needs_auto_compact` 字段之后，~L39）
  - 添加字段: `pub retry_status: Option<RetryStatus>,`
  - 位置: `AgentComm::new()` 函数内（初始化列表末尾）
  - 添加: `retry_status: None,`
- [x] 在 `handle_agent_event()` 添加 `LlmRetrying` 分支
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` ~L165（match 花括号内，`SubAgentStart` 之前）
  - 添加:
    ```rust
    AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error: _ } => {
        self.agent.retry_status = Some(RetryStatus { attempt, max_attempts, delay_ms });
        (true, false, false)
    }
    ```
  - 同时需在文件顶部添加 use: `use super::agent_comm::RetryStatus;`
- [x] 在收到其他事件时清除 `retry_status`
  - 位置: `agent_ops.rs` 中 `AgentEvent::ToolCall` 分支（~L218）和 `AgentEvent::AssistantChunk` 分支（~L320）和 `AgentEvent::Done` 分支（~L374）
  - 在每个分支处理逻辑开头添加: `self.agent.retry_status = None;`
  - 原因: 重试结束后清除状态，避免状态栏残留
- [x] 在 status_bar 第一行渲染重试状态
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs` `render_first_row` 函数（~L88，上下文使用率之后，`render_truncated_line` 调用之前）
  - 添加:
    ```rust
    // 重试状态
    if let Some(ref retry) = app.agent.retry_status {
        let delay_sec = retry.delay_ms as f64 / 1000.0;
        spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        spans.push(Span::styled(
            format!(" ⟳ 重试 {}/{} ({:.1}s)", retry.attempt, retry.max_attempts, delay_sec),
            Style::default().fg(theme::WARNING),
        ));
    }
    ```
- [x] 在 headless 测试中添加 `LlmRetrying` 事件测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`
  - 在现有测试之后新增:
    ```rust
    #[tokio::test]
    async fn test_retry_status_shows_in_status_bar() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let notify = Arc::clone(&handle.render_notify);
        let n1 = notify.notified();
        let n2 = notify.notified();

        app.push_agent_event(AgentEvent::LlmRetrying {
            attempt: 2,
            max_attempts: 5,
            delay_ms: 2000,
            error: "503".to_string(),
        });
        app.process_pending_events();
        tokio::join!(n1, n2);

        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        assert!(handle.contains("2/5"), "状态栏应显示重试次数 2/5");
    }
    ```
  - 运行命令: `cargo test -p rust-agent-tui --lib -- headless::tests::test_retry_status`
  - 预期: 测试通过

**检查步骤:**
- [x] `RetryStatus` 结构体和字段已添加
  - `grep -n "RetryStatus\|retry_status" rust-agent-tui/src/app/agent_comm.rs`
  - 预期: 结构体定义 + 字段声明 + 初始化各 1 处
- [x] `handle_agent_event` 包含 `LlmRetrying` 分支
  - `grep -n "LlmRetrying" rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 1 处 match 分支
- [x] status_bar 包含重试渲染逻辑
  - `grep -n "retry_status" rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 1 处渲染逻辑
- [x] 全 workspace 编译通过
  - `cargo build 2>&1 | grep -E "error|Finished"`
  - 预期: "Finished"
- [x] headless 测试通过
  - `cargo test -p rust-agent-tui --lib -- headless::tests::test_retry_status 2>&1 | grep "test result"`
  - 预期: "test result: ok"

---

### Task 6: LLM 重试机制 验收

**前置条件:**
- 启动命令: `cargo build --workspace`
- 测试数据准备: 所有前序 Task 已完成

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test --workspace 2>&1 | tail -20`
   - 预期: 全部测试通过，无失败
   - 结果: ✅ 全部通过（551+ tests, 0 failures）

2. 验证错误分类正确性
   - `cargo test -p rust-create-agent --lib -- error::tests 2>&1 | grep "test result"`
   - 预期: 所有 `is_retryable()` 测试通过（429/5xx 返回 true，400/401/404 返回 false）
   - 结果: ✅ 10 passed

3. 验证重试核心逻辑
   - `cargo test -p rust-create-agent --lib -- llm::retry::tests 2>&1 | grep "test result"`
   - 预期: 所有重试测试通过（可重试错误触发重试、不可重试立即返回、耗尽返回最后错误）
   - 结果: ✅ 5 passed

4. 验证事件序列化
   - `cargo test -p rust-create-agent --lib -- events::tests 2>&1 | grep "test result"`
   - 预期: `LlmRetrying` 事件序列化/反序列化正确
   - 结果: ✅ 2 passed

5. 验证 TUI 重试状态显示
   - `cargo test -p rust-agent-tui --lib -- headless::tests::test_retry_status 2>&1 | grep "test result"`
   - 预期: 状态栏正确显示重试次数和延迟
   - 结果: ✅ 1 passed

6. 验证 executor 代码零改动
   - `git diff rust-create-agent/src/agent/executor.rs`
   - 预期: 无任何改动
   - 结果: ✅ 无改动
