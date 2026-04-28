# Feature: 20260428_F001 - llm-retry

## 需求背景

当前 LLM 调用失败时（网络超时、429 限流、5xx 服务器错误），executor 直接终止 agent，用户需要手动重试整个对话。但实际上大部分 LLM 错误是暂时性的，自动重试即可恢复。

参考 Claude Code 的 `withRetry()` 机制（指数退避 + 随机抖动 + 最大重试次数），需要在 perihelion 的 ReactLLM 层实现类似的重试能力。

## 目标

- LLM 调用遇到暂时性错误（429/5xx/网络超时）时自动重试，不中断 agent 执行
- 精确区分可重试错误和不可重试错误（4xx 客户端错误）
- 重试过程对 executor 零改动，通过装饰器模式透明包装
- TUI 在 loading 信息区域显示重试状态

## 方案设计

### 整体架构

在 `ReactLLM` trait 层引入泛型装饰器 `RetryableLLM<L>`，包装任意 `ReactLLM` 实现，在 `generate_reasoning` 调用失败时根据错误类型决定是否重试。对 executor 和上层代码完全透明。

```
┌──────────────────────────────────┐
│  RetryableLLM<Inner: ReactLLM>   │
│  ┌────────────────────────────┐  │
│  │  generate_reasoning()     │  │
│  │   loop max_retries:       │  │
│  │     result = inner.generate│  │
│  │     if ok → return         │  │
│  │     if retryable → backoff │  │
│  │     else → return err      │  │
│  └────────────────────────────┘  │
└──────────────────────────────────┘
         │ wraps
    ┌────┴────┐
    │ ChatOpenAI / ChatAnthropic   │
    │ (直接 impl ReactLLM)         │
    └─────────────────────────────┘
```

组装方式：

```rust
let llm = ChatOpenAI::from_env().unwrap();
let retry_llm = RetryableLLM::new(llm, RetryConfig::default());

ReActAgent::new(retry_llm)
    .max_iterations(50)
    // ...
```

### 错误类型改造

当前 `AgentError::LlmError(String)` 统一承载所有 LLM 错误，HTTP 状态码信息被吞掉，无法精确判断是否可重试。

**新增变体：**

```rust
// error.rs
#[derive(Error, Debug)]
pub enum AgentError {
    // ... 保留已有变体

    #[error("LLM HTTP 错误 ({status}): {message}")]
    LlmHttpError { status: u16, message: String },
}
```

- `LlmHttpError`：专用于 HTTP 层面的 API 错误（4xx/5xx），携带 status code
- `LlmError(String)`：继续用于网络层错误（连接失败、DNS、超时等 reqwest 错误）

**新增 `is_retryable()` 方法：**

```rust
impl AgentError {
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

可重试的错误类型：

| 错误 | 是否重试 | 分类 |
|------|---------|------|
| 408 Request Timeout | 是 | LlmHttpError |
| 429 Too Many Requests | 是 | LlmHttpError |
| 500/502/503/504/529 | 是 | LlmHttpError |
| 网络连接失败/超时/DNS | 是 | LlmError |
| 400 Bad Request | 否 | LlmHttpError |
| 401 Unauthorized | 否 | LlmHttpError |
| 403 Forbidden | 否 | LlmHttpError |
| 404 Not Found | 否 | LlmHttpError |

### LLM 实现层改造

`openai.rs` 和 `anthropic.rs` 中，将 HTTP 错误改用 `LlmHttpError`：

```rust
// 之前
return Err(AgentError::LlmError(format!("API 错误 {status}: {msg}")));

// 之后
return Err(AgentError::LlmHttpError {
    status: status.as_u16(),
    message: format!("API 错误 {status}: {msg}"),
});
```

网络错误（reqwest 失败）保持 `LlmError` 不变。

### 重试配置

新增 `rust-create-agent/src/llm/retry.rs` 文件。

```rust
pub struct RetryConfig {
    pub max_retries: usize,     // 默认 5
    pub base_delay_ms: u64,     // 默认 500ms
    pub max_delay_ms: u64,      // 默认 32000ms
}

impl RetryConfig {
    pub fn default() -> Self { ... }

    pub fn with_max_retries(mut self, n: usize) -> Self { ... }
    pub fn with_base_delay_ms(mut self, ms: u64) -> Self { ... }
    pub fn with_max_delay_ms(mut self, ms: u64) -> Self { ... }

    /// 指数退避 + 25% 随机抖动
    pub fn exponential_delay(&self, attempt: usize) -> u64 { ... }
}
```

退避序列（默认配置）：

| 重试次数 | 基础延迟 | 抖动范围 | 实际范围 |
|---------|---------|---------|---------|
| 1 | 500ms | ±125ms | 375-625ms |
| 2 | 1s | ±250ms | 0.75-1.25s |
| 3 | 2s | ±500ms | 1.5-2.5s |
| 4 | 4s | ±1s | 3-5s |
| 5 | 8s | ±2s | 6-10s |
| 6+ | 16s | ±4s | 12-20s |
| 7+ | 32s（封顶） | ±8s | 24-40s |

### RetryableLLM 包装器

```rust
pub struct RetryableLLM<L: ReactLLM> {
    inner: L,
    config: RetryConfig,
    event_handler: Option<Arc<dyn AgentEventHandler>>,
}
```

`generate_reasoning` 核心逻辑：

```
for attempt in 0..=config.max_retries:
    result = inner.generate_reasoning(messages, tools)
    if OK → return Ok(result)
    if error.is_retryable() && attempt < config.max_retries:
        delay = config.exponential_delay(attempt)
        log.warn("LLM 调用失败，准备重试", attempt, delay)
        emit(LlmRetrying { attempt+1, max_retries, delay_ms, error })
        sleep(delay)
        continue
    else:
        return Err(error)  // 不可重试或已耗尽次数
