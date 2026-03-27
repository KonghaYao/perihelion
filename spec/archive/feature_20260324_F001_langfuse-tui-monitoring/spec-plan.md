# Langfuse TUI 监控接入 执行计划

**目标:** 在 TUI 层接入 Langfuse，对每轮对话（Trace）、每次 LLM 生成（Generation）、每次工具调用（Span）全链路追踪

**技术栈:** Rust, langfuse-ergonomic 0.6.3, parking_lot::Mutex, tokio

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: `rust-create-agent` LLM 层 usage 扩展

**涉及文件:**
- 修改: `rust-create-agent/src/llm/types.rs`
- 修改: `rust-create-agent/src/llm/anthropic.rs`
- 修改: `rust-create-agent/src/llm/openai.rs`
- 修改: `rust-create-agent/src/llm/react_adapter.rs`
- 修改: `rust-create-agent/src/agent/react.rs`

**执行步骤:**

- [x] 在 `llm/types.rs` 中新增 `TokenUsage` 结构体，并为 `LlmResponse` 增加 `usage: Option<TokenUsage>` 字段
  - `TokenUsage { input_tokens: u32, output_tokens: u32 }`，需要 `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]`
  - `LlmResponse` 的 `usage` 初始化为 `None`（兼容现有构造处）

- [x] 在 `llm/anthropic.rs` 的 `invoke` 方法中解析 usage：
  - Anthropic 响应：`resp_json["usage"]["input_tokens"]` 和 `resp_json["usage"]["output_tokens"]`
  - 在 `Ok(LlmResponse { message, stop_reason })` 前解析，改为 `Ok(LlmResponse { message, stop_reason, usage })`

- [x] 在 `llm/openai.rs` 的 `invoke` 方法中解析 usage：
  - OpenAI 响应：`resp_json["usage"]["prompt_tokens"]` 和 `resp_json["usage"]["completion_tokens"]`
  - 同上，填入 `LlmResponse.usage`

- [x] 在 `agent/react.rs` 的 `Reasoning` 结构体中增加两个字段：
  - `pub usage: Option<crate::llm::types::TokenUsage>` 初始化为 `None`
  - `pub model: String` 初始化为空字符串（model name 来自 LLM 实现）
  - `with_tools` 和 `with_answer` 构造方法保持向后兼容（新字段默认值）

- [x] 在 `llm/react_adapter.rs`（`BaseModelReactLLM::generate_reasoning`）中，将 `response.usage` 传入 `Reasoning.usage`，将 `self.model.model_id()` 传入 `Reasoning.model`

**检查步骤:**

- [x] 单元测试通过（不涉及真实 API，MockLLM 的 Reasoning 默认 usage=None 不破坏现有测试）
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok` 且无编译错误

- [x] 新结构体编译正确
  - `cargo build -p rust-create-agent 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出（无编译错误）

---

### Task 2: `rust-create-agent` AgentEvent Hook + Executor emit

**涉及文件:**
- 修改: `rust-create-agent/src/agent/events.rs`
- 修改: `rust-create-agent/src/agent/react.rs`
- 修改: `rust-create-agent/src/agent/executor.rs`

**执行步骤:**

- [x] 在 `agent/events.rs` 中为 `AgentEvent` 枚举新增两个变体：
  ```rust
  LlmCallStart {
      step: usize,
      messages: Vec<crate::messages::BaseMessage>,
  },
  LlmCallEnd {
      step: usize,
      model: String,
      output: String,
      usage: Option<crate::llm::types::TokenUsage>,
  },
  ```
  注意：`AgentEvent` 已有 `#[derive(serde::Serialize, serde::Deserialize)]`，`BaseMessage` 也已实现这两个 trait，直接兼容

- [x] 在 `agent/react.rs` 的 `ReactLLM` trait 中增加 `model_name` 方法（带默认实现）：
  ```rust
  fn model_name(&self) -> String {
      "unknown".to_string()
  }
  ```
  同时为 `Box<dyn ReactLLM + Send + Sync>` 的 blanket impl 也转发此方法

