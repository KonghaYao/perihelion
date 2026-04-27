# Feature: 20260330_F004 - langfuse-client

## 需求背景

当前项目使用 `langfuse-ergonomic` + `langfuse-client-base` 两个第三方 crate 实现 Langfuse 追踪上报。存在以下问题：

1. **生成代码质量差**：API 类型由 openapi-generator 生成，代码冗余（大量 `Option<Option<T>>` 嵌套）、可读性差、调试困难
2. **已知 bug**：`langfuse-ergonomic` 的 Batcher 有 usage 字段处理错误等问题，需要手动绕过
3. **V4 不兼容**：无法对接 Langfuse V4 observation-centric 数据模型（单表 immutable、`x-langfuse-ingestion-version: 4`）
4. **依赖不可控**：外部 crate 无法按需定制背压策略、批量逻辑、重试机制等

项目已有 `langfuse-client/` 目录（含 `Concept.md`、`DOC.md`、`langfuse-api.json`），可以作为新 crate 的载体。

## 目标

- 在 `langfuse-client/` 目录下新建独立 Rust crate，手工实现 Langfuse Ingestion API 客户端
- 两层设计：底层 Client（认证 + HTTP）+ 上层 Batcher（批量聚合 + 背压 + flush）
- 仅覆盖 V4 ingestion 端点（`POST /api/public/ingestion`），不涉及查询/管理 API
- 替换 `rust-agent-tui` 中的 `langfuse-ergonomic` + `langfuse-client-base` 依赖
- 所有类型手工定义，无生成代码，消除 `Option<Option<T>>` 嵌套

## 方案设计

### 架构概览

![架构概览](./images/01-architecture.png)

两层 API 设计，上层 Batcher 封装批量逻辑，底层 Client 封装 HTTP 通信：

```
┌─────────────────────────────────────────────┐
│  上层：Batcher                              │
│  - 异步任务循环，定时/定量 flush              │
│  - 背压策略（DropNew / Block）               │
│  - flush() 手动触发（短生命周期应用）         │
│  - 事件队列：tokio::mpsc 有界 channel        │
└────────────────┬────────────────────────────┘
                 │ 调用
┌────────────────▼────────────────────────────┐
│  底层：Client                               │
│  - reqwest::Client（复用连接池）              │
│  - Basic Auth 认证                          │
│  - POST /api/public/ingestion               │
│  - 请求/响应序列化（serde_json）              │
│  - 自动重试（可配置次数，指数退避）            │
│  - 207 多状态响应解析（success + errors）      │
└─────────────────────────────────────────────┘
```

### 模块结构

```
langfuse-client/
├── Cargo.toml
├── Concept.md            # Langfuse 概念文档（参考资料）
├── DOC.md                # V4 header 说明（参考资料）
├── langfuse-api.json     # OpenAPI spec（参考资料）
├── src/
│   ├── lib.rs            # pub mod 声明
│   ├── client.rs         # Client 结构体 + HTTP 调用
│   ├── batcher.rs        # Batcher 结构体 + 异步 flush 循环
│   ├── types.rs          # IngestionEvent 枚举 + ObservationBody 等
│   ├── config.rs         # ClientConfig / BatcherConfig
│   └── error.rs          # LangfuseError 统一错误类型
```

### 类型设计

#### IngestionEvent — 10 种事件类型