return Err(last_error)
```

`model_name()` 和 `context_window()` 直接委托给 inner。

Builder 方法：

```rust
impl<L: ReactLLM> RetryableLLM<L> {
    pub fn new(inner: L, config: RetryConfig) -> Self { ... }
    pub fn with_event_handler(mut self, handler: Arc<dyn AgentEventHandler>) -> Self { ... }
}
```

### 事件扩展

`events.rs` 新增 `LlmRetrying` 变体：

```rust
/// LLM 调用重试中
LlmRetrying {
    attempt: usize,        // 当前重试次数（从 1 开始）
    max_attempts: usize,   // 最大重试次数
    delay_ms: u64,         // 本次退避延迟（毫秒）
    error: String,         // 触发重试的错误信息
},
```

### TUI 集成

**事件处理**（`agent_ops.rs`）：

`poll_agent()` 收到 `LlmRetrying` 后，将重试状态存入 App 状态：

```rust
AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error } => {
    app.retry_status = Some(RetryStatus { attempt, max_attempts, delay_ms });
}
```

重试结束后（收到下一个 `LlmCallStart` 或 `LlmCallEnd`），清除 `retry_status`。

**Loading 区域显示**（`message_render.rs` 或 `status_bar.rs`）：

在现有 loading 信息区域显示重试状态：

```
LLM 重试 2/5 (2.1s)...  API 错误 503: Service Unavailable
```

样式使用 `Color::Yellow`（警告色），与现有工具警告保持一致。

### 改动文件清单

| 文件 | 改动类型 | 说明 |
|------|---------|------|
| `rust-create-agent/src/error.rs` | 修改 | 新增 `LlmHttpError` 变体 + `is_retryable()` |
| `rust-create-agent/src/agent/events.rs` | 修改 | 新增 `LlmRetrying` 事件 |
| `rust-create-agent/src/llm/mod.rs` | 修改 | 导出 `retry` 模块 |
| `rust-create-agent/src/llm/retry.rs` | **新建** | `RetryConfig` + `RetryableLLM<L>` |
| `rust-create-agent/src/llm/openai.rs` | 修改 | HTTP 错误改用 `LlmHttpError` |
| `rust-create-agent/src/llm/anthropic.rs` | 修改 | HTTP 错误改用 `LlmHttpError` |
| `rust-agent-tui/src/app/agent_ops.rs` | 修改 | 处理 `LlmRetrying` 事件 |
| `rust-agent-tui/src/app/mod.rs` | 修改 | 新增 `RetryStatus` 字段 |
| `rust-agent-tui/src/ui/main_ui/status_bar.rs` | 修改 | 重试状态显示 |
| `rust-agent-tui/src/ui/headless.rs` | 修改 | 测试适配新事件 |

## 实现要点

- **依赖 `rand` crate**：退避抖动需要随机数生成。如果不想引入新依赖，可用 `std::time::SystemTime::now()` 的纳秒部分做简易抖动。推荐用 `rand` 保持代码清晰。
- **`LlmHttpError` 向后兼容**：现有代码匹配 `AgentError::LlmError` 的地方不受影响（新增变体不破坏已有 match）。但如果有 `_` 通配分支，编译器会提醒补全新变体。
- **重试不回滚 state**：由于 `generate_reasoning` 是只读操作（只读取 messages，不写入 state），重试不需要回滚任何状态，天然安全。
- **Cancel 与重试的交互**：重试 sleep 期间应尊重 CancellationToken。`RetryableLLM` 不持有 cancel token，但如果未来需要支持取消重试，可以在 `RetryConfig` 中加 `cancel: Option<CancellationToken>` 字段。当前阶段先不处理，因为 cancel 已在 executor 的 `tokio::select!` 中处理，LLM 层重试完成后 executor 才会检查 cancel。

## 约束一致性

- **Workspace 分层**：`retry.rs` 放在 `rust-create-agent`（核心框架层），符合"基础设施放在下层"的约束。
- **异步优先**：`RetryableLLM` 的 `generate_reasoning` 是 async 函数，sleep 使用 `tokio::time::sleep`，符合异步优先约束。
- **Middleware Chain 不受影响**：重试在 ReactLLM 层，不涉及 Middleware，不违反 Middleware Chain 模式。
- **事件驱动通信**：通过 `AgentEvent::LlmRetrying` 事件通知 TUI，符合事件驱动 TUI 通信约束。
- **新增依赖**：`rand` crate（如采用），需确认是否在 Workspace 依赖白名单中。替代方案是用 `std::time` 做简易抖动，避免引入新依赖。

## 验收标准

- [ ] LLM 调用遇到 429/5xx/网络错误时自动重试，最多 5 次
- [ ] 400/401/403/404 等客户端错误不触发重试，直接返回
- [ ] 退避延迟符合指数退避 + 抖动的预期序列
- [ ] TUI loading 区域显示重试状态（次数/总次数/延迟）
- [ ] 重试成功后 agent 继续正常执行，不丢失上下文
- [ ] 重试全部失败后返回最后一次的错误信息
- [ ] `LlmRetrying` 事件正确序列化/反序列化
- [ ] 不影响现有 `LlmCallStart`/`LlmCallEnd` 事件对
- [ ] executor 代码零改动
- [ ] 单元测试覆盖：MockLLM 前几次返回错误、最后一次成功；不可重试错误立即返回
