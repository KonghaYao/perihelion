# Feature: 20260325_F003 - langfuse-observation-types

## 需求背景

当前 Langfuse 追踪记录存在三处偏差：

1. **Generation 名称错误**：LLM 调用记录的 `name` 字段为 `llm-call-step-N`，不符合可读性要求，应改为 `ChatOpenAI` 或 `ChatAnthropic`，与 LangChain 等主流框架的命名约定一致。
2. **缺少 Agent 层级观测**：每轮 Agent ReAct 循环没有一个总的 Agent 类型 Observation 包裹，所有 Generation 和 Tool 观测直接挂在 Trace 下，缺乏层次结构。
3. **工具调用未使用 Tool 类型**：当前工具调用使用 `span-create` API（`ObservationType::Span`），应改为 `observation-create` API 并指定 `ObservationType::Tool`，使 Langfuse UI 展示正确的图标和类型标签。

## 目标

- Generation 名称改为 `ChatOpenAI` / `ChatAnthropic`，基于实际使用的 Provider 区分
- 每次 agent 执行创建一个 `ObservationType::Agent` Observation，包裹整个 ReAct 循环
- 工具调用创建 `ObservationType::Tool` Observation（而非 SPAN）
- Generation 和 Tool 观测通过 `parent_observation_id` 挂在 Agent Observation 下，形成正确层级

## 方案设计

### Langfuse 观测层级目标结构

![Langfuse 观测层级结构](./images/01-flow.png)

```
Session (session_id = thread_id)
  └── Trace (trace_id, name="agent-run")
        └── Observation(type=Agent, name="Agent")         ← 新增，包裹整个循环
              ├── Observation(type=Generation, name="ChatAnthropic" 或 "ChatOpenAI")
              ├── Observation(type=Tool, name=tool_name)   ← 改为 Tool 类型
              ├── Observation(type=Generation, ...)        ← 下一次 LLM 调用
              └── Observation(type=Tool, ...)
```

### API 类型说明

`langfuse-client-base` 的 `ObservationType` 枚举包含 `Agent`、`Tool`、`Generation`、`Span` 等值。携带 `type` 字段的唯一途径是通过 `IngestionEventOneOf8`（`observation-create`）API，使用 `ObservationBody`。

当前工具调用使用 `span-create`（`IngestionEventOneOf2`），其 `CreateSpanBody` 没有 `type` 字段，因此需要切换到 `observation-create`（`IngestionEventOneOf8`）。

### LangfuseTracer 字段变更

**新增字段：**

```rust
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,
    trace_id: String,
    agent_span_id: String,        // ← 新增：Agent Observation 的 ID
    generation_data: HashMap<usize, (String, Vec<BaseMessage>)>,
    pending_spans: VecDeque<String>,
}
```

### 方法变更详情

#### `on_trace_start`（修改）

除了创建 Trace，额外通过 Batcher 创建 `Agent` 类型 Observation：

```rust
// 额外创建 Agent Observation（通过 Batcher）
let body = ObservationBody {
    id: Some(Some(agent_span_id.clone())),
    trace_id: Some(Some(trace_id.clone())),
    r#type: ObservationType::Agent,
    name: Some(Some("Agent".to_string())),
    input: Some(Some(serde_json::json!(input))),
    start_time: Some(Some(timestamp)),
    ..Default::default()
};
// 用 IngestionEventOneOf8 包装，通过 batcher.add() 上报
```

#### `on_llm_end`（修改）

增加 `provider: &str` 参数，名称改为 `format!("Chat{}", provider)`，并添加 `parent_observation_id`：

```rust
pub fn on_llm_end(
    &mut self,
    step: usize,
    model: &str,
    provider: &str,   // ← 新增参数："OpenAI" 或 "Anthropic"
    output: &str,
    usage: Option<&TokenUsage>,
) {
    // name = "ChatOpenAI" 或 "ChatAnthropic"
    let name = format!("Chat{}", provider);
    // CreateGenerationBody 新增：
    // parent_observation_id: Some(Some(self.agent_span_id.clone()))
}
```

#### `on_tool_start`（修改）

