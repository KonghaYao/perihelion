# Feature: 20260324_F001 - langfuse-tui-monitoring

## 需求背景

项目已具备完整的 Agent 执行链路（ReAct 循环、工具调用、多轮对话），但缺乏可观测性：无法追踪每次 LLM 调用的 token 消耗、工具调用的耗时与错误率、各会话的整体质量。Langfuse 是专为 LLM 应用设计的监控平台，支持 Trace / Generation / Span 三级结构，可直接对接成本分析和质量评估。

本 feature 在 **TUI 层**（`rust-agent-tui`）接入 Langfuse，不侵染核心 agent 框架（`rust-create-agent`）。核心 agent 层仅通过扩展现有事件枚举的方式暴露必要 hook，所有 Langfuse 依赖和上报逻辑均封装在 TUI 层。

## 目标

- 接入 Langfuse，对每轮对话（Trace）、每次 LLM 生成（Generation）、每次工具调用（Span）进行全链路追踪
- Agent 层仅新增 2 个轻量事件变体作为 hook，不引入任何监控依赖
- Langfuse 配置通过环境变量读取，未配置时静默跳过，不影响正常使用
- 使用 `Batcher` 异步批量上报，不阻塞 TUI 主循环

## 方案设计

### 架构概览

![Langfuse TUI 集成架构](./images/01-architecture.png)

新增改动仅涉及两个层次：

| 层次 | 改动内容 |
|------|---------|
| `rust-create-agent` | `AgentEvent` 枚举新增 `LlmCallStart` / `LlmCallEnd` 两个变体；`ReActAgent` 在 LLM 调用前后 emit |
| `rust-agent-tui` | 新增 `src/langfuse/` 模块；`app/agent.rs` 构造 `LangfuseTracer`；`handle_agent_event` 中调用 tracer |

`rust-langfuse-ergonomic`（即 `langfuse-ergonomic = "0.6.3"` crate）作为上报客户端，只在 `rust-agent-tui` 的 `Cargo.toml` 中依赖。

### Agent 层 Hook 扩展（`rust-create-agent`）

在 `rust-create-agent/src/agent/events.rs` 中新增两个变体：

```rust
/// LLM 调用开始（携带完整 input messages 快照）
LlmCallStart {
    step: usize,
    messages: Vec<BaseMessage>,
},

/// LLM 调用结束
LlmCallEnd {
    step: usize,
    model: String,
    output: String,
    usage: Option<TokenUsage>,
},
```

新增辅助结构：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
```

`rust-create-agent/src/agent/react.rs` 在 `llm.generate_reasoning()` 调用前 emit `LlmCallStart`，调用后 emit `LlmCallEnd`（model 名从 `ReactLLM` trait 获取，usage 来自 LLM 响应可选字段）。

### Langfuse 数据模型映射

![Langfuse 数据流图](./images/02-dataflow.png)

| TUI 侧事件 | Langfuse 对象 | 携带数据 |
|-----------|-------------|---------|
| `submit_message()` 调用 | `Trace` 开始 | `input`=用户消息, `session_id`=thread_id |
| `ExecutorEvent::LlmCallStart` | `Generation` 创建 | `input`=messages 列表 |
| `ExecutorEvent::LlmCallEnd` | `Generation` 更新 | `model`, `output`, `usage` |
| `ExecutorEvent::ToolStart` | `Span` 创建 | `name`=工具名, `input`=参数 |
| `ExecutorEvent::ToolEnd` | `Span` 结束 | `output`, `status_message` |
| `AgentEvent::Done` | `Trace` 结束 | `output`=最终 AI 回答 |

Generation 嵌套在 Trace 内，Span（工具调用）也嵌套在当前 Trace 内，共同构成一次对话轮的完整可观测树。

### `LangfuseTracer` 设计

新增文件：`rust-agent-tui/src/langfuse/mod.rs`

```rust
pub struct LangfuseTracer {
    batcher: Arc<Batcher>,
    // 当前 Trace 状态
    trace_id: Option<String>,
    thread_id: Option<String>,
    // step → generation_id（跨多轮 LLM 调用）
    generation_ids: HashMap<usize, String>,
    // tool_call_id → span_id
    span_ids: HashMap<String, String>,
    // 累积最终答案（用于 Trace.output）
    final_answer: String,
}
```

核心方法（均为同步，内部通过 `batcher.send()` 异步推送）：

```rust
pub fn on_trace_start(&mut self, input: &str, thread_id: Option<&str>)
pub fn on_llm_start(&mut self, step: usize, messages: &[BaseMessage])
pub fn on_llm_end(&mut self, step: usize, model: &str, output: &str, usage: Option<&TokenUsage>)
pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value)
pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool)
pub fn on_trace_end(&mut self, final_answer: &str)
```

新增文件：`rust-agent-tui/src/langfuse/config.rs`

```rust
pub struct LangfuseConfig {
    pub public_key: String,
    pub secret_key: String,
    pub host: String,  // 默认 "https://cloud.langfuse.com"
}

