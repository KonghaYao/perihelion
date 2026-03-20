# rust-create-agent

Rust Agent 框架，实现 ReAct 循环与可组合中间件系统。与 TypeScript 端的 `@langgraph-js/standard-agent` 在概念上对齐。

## 快速开始

```rust
use rust_create_agent::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = rust_create_agent::telemetry::init_tracing("my-agent").await;

    let agent = ReActAgent::new(MockLLM::always_answer("任务完成"))
        .max_iterations(10)
        .add_middleware(Box::new(LoggingMiddleware::new().verbose()));

    let mut state = AgentState::new("/workspace");
    let output = agent.execute(AgentInput::text("请帮我完成这个任务"), &mut state).await?;

    println!("回答：{}", output.text);
    println!("步骤：{}", output.steps);
    Ok(())
}
```

## 核心概念

### ReActAgent

ReAct 循环的执行器，管理 LLM 推理 → 工具调用 → 结果反馈的完整流程。

```rust
let agent = ReActAgent::new(llm)
    .max_iterations(20)             // 最大循环步数，默认 10
    .register_tool(Box::new(my_tool))
    .add_middleware(Box::new(LoggingMiddleware::new()))
    .with_event_handler(Arc::new(handler));
```

### 中间件（Middleware）

通过实现 `Middleware<S>` trait 在 Agent 生命周期各节点插入逻辑。

```rust
use async_trait::async_trait;
use rust_create_agent::prelude::*;

struct MyMiddleware;

#[async_trait]
impl Middleware<AgentState> for MyMiddleware {
    fn name(&self) -> &str { "my-middleware" }

    async fn before_agent(&self, state: &mut AgentState) -> AgentResult<()> {
        // Agent 开始前执行
        Ok(())
    }

    async fn before_tool(&self, _: &mut AgentState, call: &ToolCall) -> AgentResult<ToolCall> {
        // 工具调用前执行，可修改调用参数
        Ok(call.clone())
    }

    async fn after_tool(&self, _: &mut AgentState, _: &ToolCall, _: &ToolResult) -> AgentResult<()> {
        // 工具调用后执行
        Ok(())
    }

    async fn after_agent(&self, _: &mut AgentState, output: &AgentOutput) -> AgentResult<AgentOutput> {
        // Agent 完成后执行，可修改最终输出
        Ok(output.clone())
    }
}
```

生命周期钩子执行顺序：`before_agent` → (每步) `before_tool` → `after_tool` → `after_agent`，出错时触发 `on_error`。

### 自定义工具（Tool）

```rust
use async_trait::async_trait;
use rust_create_agent::tools::BaseTool;

struct EchoTool;

#[async_trait]
impl BaseTool for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "原样返回输入内容" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            },
            "required": ["message"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(input["message"].as_str().unwrap_or("").to_string())
    }
}
```

### 事件回调（EventHandler）

在不修改中间件的情况下监听关键事件：

```rust
use std::sync::Arc;
use rust_create_agent::prelude::*;

let handler = FnEventHandler(|event| match event {
    AgentEvent::ToolStart { name, .. } => println!("开始调用工具: {name}"),
    AgentEvent::ToolEnd { name, is_error, .. } => println!("工具 {name} 完成，错误={is_error}"),
    AgentEvent::TextChunk(text) => println!("回答: {text}"),
    AgentEvent::StepDone { step } => println!("步骤 {step} 完成"),
});

let agent = ReActAgent::new(llm)
    .with_event_handler(Arc::new(handler));
```

## Telemetry（可观测性）

### 基本用法

在 `main` 入口调用一次，其余自动处理：

```rust
let _guard = rust_create_agent::telemetry::init_tracing("my-agent").await;
// _guard 必须存活到程序退出，drop 时自动 flush
```

### 开关控制

**不配置环境变量则不开启 OTLP**，仅输出到 stdout：

| 环境变量                      | 说明                                            |
| ----------------------------- | ----------------------------------------------- |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | 设置后自动启用 OTLP 导出，未设置则只输出 stdout |
| `RUST_LOG`                    | 日志级别，默认 `info`                           |
| `RUST_LOG_FORMAT=json`        | 使用 JSON 格式输出（默认 pretty）               |

```bash
# 仅 stdout 输出（默认行为）
cargo run

# 开启 OTLP 导出到本地 Jaeger
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run --features otel

# 调整日志级别
RUST_LOG=debug cargo run
RUST_LOG=rust_create_agent=trace cargo run
```

### 本地可视化（Jaeger）

项目根目录提供了 `docker-compose.otel.yml`，一键启动 Jaeger（内置 OTLP 接收器 + UI）：

```bash
# 启动
docker compose -f docker-compose.otel.yml up -d

# 停止
docker compose -f docker-compose.otel.yml down
```

启动后：

- **可视化 UI**：<http://localhost:16686>
- **OTLP HTTP**：`http://localhost:4318`（`OTEL_EXPORTER_OTLP_ENDPOINT` 填这个）
- **OTLP gRPC**：`localhost:4317`

### otel Feature

OTLP 导出功能通过 Cargo feature 控制，默认不编译进二进制：

```toml
# Cargo.toml
[dependencies]
rust-create-agent = { version = "*", features = ["otel"] }
```

| 场景                     | 配置                                              | 结果                        |
| ------------------------ | ------------------------------------------------- | --------------------------- |
| 开发/测试                | 无                                                | 只输出到 stdout             |
| 生产（有 Collector）     | `OTEL_EXPORTER_OTLP_ENDPOINT` + `--features otel` | 同时导出 trace              |
| 配置了变量但未开 feature | `OTEL_EXPORTER_OTLP_ENDPOINT`（无 feature）       | 打印 warn，降级为 stdout    |
| OTLP 初始化失败          | 网络不通等                                        | 打印 warn，自动降级，不崩溃 |

`ReActAgent::execute()`、每次工具调用均已自动埋点，无需额外代码。

## Cargo Features

| Feature | 默认 | 说明                                                                                           |
| ------- | ---- | ---------------------------------------------------------------------------------------------- |
| `otel`  | 否   | 启用 OpenTelemetry OTLP 导出（`opentelemetry`、`opentelemetry-otlp`、`tracing-opentelemetry`） |