使用 Rust enum 表达所有 ingestion 事件，通过 serde 内部标签自动序列化 `type` 判别字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum IngestionEvent {
    TraceCreate {
        id: String,
        timestamp: String,
        body: TraceBody,
    },
    SpanCreate {
        id: String,
        timestamp: String,
        body: SpanBody,
    },
    SpanUpdate {
        id: String,
        timestamp: String,
        body: SpanBody,
    },
    GenerationCreate {
        id: String,
        timestamp: String,
        body: GenerationBody,
    },
    GenerationUpdate {
        id: String,
        timestamp: String,
        body: GenerationBody,
    },
    EventCreate {
        id: String,
        timestamp: String,
        body: EventBody,
    },
    ScoreCreate {
        id: String,
        timestamp: String,
        body: ScoreBody,
    },
    ObservationCreate {
        id: String,
        timestamp: String,
        body: ObservationBody,
    },
    ObservationUpdate {
        id: String,
        timestamp: String,
        body: ObservationBody,
    },
    SdkLog {
        id: String,
        timestamp: String,
        body: SdkLogBody,
    },
}
```

#### ObservationBody — V4 统一观测类型

所有字段直接用 `Option<T>`，无 `Option<Option<T>>` 嵌套：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationBody {
    pub id: Option<String>,
    pub trace_id: Option<String>,
    pub r#type: ObservationType,
    pub name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub completion_start_time: Option<String>,
    pub parent_observation_id: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub model: Option<String>,
    pub model_parameters: Option<HashMap<String, serde_json::Value>>,
    pub usage: Option<Usage>,
    pub usage_details: Option<UsageDetails>,
    pub level: Option<ObservationLevel>,
    pub status_message: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
}
```

#### ObservationType — 10 种观测类型

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationType {
    Span, Generation, Event, Agent, Tool,
    Chain, Retriever, Evaluator, Embedding, Guardrail,
}
```

#### Usage & UsageDetails

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input: Option<i32>,
    pub output: Option<i32>,
    pub total: Option<i32>,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub total_cost: Option<f64>,
    pub unit: Option<String>,
}

// UsageDetails: 灵活的 key-value map（如 cache tokens 等扩展字段）
pub type UsageDetails = HashMap<String, i32>;
```

### 底层 Client 设计

```rust
pub struct LangfuseClient {
    http: reqwest::Client,
    base_url: String,
    auth_header: String,  // "Basic {base64(public_key:secret_key)}"
}

impl LangfuseClient {
    /// 构造客户端
    pub fn new(public_key: &str, secret_key: &str, base_url: &str) -> Self {
        let auth = base64::encode(format!("{}:{}", public_key, secret_key));
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.to_string(),
            auth_header: format!("Basic {}", auth),
        }
    }

    /// 发送一批 ingestion 事件
    /// POST /api/public/ingestion
    /// Header: x-langfuse-ingestion-version: 4
    pub async fn ingest(
        &self,
        batch: Vec<IngestionEvent>,
    ) -> Result<IngestionResponse, LangfuseError>;
}
```

**关键设计点：**

| 特性 | 实现 |
|------|------|
| 认证 | `Authorization: Basic {base64(public_key:secret_key)}` |
| V4 标识 | `x-langfuse-ingestion-version: 4` 请求头 |
| 响应处理 | 207 Multi-Status，解析 `successes` + `errors` 列表 |
| 错误重试 | 网络错误自动重试（可配置次数，指数退避） |
| 连接复用 | `reqwest::Client` 内部连接池 |
| 批次限制 | 3.5MB 由 Batcher 层控制（max_events 间接限制） |

### 上层 Batcher 设计

![Batcher 数据流](./images/02-batcher-flow.png)

```rust
pub enum BackpressurePolicy {
    DropNew,   // 队列满时丢弃新事件
    Block,     // 队列满时阻塞等待
}

pub struct BatcherConfig {
    pub max_events: usize,              // 每批最大事件数（默认 50）
    pub flush_interval: Duration,       // 定时 flush 间隔（默认 10s）
    pub backpressure: BackpressurePolicy,  // 背压策略（默认 DropNew）
    pub max_retries: usize,             // HTTP 重试次数（默认 3）
}

pub struct Batcher {
    client: Arc<LangfuseClient>,
    tx: mpsc::Sender<BatcherCommand>,   // 事件入队通道
}

enum BatcherCommand {
    Add(IngestionEvent),                 // 添加事件
    Flush(oneshot::Sender<()>),          // 手动 flush（完成后通知）
    Shutdown,                            // 关闭后台任务
}
```