- [x] 在 `agent/executor.rs` 的 ReAct 循环中，在 `generate_reasoning` 调用前后各 emit 一个事件：
  - 调用前（在 `tokio::select!` 块开始之前）：
    ```rust
    self.emit(AgentEvent::LlmCallStart {
        step,
        messages: state.messages().to_vec(),
    });
    ```
  - 调用成功后（拿到 `reasoning` 之后）：
    ```rust
    let llm_output = reasoning.final_answer.as_deref()
        .unwrap_or(&reasoning.thought)
        .to_string();
    self.emit(AgentEvent::LlmCallEnd {
        step,
        model: self.llm.model_name(),
        output: llm_output,
        usage: reasoning.usage.clone(),
    });
    ```

- [x] 更新 `llm/anthropic.rs` 和 `llm/openai.rs` 中 `ReactLLM` 的直接实现（这两个文件也直接实现了 `ReactLLM` trait），在 `generate_reasoning` 中填充 `reasoning.usage` 和 `reasoning.model`
  - 当前 `ChatAnthropic::generate_reasoning` 调用 `self.invoke(request)` 拿到 `LlmResponse`，需从 response.usage 同步到 reasoning.usage
  - `model_name()` 直接覆写返回 `self.model.clone()`

**检查步骤:**

- [x] 编译通过
  - `cargo build -p rust-create-agent 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出

- [x] 现有测试仍然通过（`AgentEvent` 新变体不影响现有 `match`，因为新 handler 含 `_ => {}` 兜底）
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok`

- [x] 确认 events.rs 中新变体存在
  - `grep -n "LlmCallStart\|LlmCallEnd" rust-create-agent/src/agent/events.rs`
  - 预期: 两行输出，各含对应变体名

---

### Task 3: TUI 新建 `langfuse` 模块

**涉及文件:**
- 新建: `rust-agent-tui/src/langfuse/config.rs`
- 新建: `rust-agent-tui/src/langfuse/mod.rs`

**执行步骤:**

- [x] 新建 `rust-agent-tui/src/langfuse/config.rs`，实现 `LangfuseConfig::from_env()`：
  ```rust
  pub struct LangfuseConfig {
      pub public_key: String,
      pub secret_key: String,
      pub host: String,
  }
  impl LangfuseConfig {
      pub fn from_env() -> Option<Self> {
          let public_key = std::env::var("LANGFUSE_PUBLIC_KEY").ok()?;
          let secret_key = std::env::var("LANGFUSE_SECRET_KEY").ok()?;
          let host = std::env::var("LANGFUSE_HOST")
              .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());
          Some(Self { public_key, secret_key, host })
      }
  }
  ```

- [x] 新建 `rust-agent-tui/src/langfuse/mod.rs`，实现 `LangfuseTracer`：
  - 字段：`batcher: Arc<langfuse_ergonomic::Batcher>`，`trace_id: Option<String>`，`thread_id: Option<String>`，`generation_ids: HashMap<usize, String>`，`pending_span: Option<(String, String)>`（(tool_call_id, span_id)，FIFO 关联 ToolEnd）
  - `new(config: LangfuseConfig) -> Option<Self>`：从配置构造 Batcher；若 Batcher 初始化失败则返回 None（静默降级）
    - Batcher 构造：`Batcher::builder().client(client).max_events(50).flush_interval(Duration::from_secs(10)).backpressure_policy(BackpressurePolicy::Drop).build().await`
    - 注意：`Batcher::build()` 是 async，需在 async 上下文中调用
  - `on_trace_start(&mut self, input: &str, thread_id: Option<&str>)`：
    - 生成 trace_id（`uuid::Uuid::now_v7().to_string()`）
    - 调用 `batcher.send()` 发送 `IngestionEvent::TraceCreate(CreateTraceBody::builder().id(trace_id).name("agent-run").input(json!(input)).session_id(thread_id).build())`
  - `on_llm_start(&mut self, step: usize, messages: &[BaseMessage])`：
    - 生成 generation_id，存入 `generation_ids[step]`
    - 调用 `batcher.send()` 发送 GenerationCreate（含 trace_id、input=messages 序列化为 JSON）
  - `on_llm_end(&mut self, step: usize, model: &str, output: &str, usage: Option<&TokenUsage>)`：
    - 取出 `generation_ids[step]`，发送 GenerationUpdate（含 model、output、usage）
  - `on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value)`：
    - 生成 span_id，存入 `pending_span = Some((tool_call_id.to_string(), span_id))`
    - 发送 SpanCreate（含 trace_id、name、input）
  - `on_tool_end_by_name_order(&mut self, output: &str, is_error: bool)`：
    - 取出 `pending_span`，发送 SpanUpdate（含 output、status_message）
  - `on_trace_end(&mut self, final_answer: &str)`：
    - 发送 TraceUpdate（含 output=final_answer）