从 `client.span()` 直接调用改为通过 Batcher 上报 `ObservationType::Tool`：

```rust
pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
    let span_id = uuid::Uuid::now_v7().to_string();
    self.pending_spans.push_back(span_id.clone());
    // 用 IngestionEventOneOf8(type=Tool) + parent_observation_id=agent_span_id
    // 通过 batcher.add() 异步上报，不再 tokio::spawn + client.span()
}
```

#### `on_tool_end_by_name_order`（修改）

从 `client.update_span()` 改为通过 Batcher 上报 `span-update`（`IngestionEventOneOf3`）：

```rust
pub fn on_tool_end_by_name_order(&mut self, output: &str, is_error: bool) {
    // 用 IngestionEventOneOf3(span-update) + Batcher，不再 tokio::spawn + client.update_span()
}
```

#### `on_trace_end`（修改）

额外更新 Agent Observation 的 `end_time`：

```rust
pub fn on_trace_end(&mut self, final_answer: &str) {
    // 原有 Trace 更新保持不变
    // 额外：ObservationUpdate(agent_span_id, end_time=now) via Batcher
}
```

### app/agent.rs 变更

在构建 handler closure 之前，从 `provider` 派生出 `provider_name`，并在 closure 中捕获：

```rust
// provider 被 into_model() 消耗前，提取名称
let provider_name = provider.display_name().to_string(); // "OpenAI" 或 "Anthropic"

// ... 在 langfuse hook 分支中：
ExecutorEvent::LlmCallEnd { step, model, output, usage } =>
    t.on_llm_end(*step, model, &provider_name, output, usage.as_ref()),
```

### 改动文件汇总

| 文件 | 改动类型 | 主要内容 |
|------|---------|---------|
| `rust-agent-tui/src/langfuse/mod.rs` | 修改 | 新增 `agent_span_id` 字段；`on_trace_start` 创建 Agent Observation；`on_llm_end` 增加 `provider` 参数、改名称、加 parent_id；`on_tool_start/end` 改用 observation-create/span-update via Batcher |
| `rust-agent-tui/src/app/agent.rs` | 修改 | 提取 `provider_name`，传给 `on_llm_end` |

**改动量估计：** ~60 行，不引入新依赖，不修改 core crate。

## 实现要点

- `ObservationType::Agent/Tool` 只能通过 `IngestionEventOneOf8`（observation-create）设置，`span-create` 的 `CreateSpanBody` 没有 `type` 字段
- Agent Observation 的 `agent_span_id` 在 `on_trace_start` 时提前生成并存入 tracer 字段，后续所有观测使用它作为 `parent_observation_id`
- `provider_name` 在 `agent.rs` 中从 `LlmProvider::display_name()` 取得，在 closure 中捕获，不修改 `LlmCallEnd` 事件结构（避免跨 crate 改动）
- 工具调用 update 路径继续使用 `span-update`（`IngestionEventOneOf3`），Langfuse 服务端按 ID 匹配，兼容 observation-create 创建的记录
- `on_trace_end` 时同时更新 Agent Observation 的 end_time，使 Langfuse UI 中 Agent span 的持续时间可见

## 约束一致性

- 仅修改 `rust-agent-tui`（应用层），不涉及 `rust-create-agent` core crate，符合「Workspace 多 crate 分层」约束
- 所有 Batcher 操作均为 `tokio::spawn` 异步执行，符合「异步优先」约束
- 无新增依赖（`langfuse-client-base` 已是现有依赖），无 API 破坏性变更

## 验收标准

- [ ] Langfuse UI 中 LLM 调用记录名称显示为 `ChatOpenAI` 或 `ChatAnthropic`（不再是 `llm-call-step-N`）
- [ ] 每次 agent 执行在 Trace 下有一个 `type=Agent` 的 Observation，包裹整个循环
- [ ] 工具调用 Observation 类型为 `Tool`（不再是 `SPAN`），并挂在 Agent Observation 下
- [ ] `cargo build -p rust-agent-tui` 编译通过
- [ ] `cargo test -p rust-agent-tui` 测试全部通过