**核心机制：**

1. **事件入队**：`add()` 通过 `mpsc::Sender` 发送事件到后台 task
2. **批量聚合**：后台 task 收集事件到 `Vec<IngestionEvent>`，达到 `max_events` 或 `flush_interval` 到期时调用 `client.ingest()`
3. **定时 flush**：`tokio::time::interval` 驱动定期发送
4. **手动 flush**：`flush()` 通过 oneshot channel 通知后台 task 立即发送，等待完成后返回
5. **背压控制**：`mpsc::channel(max_events)` 有界通道，DropNew 时用 `try_send`，Block 时用 `send`
6. **优雅关闭**：Drop 时发送 Shutdown 命令，后台 task 先 flush 剩余事件再退出

### 与现有系统集成

替换路径：

```
rust-agent-tui/Cargo.toml:
  - 移除: langfuse-ergonomic = "0.6.3"
  - 移除: langfuse-client-base = "0.7.1"
  - 新增: langfuse-client = { path = "../../langfuse-client" }

rust-agent-tui/src/langfuse/:
  session.rs  → 改用 langfuse_client::Batcher + LangfuseClient
  tracer.rs   → 改用 langfuse_client::types::* (IngestionEvent, ObservationBody 等)
  config.rs   → 保持不变（环境变量读取逻辑不变）
```

**对外接口不变**：`LangfuseSession` / `LangfuseTracer` 行为保持一致，仅替换底层实现。

## 实现要点

1. **serde 内部标签**：`#[serde(tag = "type", rename_all = "kebab-case")]` 自动处理 IngestionEvent 的 `type` 判别字段，无需手工序列化
2. **避免 Option<Option<T>)**：所有 body 字段直接 `Option<T>`，序列化时 `skip_serializing_if = "Option::is_none"` 控制空值
3. **Batcher 后台 task**：用 `tokio::spawn` 启动，持有 `Arc<LangfuseClient>`，通过 `mpsc` 接收命令
4. **flush 确保完成**：`on_trace_end` 中先 join 所有 pending_handles 再 flush，避免竞态
5. **依赖精简**：仅 reqwest + serde + serde_json + tokio + thiserror + chrono + base64
6. **crate 独立**：位于 workspace 之外（`langfuse-client/` 目录），不参与 workspace 统一编译

## 约束一致性

- **技术栈**：符合 constraints.md — Rust 2021 edition + tokio 异步 + reqwest HTTP + thiserror 错误处理
- **依赖方向**：新 crate 无内部依赖，独立于 workspace；`rust-agent-tui` 引用新 crate（单向依赖）
- **编码规范**：遵循 snake_case + PascalCase 命名约定，`thiserror` 定义错误类型
- **日志**：使用 `tracing` 宏记录 warn/error，不使用 `println!`
- **架构偏离**：新 crate 不在 workspace Cargo.toml 中注册，作为 path dependency 引用 — 这是因为它设计为可独立发布的 crate

## 验收标准

- [ ] `LangfuseClient` 能成功 POST 事件到 Langfuse V4 ingestion API（带 `x-langfuse-ingestion-version: 4` header）
- [ ] `Batcher` 支持批量聚合 + 定时 flush + 手动 flush
- [ ] 背压策略 DropNew / Block 可配置且正确工作
- [ ] 所有 10 种 `IngestionEvent` 类型正确序列化（`type` 字段自动填充）
- [ ] `ObservationBody` 字段无 `Option<Option<T>>` 嵌套
- [ ] 207 Multi-Status 响应正确解析（successes + errors 列表）
- [ ] 网络错误自动重试（指数退避）
- [ ] 替换后 `rust-agent-tui` 的 Langfuse 功能正常工作（Trace/Span/Generation/Tool 上报）
- [ ] 单元测试覆盖：类型序列化、Client 请求构建、Batcher 批量逻辑