impl LangfuseConfig {
    /// 从环境变量读取，任一缺失则返回 None（静默禁用）
    pub fn from_env() -> Option<Self> { ... }
}
```

### TUI 集成点

**`app/agent.rs`（`run_universal_agent`）**

```rust
pub async fn run_universal_agent(
    ...
    tracer: Option<Arc<Mutex<LangfuseTracer>>>,  // 新增参数
) {
    // 在 FnEventHandler 闭包中 clone tracer，调用对应方法
    let handler = Arc::new(FnEventHandler(move |event: ExecutorEvent| {
        if let Some(ref t) = tracer_for_handler {
            match &event {
                ExecutorEvent::LlmCallStart { step, messages } =>
                    t.lock().on_llm_start(*step, messages),
                ExecutorEvent::LlmCallEnd { step, model, output, usage } =>
                    t.lock().on_llm_end(*step, model, output, usage.as_ref()),
                ExecutorEvent::ToolStart { tool_call_id, name, input } =>
                    t.lock().on_tool_start(tool_call_id, name, input),
                ExecutorEvent::ToolEnd { name: _, output, is_error } =>
                    // tool_call_id 从 ToolStart 时已缓存
                    t.lock().on_tool_end_by_output(output, *is_error),
                _ => {}
            }
        }
        // ... 原有 TUI 事件映射逻辑不变
    }));
}
```

**`app/mod.rs`（`submit_message`）**

在发起 Agent 任务前构造 tracer，并在 `Done` 事件处理中调用 `on_trace_end`：

```rust
// submit_message 中
let tracer = LangfuseConfig::from_env().map(|cfg| {
    let batcher = /* 构造 Batcher */;
    let mut t = LangfuseTracer::new(batcher);
    t.on_trace_start(&input, self.current_thread_id.as_deref());
    Arc::new(Mutex::new(t))
});
```

### `ToolEnd` tool_call_id 传递问题

现有 `ExecutorEvent::ToolEnd` 不包含 `tool_call_id`，只有 `name`。为了匹配 Span，`LangfuseTracer` 内维护 `pending_span: Option<(String, String)>` 即 `(tool_call_id, span_id)`，按 FIFO 顺序关联（工具串行执行，不存在并发混淆）。

若后续需精确匹配，可在 agent 层 `ToolEnd` 事件中补充 `tool_call_id` 字段（单独 PR）。

## 实现要点

1. **`langfuse-ergonomic` 版本**：使用 `0.6.3`，通过 `Batcher` 批量上报，`BackpressurePolicy::Drop` 避免 OOM
2. **`Batcher` 生命周期**：每次 `submit_message` 创建新 `Batcher` 实例（独立 flush）；Batcher 在 `Done` 后 Drop 触发最终 flush
3. **线程安全**：`LangfuseTracer` 包装在 `Arc<parking_lot::Mutex<>>` 中，在 FnEventHandler 闭包（tokio 任务）和主线程（`handle_agent_event`）间共享
4. **`TokenUsage` 可选性**：`LlmCallEnd` 的 usage 字段为 `Option<TokenUsage>`，不支持 usage 的 LLM（如某些 OpenAI 兼容代理）跳过 usage 上报，不报错
5. **ReactLLM trait 扩展**：`model_name()` 方法若还不存在，需在 `ReactLLM` trait 中新增，返回当前模型标识符

## 约束一致性

- 不在 `rust-create-agent` 或 `rust-agent-middlewares` 引入 Langfuse 依赖，保持核心层轻量
- 仅在 `rust-agent-tui` 的 `Cargo.toml` 引入 `langfuse-ergonomic`，符合"仅在 TUI 层使用"的要求
- `AgentEvent` 新增的 2 个变体完全向后兼容（枚举新增变体，调用方 match 添加 `_ => {}` 即可）
- `LangfuseTracer` 使用 `parking_lot::Mutex`，与项目现有锁库一致（`app/mod.rs` 已使用 `parking_lot`）
- `Batcher` 异步批量上报不阻塞 tokio 主线程，符合 TUI 响应性要求

## 验收标准

- [ ] 设置环境变量后，TUI 运行时每次用户发送消息在 Langfuse 控制台可见新 Trace
- [ ] Trace 下包含正确数量的 Generation（等于本轮 LLM 调用次数）
- [ ] Trace 下包含正确数量的 Span（等于本轮工具调用次数）
- [ ] Generation 携带 `model` 字段和 `usage`（input/output tokens，如 LLM 支持）
- [ ] 未设置 `LANGFUSE_PUBLIC_KEY` 时 TUI 正常启动，无 panic 或错误日志
- [ ] TUI 响应性无明显下降（Batcher 异步不阻塞主循环）
- [ ] `cargo build` 通过，无编译错误