- [x] `mod.rs` 在文件顶部 `pub mod config;`，并 `pub use config::LangfuseConfig;`

**检查步骤:**

- [x] 两个文件创建成功
  - `ls rust-agent-tui/src/langfuse/`
  - 预期: 输出包含 `config.rs` 和 `mod.rs`

- [x] 单独编译 langfuse 模块时无语法错误（需 Task 4 添加依赖后）
  - 先执行 Task 4 的 Cargo.toml 修改，再运行
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error\[" | head -10`
  - 预期: 无输出

---

### Task 4: TUI 集成 Langfuse Tracer

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`
- 修改: `rust-agent-tui/src/main.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**

- [x] 在 `rust-agent-tui/Cargo.toml` 的 `[dependencies]` 中添加：
  ```toml
  langfuse-ergonomic = "0.6.3"
  ```

- [x] 在 `rust-agent-tui/src/main.rs` 的模块声明区增加：
  ```rust
  mod langfuse;
  ```

- [x] 在 `app/mod.rs` 的 `App` 结构体中增加字段：
  ```rust
  /// 当前轮次的 Langfuse Tracer（submit_message 时创建，Done 时结束）
  langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
  ```
  注意：同时在 `App::new()` 和 `App::new_headless()` 的初始化中将此字段设为 `None`

- [x] 在 `app/mod.rs` 的 `submit_message` 方法中，在 `tokio::spawn` 之前，构造 tracer：
  ```rust
  let langfuse_tracer = crate::langfuse::LangfuseConfig::from_env()
      .and_then(|cfg| {
          // Batcher 需要 async 上下文，使用 block_in_place
          tokio::task::block_in_place(|| {
              tokio::runtime::Handle::current().block_on(
                  crate::langfuse::LangfuseTracer::new(cfg)
              )
          })
      })
      .map(|mut t| {
          t.on_trace_start(&input, self.current_thread_id.as_deref());
          Arc::new(parking_lot::Mutex::new(t))
      });
  self.langfuse_tracer = langfuse_tracer.clone();
  ```

- [x] 在 `app/mod.rs` 的 `submit_message` 中，将 `langfuse_tracer.clone()` 传入 `run_universal_agent` 调用

- [x] 在 `app/mod.rs` 的 `handle_agent_event` 的 `AgentEvent::Done` 分支，在 `set_loading(false)` 之前调用：
  ```rust
  // 提取最终答案（最后一个 AssistantBubble 的文字内容）
  let final_answer = self.view_messages.iter().rev()
      .find_map(|m| if let MessageViewModel::AssistantBubble { blocks, .. } = m {
          blocks.iter().find_map(|b| if let ContentBlockView::Text { raw, .. } = b { Some(raw.clone()) } else { None })
      } else { None })
      .unwrap_or_default();
  if let Some(ref tracer) = self.langfuse_tracer {
      tracer.lock().on_trace_end(&final_answer);
  }
  self.langfuse_tracer = None;
  ```

- [x] 在 `app/agent.rs` 的 `run_universal_agent` 函数签名中新增参数：
  ```rust
  langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
  ```

- [x] 在 `app/agent.rs` 的 `FnEventHandler` 闭包内，在原有 TUI 事件映射逻辑**之前**插入 Langfuse 调用：
  ```rust
  // Langfuse hook（在 TUI 事件映射前执行，使用原始 ExecutorEvent）
  if let Some(ref tracer) = langfuse_for_handler {
      let mut t = tracer.lock();
      match &event {
          ExecutorEvent::LlmCallStart { step, messages } =>
              t.on_llm_start(*step, messages),
          ExecutorEvent::LlmCallEnd { step, model, output, usage } =>
              t.on_llm_end(*step, model, output, usage.as_ref()),
          ExecutorEvent::ToolStart { tool_call_id, name, input } =>
              t.on_tool_start(tool_call_id, name, input),
          ExecutorEvent::ToolEnd { is_error, output, .. } =>
              t.on_tool_end_by_name_order(output, *is_error),
          _ => {}
      }
  }
  ```
  其中 `langfuse_for_handler` 是在创建闭包前 `let langfuse_for_handler = langfuse_tracer.clone();`

**检查步骤:**

- [x] 全量编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -10`
  - 预期: 无输出

- [x] 未设置环境变量时 TUI 二进制正常退出（静默跳过 Langfuse）
  - `env -i HOME=$HOME ANTHROPIC_API_KEY=fake timeout 2 cargo run -p rust-agent-tui 2>&1 | grep -i "panic\|langfuse" | head -5`
  - 预期: 无 panic 输出，Langfuse 相关无错误日志

- [x] rust-create-agent 测试仍全绿
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok`

---

### Task 5: Langfuse TUI 监控 Acceptance

**Prerequisites:**
- 启动命令: `LANGFUSE_PUBLIC_KEY=<pk> LANGFUSE_SECRET_KEY=<sk> cargo run -p rust-agent-tui`
- 测试前提: 拥有可访问的 Langfuse 实例（cloud.langfuse.com 或自托管），并生成有效的 Public/Secret Key 对
- 环境准备: 在 `rust-agent-tui/.env` 中设置 `LANGFUSE_PUBLIC_KEY`、`LANGFUSE_SECRET_KEY`（可选 `LANGFUSE_HOST`）

**End-to-end verification:**

1. [x] 编译验证：全 workspace 编译通过
   - `cargo build 2>&1 | grep -E "^error" | head -5`
   - Expected: 无输出（零编译错误）
   - On failure: check Task 1（LLM 类型） 或 Task 2（AgentEvent）编译问题

2. [x] 静默降级：未配置 LANGFUSE_PUBLIC_KEY 时 TUI 正常启动不崩溃
   - `env -i HOME=$HOME timeout 3 cargo run -p rust-agent-tui -- -y 2>&1 | grep -c "panic"`
   - Expected: 输出 `0`（无 panic）
   - On failure: check Task 4 `LangfuseConfig::from_env()` 空值处理

3. [x] `TokenUsage` 序列化正确（Rust 单元测试）
   - `cargo test -p rust-create-agent --lib 2>&1 | tail -3`
   - Expected: 输出包含 `test result: ok`
   - On failure: check Task 1 `TokenUsage` derive 宏或 LlmResponse 构造

4. [x] 环境变量配置检测
   - `grep -n "LANGFUSE_PUBLIC_KEY\|LANGFUSE_SECRET_KEY\|LANGFUSE_HOST" rust-agent-tui/src/langfuse/config.rs`
   - Expected: 三行输出，各含对应变量名
   - On failure: check Task 3 `LangfuseConfig::from_env()` 实现

5. [x] Langfuse hook 事件调用路径存在
   - `grep -n "LlmCallStart\|LlmCallEnd\|on_llm_start\|on_llm_end" rust-agent-tui/src/app/agent.rs`
   - Expected: 至少 4 行输出（两处 match arm + 两处方法调用）
   - On failure: check Task 4 `FnEventHandler` 中的 Langfuse hook 插入
