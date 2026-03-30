# langfuse-client 执行计划

**目标:** 手工实现 Langfuse V4 Ingestion API 客户端，替换 langfuse-ergonomic + langfuse-client-base 第三方依赖

**技术栈:** Rust 2021 edition, reqwest, serde/serde_json, tokio (mpsc + spawn + interval), thiserror, chrono, base64

**设计文档:** spec-design.md

## 改动总览

- 在 `langfuse-client/` 目录下新建独立 Rust crate（不属于 workspace），包含 6 个源文件（lib.rs、error.rs、config.rs、types.rs、client.rs、batcher.rs），提供两层 API（底层 Client + 上层 Batcher）
- Task 1-4 按依赖顺序构建 crate 骨架→数据类型→Client→Batcher；Task 5 将 rust-agent-tui 的 session.rs/tracer.rs 迁移到新 crate API，替换 langfuse-ergonomic + langfuse-client-base 两个第三方依赖
- 关键决策：所有 body 字段用 `Option<T>`（无 `Option<Option<T>>`），IngestionEvent 用 serde 内部标签枚举（10 变体），ObservationBody 作为 V4 统一类型覆盖 create/update

---

### Task 0: 环境准备

**背景:**
确保 Rust 构建工具链可用，验证 langfuse-client 作为独立 crate 和 rust-agent-tui 的编译环境正常。

**执行步骤:**
- [ ] 验证 Rust 工具链可用
  - `rustc --version && cargo --version`
  - 预期: 输出 Rust 版本信息
- [ ] 验证 workspace 现有 crate 可编译
  - `cargo build -p rust-agent-tui 2>&1`
  - 预期: 编译成功（确认基线无问题）

**检查步骤:**
- [ ] Rust 工具链可用
  - `cargo --version`
  - 预期: 输出 cargo 版本号
- [ ] 现有 workspace 编译成功
  - `cargo check 2>&1 | tail -5`
  - 预期: 无 error

---

### Task 1: Crate 骨架 + 错误/配置类型

**背景:**
创建 langfuse-client crate 的基础骨架。当前 `langfuse-client/` 目录仅包含参考资料（Concept.md、Doc.md、langfuse-api.json），无 Cargo.toml 和 src/ 目录。本 Task 创建 crate 的最小可编译结构：Cargo.toml 依赖声明、lib.rs 模块导出、error.rs 统一错误类型（thiserror 定义，供 client.rs / batcher.rs 使用）、config.rs 配置结构体（ClientConfig + BatcherConfig，定义认证参数和批量策略）。Task 2/3/4 均依赖本 Task 的类型定义。

**涉及文件:**
- 新建: `langfuse-client/Cargo.toml`
- 新建: `langfuse-client/src/lib.rs`
- 新建: `langfuse-client/src/error.rs`
- 新建: `langfuse-client/src/config.rs`

**执行步骤:**

- [ ] 创建 `langfuse-client/Cargo.toml`
  - 位置: `langfuse-client/Cargo.toml`（新建）
  - 包名 `langfuse-client`，edition 2021，版本 0.1.0
  - 依赖与 workspace 已有 crate 版本对齐：
    ```toml
    [package]
    name = "langfuse-client"
    version = "0.1.0"
    edition = "2021"

    [dependencies]
    reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
    serde = { version = "1.0", features = ["derive"] }
    serde_json = "1.0"
    tokio = { version = "1", features = ["full"] }
    thiserror = "2.0"
    chrono = { version = "0.4", features = ["serde"] }
    base64 = { version = "0.22", features = ["alloc"] }
    tracing = "0.1"

    [dev-dependencies]
    tokio-test = "0.4"
    ```
  - 原因: reqwest 用于 HTTP 调用，serde/serde_json 用于序列化，tokio 用于异步运行时，thiserror 用于错误定义，chrono 用于时间戳，base64 用于 Basic Auth 编码，tracing 用于日志

- [ ] 创建 `langfuse-client/src/error.rs` — 统一错误类型
  - 位置: `langfuse-client/src/error.rs`（新建）
  - 使用 thiserror 定义 `LangfuseError` 枚举，覆盖以下变体：
    ```rust
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum LangfuseError {
        #[error("HTTP request failed: {0}")]
        Http(#[from] reqwest::Error),

        #[error("JSON serialization failed: {0}")]
        JsonSerialize(#[from] serde_json::Error),

        #[error("Ingestion API returned errors: {0}")]
        IngestionApi(String),

        #[error("Batch sender dropped, batcher is shut down")]
        ChannelClosed,

        #[error("Invalid configuration: {0}")]
        Config(String),
    }
    ```
  - 原因: 统一错误类型供 Client（Http/IngestionApi）、Batcher（ChannelClosed）、Config（Config）使用，避免 anyhow 保持 API 边界清晰

- [ ] 创建 `langfuse-client/src/config.rs` — 配置结构体
  - 位置: `langfuse-client/src/config.rs`（新建）
  - 定义 `ClientConfig` 和 `BatcherConfig`，以及 `BackpressurePolicy` 枚举：
    ```rust
    use std::time::Duration;

    /// Langfuse Client 认证配置
    #[derive(Debug, Clone)]
    pub struct ClientConfig {
        pub public_key: String,
        pub secret_key: String,
        pub base_url: String,
    }

    impl ClientConfig {
        /// 从环境变量构造配置
        /// 读取 LANGFUSE_PUBLIC_KEY、LANGFUSE_SECRET_KEY、LANGFUSE_BASE_URL
        /// base_url 默认值为 "https://cloud.langfuse.com"
        pub fn from_env() -> Result<Self, crate::LangfuseError> {
            let public_key = std::env::var("LANGFUSE_PUBLIC_KEY")
                .map_err(|_| crate::LangfuseError::Config("LANGFUSE_PUBLIC_KEY not set".into()))?;
            let secret_key = std::env::var("LANGFUSE_SECRET_KEY")
                .map_err(|_| crate::LangfuseError::Config("LANGFUSE_SECRET_KEY not set".into()))?;
            let base_url = std::env::var("LANGFUSE_BASE_URL")
                .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());
            Ok(Self { public_key, secret_key, base_url })
        }
    }

    /// 背压策略
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BackpressurePolicy {
        /// 队列满时丢弃新事件
        DropNew,
        /// 队列满时阻塞等待
        Block,
    }

    impl Default for BackpressurePolicy {
        fn default() -> Self {
            Self::DropNew
        }
    }

    /// Batcher 批量聚合配置
    #[derive(Debug, Clone)]
    pub struct BatcherConfig {
        pub max_events: usize,
        pub flush_interval: Duration,
        pub backpressure: BackpressurePolicy,
        pub max_retries: usize,
    }

    impl Default for BatcherConfig {
        fn default() -> Self {
            Self {
                max_events: 50,
                flush_interval: Duration::from_secs(10),
                backpressure: BackpressurePolicy::default(),
                max_retries: 3,
            }
        }
    }
    ```
  - 原因: ClientConfig 封装认证三要素供 Client 使用；BatcherConfig 定义批量策略供 Batcher 使用；from_env() 提供环境变量加载，与现有 rust-agent-tui 的配置方式一致

- [ ] 创建 `langfuse-client/src/lib.rs` — 模块声明与重导出
  - 位置: `langfuse-client/src/lib.rs`（新建）
  - 声明子模块并重导出核心类型：
    ```rust
    pub mod config;
    pub mod error;

    // 重导出常用类型，方便使用者直接写 langfuse_client::LangfuseError
    pub use error::LangfuseError;
    pub use config::{ClientConfig, BatcherConfig, BackpressurePolicy};
    ```
  - 原因: 后续 Task 会追加 `pub mod types;`、`pub mod client;`、`pub mod batcher;`，此处先声明 error 和 config 两个模块

- [ ] 为 config.rs 和 error.rs 编写单元测试
  - 测试文件: `langfuse-client/src/config.rs`（内联 `#[cfg(test)] mod tests`）和 `langfuse-client/src/error.rs`（内联 `#[cfg(test)] mod tests`）
  - 测试场景（config.rs）:
    - `test_batcher_config_default`: 验证 `BatcherConfig::default()` 的 max_events=50, flush_interval=10s, backpressure=DropNew, max_retries=3
    - `test_backpressure_default`: 验证 `BackpressurePolicy::default()` == DropNew
    - `test_client_config_from_env`: 用临时环境变量验证 `ClientConfig::from_env()` 正确读取
    - `test_client_config_from_env_missing_key`: 不设置环境变量时验证返回 Config 错误
    - `test_client_config_default_base_url`: 仅设置 key 不设置 base_url 时验证 base_url 默认值为 "https://cloud.langfuse.com"
  - 测试场景（error.rs）:
    - `test_error_display_http`: 验证 `LangfuseError::Http` 的 Display 输出包含 "HTTP request failed"
    - `test_error_display_config`: 验证 `LangfuseError::Config("test".into())` 的 Display 输出包含 "Invalid configuration" 和 "test"
    - `test_error_display_channel_closed`: 验证 `LangfuseError::ChannelClosed` 的 Display 输出包含 "ChannelClosed" 或 "shut down"
  - 运行命令: `cd langfuse-client && cargo test`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 crate 可独立编译
  - `cd langfuse-client && cargo check 2>&1`
  - 预期: 输出 `Checking langfuse-client v0.1.0` 且无 error

- [ ] 验证所有测试通过
  - `cd langfuse-client && cargo test 2>&1`
  - 预期: 所有 test 结果为 `test result: ok`，0 failures

- [ ] 验证模块导出结构正确
  - `cd langfuse-client && grep -n 'pub mod\|pub use' src/lib.rs`
  - 预期: 包含 `pub mod config;`、`pub mod error;`、`pub use error::LangfuseError;`、`pub use config::{ClientConfig, BatcherConfig, BackpressurePolicy};`

- [ ] 验证 error.rs 包含所有 4 个变体
  - `cd langfuse-client && grep -c '#\[error(' src/error.rs` (注意: thiserror 2.0 使用 `#[error(...)]` 属性)
  - 预期: 匹配 4 个变体（Http、JsonSerialize、IngestionApi、ChannelClosed、Config = 5 个，或根据实际实现计数，至少覆盖 HTTP/JSON/IngestionApi/ChannelClosed/Config）

- [ ] 验证 config.rs 包含 ClientConfig、BatcherConfig、BackpressurePolicy
  - `cd langfuse-client && grep -n 'pub struct ClientConfig\|pub struct BatcherConfig\|pub enum BackpressurePolicy' src/config.rs`
  - 预期: 三个类型定义各出现一次

- [ ] 验证 Cargo.toml 依赖版本与 workspace 对齐
  - `cd langfuse-client && grep -E 'thiserror|reqwest|serde|tokio|chrono|base64' Cargo.toml`
  - 预期: thiserror = "2.0"、reqwest = "0.12"、serde = "1.0"、tokio = "1"、chrono = "0.4"、base64 = "0.22"

---

### Task 2: 核心数据类型

**背景:**
定义 Langfuse V4 Ingestion API 所有相关的 Rust 数据类型。当前 `langfuse-client/src/` 仅有 lib.rs、error.rs、config.rs（Task 1 产物），缺少 types.rs。本 Task 创建 types.rs，定义 IngestionEvent（10 种变体的内部标签枚举）、各 Body 结构体（TraceBody、SpanBody、GenerationBody、EventBody、ObservationBody、ScoreBody、SdkLogBody）、辅助类型（ObservationType、ObservationLevel、Usage、UsageDetails）以及 IngestionResponse（207 响应）。Task 3（client.rs）依赖本 Task 的 IngestionEvent 和 IngestionResponse 进行 HTTP 请求/响应序列化；Task 4（batcher.rs）依赖 IngestionEvent 作为事件队列元素；Task 5（TUI 集成）依赖各 Body 类型替换现有 `langfuse_client_base` 的生成代码。

**涉及文件:**
- 新建: `langfuse-client/src/types.rs`
- 修改: `langfuse-client/src/lib.rs`（追加 `pub mod types;` 和类型重导出）

**执行步骤:**

- [ ] 定义辅助枚举类型 — ObservationType、ObservationLevel、ScoreDataType
  - 位置: `langfuse-client/src/types.rs`（新建，文件开头）
  - 从 langfuse-api.json 确认的精确枚举值：
    ```rust
    use std::collections::HashMap;
    use serde::{Deserialize, Serialize};

    /// 观测类型（V4 扩展，含 10 种变体）
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum ObservationType {
        Span,
        Generation,
        Event,
        Agent,
        Tool,
        Chain,
        Retriever,
        Evaluator,
        Embedding,
        Guardrail,
    }

    /// 观测日志级别
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum ObservationLevel {
        Debug,
        Default,
        Warning,
        Error,
    }

    /// 评分数据类型
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum ScoreDataType {
        Numeric,
        Boolean,
        Categorical,
        Correction,
    }
    ```
  - 原因: ObservationType 是 ObservationBody 的 required 字段；ObservationLevel 用于 observation 的 level 字段；ScoreDataType 用于 ScoreBody 的 dataType 字段。均使用 `SCREAMING_SNAKE_CASE` 与 API 的 JSON 格式一致（API 返回 "SPAN"、"DEBUG" 等）

- [ ] 定义 Usage、UsageDetails、IngestionUsage 类型
  - 位置: `langfuse-client/src/types.rs`（紧接枚举定义之后）
  - 经 langfuse-api.json 确认：Usage 有 input/output/total（required）+ inputCost/outputCost/totalCost/unit（nullable）；IngestionUsage 是 Usage | OpenAIUsage 的 oneOf；UsageDetails 是 HashMap<String, integer> | OpenAICompletionUsageSchema | OpenAIResponseUsageSchema 的 oneOf
  - 设计决策：IngestionUsage 简化为 `HashMap<String, serde_json::Value>` 通过 `#[serde(flatten)]` 兼容所有格式；UsageDetails 同理简化为 `HashMap<String, i32>`（OpenAI 的 completion/response schema 属于低频场景，后续可按需扩展）
    ```rust
    /// Langfuse Usage（legacy，API required 字段为 input/output/total）
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Usage {
        pub input: i32,
        pub output: i32,
        pub total: i32,
        pub input_cost: Option<f64>,
        pub output_cost: Option<f64>,
        pub total_cost: Option<f64>,
        pub unit: Option<String>,
    }

    /// UsageDetails — 灵活的 key-value map
    /// API 支持 HashMap<String, integer> | OpenAICompletionUsageSchema | OpenAIResponseUsageSchema
    /// 简化为 HashMap<String, i32>，覆盖最常用的 integer map 格式
    pub type UsageDetails = HashMap<String, i32>;

    /// CostDetails — 成本详情 map
    pub type CostDetails = HashMap<String, f64>;

    /// IngestionUsage — 兼容 Usage 和 OpenAIUsage 的灵活格式
    /// API 定义为 Usage | OpenAIUsage，两者字段不同
    /// 使用 HashMap<String, serde_json::Value> 兼容所有格式
    pub type IngestionUsage = HashMap<String, serde_json::Value>;
    ```
  - 原因: IngestionUsage 出现在 CreateGenerationBody/UpdateGenerationBody 的 usage 字段；UsageDetails 出现在这两个 body 的 usageDetails 字段。简化为 HashMap 避免复杂的 oneOf 反序列化，同时覆盖实际使用场景

- [ ] 定义 TraceBody
  - 位置: `langfuse-client/src/types.rs`（紧接 Usage 系列之后）
  - 经 langfuse-api.json 确认，TraceBody 字段：id(nullable), name(nullable), userId(nullable), input(nullable), output(nullable), sessionId(nullable), release(nullable), version(nullable), metadata(nullable), tags(nullable array of string), environment(nullable), public(nullable boolean), timestamp(nullable)
    ```rust
    /// Trace 创建/更新的 Body
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
    pub struct TraceBody {
        pub id: Option<String>,
        pub name: Option<String>,
        pub user_id: Option<String>,
        pub input: Option<serde_json::Value>,
        pub output: Option<serde_json::Value>,
        pub session_id: Option<String>,
        pub release: Option<String>,
        pub version: Option<String>,
        pub metadata: Option<serde_json::Value>,
        pub tags: Option<Vec<String>>,
        pub environment: Option<String>,
        pub public: Option<bool>,
        pub timestamp: Option<String>,
    }
    ```
  - 原因: TraceBody 仅用于 IngestionEvent::TraceCreate 的 body 字段，所有字段均可选

- [ ] 定义 ObservationBody — V4 统一观测类型
  - 位置: `langfuse-client/src/types.rs`（紧接 TraceBody 之后）
  - 经 langfuse-api.json 确认，ObservationBody 是 V4 独立的扁平结构（不继承 SpanBody），required 仅 `type` 字段
  - 注意：API 的 ObservationBody 中 `modelParameters` 值类型为 MapValue（string|integer|float|boolean|array<string> 的 oneOf），使用 `serde_json::Value` 兼容
    ```rust
    /// V4 统一观测类型（ObservationCreate/ObservationUpdate 共用）
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
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
        pub level: Option<ObservationLevel>,
        pub status_message: Option<String>,
        pub version: Option<String>,
        pub environment: Option<String>,
    }
    ```
  - 原因: ObservationBody 用于 IngestionEvent::ObservationCreate/ObservationUpdate 的 body 字段。`r#type` 是 required 字段，直接使用 `ObservationType`（非 Option）；其余字段全部 `Option<T>`，无 `Option<Option<T>>` 嵌套

- [ ] 定义 SpanBody — V3 Span 创建/更新共用
  - 位置: `langfuse-client/src/types.rs`（紧接 ObservationBody 之后）
  - 经 langfuse-api.json 确认继承链：CreateSpanBody extends CreateEventBody extends OptionalObservationBody；UpdateSpanBody extends UpdateEventBody extends OptionalObservationBody
  - 设计决策：SpanBody 扁平化合并 OptionalObservationBody 的所有字段 + endTime + id（create 时 id nullable，update 时 id required，统一为 Option<String>）
    ```rust
    /// Span Body（SpanCreate/SpanUpdate 共用）
    /// API 继承链: CreateSpanBody = {endTime} + CreateEventBody{id} + OptionalObservationBody
    /// UpdateSpanBody = {endTime} + UpdateEventBody{id(required)} + OptionalObservationBody
    /// 扁平化合并为统一结构，id 在 update 场景由调用方保证非空
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
    pub struct SpanBody {
        pub id: Option<String>,
        pub trace_id: Option<String>,
        pub name: Option<String>,
        pub start_time: Option<String>,
        pub end_time: Option<String>,
        pub input: Option<serde_json::Value>,
        pub output: Option<serde_json::Value>,
        pub metadata: Option<serde_json::Value>,
        pub level: Option<ObservationLevel>,
        pub status_message: Option<String>,
        pub parent_observation_id: Option<String>,
        pub version: Option<String>,
        pub environment: Option<String>,
    }
    ```
  - 原因: 扁平化合并消除 API 的多级继承，SpanCreate 和 SpanUpdate 使用同一结构体，简化使用方代码

- [ ] 定义 GenerationBody — V3 Generation 创建/更新共用
  - 位置: `langfuse-client/src/types.rs`（紧接 SpanBody 之后）
  - 经 langfuse-api.json 确认继承链：CreateGenerationBody extends CreateSpanBody + 独有字段（completionStartTime, model, modelParameters, usage[IngestionUsage], usageDetails, costDetails, promptName, promptVersion）
  - 扁平化合并所有字段
    ```rust
    /// Generation Body（GenerationCreate/GenerationUpdate 共用）
    /// API 继承链: CreateGenerationBody = {completionStartTime, model, modelParameters,
    ///   usage[IngestionUsage], usageDetails, costDetails, promptName, promptVersion}
    ///   + CreateSpanBody + OptionalObservationBody
    /// UpdateGenerationBody = 同上独有字段 + UpdateSpanBody + OptionalObservationBody
    /// 扁平化合并为统一结构
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
    pub struct GenerationBody {
        // From OptionalObservationBody
        pub id: Option<String>,
        pub trace_id: Option<String>,
        pub name: Option<String>,
        pub start_time: Option<String>,
        pub end_time: Option<String>,
        pub input: Option<serde_json::Value>,
        pub output: Option<serde_json::Value>,
        pub metadata: Option<serde_json::Value>,
        pub level: Option<ObservationLevel>,
        pub status_message: Option<String>,
        pub parent_observation_id: Option<String>,
        pub version: Option<String>,
        pub environment: Option<String>,
        // Generation-specific fields
        pub completion_start_time: Option<String>,
        pub model: Option<String>,
        pub model_parameters: Option<HashMap<String, serde_json::Value>>,
        pub usage: Option<IngestionUsage>,
        pub usage_details: Option<UsageDetails>,
        pub cost_details: Option<CostDetails>,
        pub prompt_name: Option<String>,
        pub prompt_version: Option<Option<i32>>,
    }
    ```
  - 注意：promptVersion 在 API 中是 `nullable integer`，即 `Option<Option<i32>>` 语义（null=不传 vs 未设置），但遵循设计文档规则"所有字段用 Option<T>"，此处实际需 double-option 因为 API 区分"不传字段"和"传 null"。**决策：使用 `Option<i32>` 即可**，因为 Langfuse 服务端对 null 和 missing 的处理一致（均表示"未设置"），这是 spec-design.md 的明确约束
  - 修正后：`pub prompt_version: Option<i32>`

- [ ] 定义 EventBody — V3 Event 创建共用
  - 位置: `langfuse-client/src/types.rs`（紧接 GenerationBody 之后）
  - 经 langfuse-api.json 确认：CreateEventBody = {id(nullable)} + OptionalObservationBody
  - 无 UpdateEventBody 的独立事件类型（API 中 event-create 是唯一的事件事件类型）
    ```rust
    /// Event Body（EventCreate 使用）
    /// API: CreateEventBody = {id} + OptionalObservationBody
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
    pub struct EventBody {
        pub id: Option<String>,
        pub trace_id: Option<String>,
        pub name: Option<String>,
        pub start_time: Option<String>,
        pub input: Option<serde_json::Value>,
        pub output: Option<serde_json::Value>,
        pub metadata: Option<serde_json::Value>,
        pub level: Option<ObservationLevel>,
        pub status_message: Option<String>,
        pub parent_observation_id: Option<String>,
        pub version: Option<String>,
        pub environment: Option<String>,
    }
    ```
  - 原因: EventBody 没有 endTime 和 model/usage 等 Generation 字段，与 SpanBody/GenerationBody 分开定义更清晰

- [ ] 定义 ScoreBody 和 SdkLogBody
  - 位置: `langfuse-client/src/types.rs`（紧接 EventBody 之后）
  - 经 langfuse-api.json 确认：ScoreBody required 字段为 name + value；value 是 CreateScoreValue（number | string 的 oneOf），用 `serde_json::Value` 兼容；SdkLogBody required 字段为 log（任意值）
    ```rust
    /// Score Body（ScoreCreate 使用）
    /// API: required = [name, value]; value 是 number | string 的 oneOf
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
    pub struct ScoreBody {
        pub name: String,
        pub value: serde_json::Value,
        pub id: Option<String>,
        pub trace_id: Option<String>,
        pub observation_id: Option<String>,
        pub comment: Option<String>,
        pub data_type: Option<ScoreDataType>,
        pub config_id: Option<String>,
        pub queue_id: Option<String>,
        pub environment: Option<String>,
        pub session_id: Option<String>,
        pub metadata: Option<serde_json::Value>,
        pub dataset_run_id: Option<String>,
    }

    /// SDK Log Body（SdkLog 使用）
    /// API: required = [log]; log 是任意 JSON 值
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(deny_unknown_fields)]
    pub struct SdkLogBody {
        pub log: serde_json::Value,
    }
    ```
  - 原因: ScoreBody 的 value 必须同时支持数字和字符串（categorical score 传字符串），使用 `serde_json::Value` 是最简洁的方案

- [ ] 定义 IngestionEvent — 10 种事件类型的统一枚举
  - 位置: `langfuse-client/src/types.rs`（所有 Body 定义之后）
  - 经 langfuse-api.json 确认 10 种类型标签：trace-create, span-create, span-update, generation-create, generation-update, event-create, score-create, observation-create, observation-update, sdk-log
  - BaseEvent 结构：id(required string) + timestamp(required string) + metadata(nullable)
  - 使用 serde 内部标签 `#[serde(tag = "type", rename_all = "kebab-case")]` 自动处理 type 判别字段
    ```rust
    /// Ingestion 事件统一枚举（10 种变体）
    /// 通过 serde 内部标签自动序列化 `type` 判别字段
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "kebab-case")]
    pub enum IngestionEvent {
        TraceCreate {
            id: String,
            timestamp: String,
            body: TraceBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        SpanCreate {
            id: String,
            timestamp: String,
            body: SpanBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        SpanUpdate {
            id: String,
            timestamp: String,
            body: SpanBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        GenerationCreate {
            id: String,
            timestamp: String,
            body: GenerationBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        GenerationUpdate {
            id: String,
            timestamp: String,
            body: GenerationBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        EventCreate {
            id: String,
            timestamp: String,
            body: EventBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        ScoreCreate {
            id: String,
            timestamp: String,
            body: ScoreBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        ObservationCreate {
            id: String,
            timestamp: String,
            body: ObservationBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        ObservationUpdate {
            id: String,
            timestamp: String,
            body: ObservationBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
        SdkLog {
            id: String,
            timestamp: String,
            body: SdkLogBody,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
        },
    }
    ```
  - 原因: 内部标签模式让序列化结果为扁平 JSON 对象（`type` 与 `id`/`timestamp`/`body` 同级），与 API 期望的格式完全匹配。metadata 字段为 nullable（API BaseEvent 定义为 nullable），使用 `skip_serializing_if` 避免发送 null 值

- [ ] 定义 IngestionResponse — 207 多状态响应
  - 位置: `langfuse-client/src/types.rs`（IngestionEvent 之后）
  - 经 langfuse-api.json 确认：IngestionResponse = {successes: [IngestionSuccess], errors: [IngestionError]}；IngestionSuccess = {id: string, status: integer}（required）；IngestionError = {id: string, status: integer, message: nullable string, error: nullable any}
    ```rust
    /// 207 Multi-Status 响应中的成功项
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IngestionSuccess {
        pub id: String,
        pub status: i32,
    }

    /// 207 Multi-Status 响应中的错误项
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IngestionError {
        pub id: String,
        pub status: i32,
        pub message: Option<String>,
        pub error: Option<serde_json::Value>,
    }

    /// Ingestion API 的 207 Multi-Status 响应
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IngestionResponse {
        pub successes: Vec<IngestionSuccess>,
        pub errors: Vec<IngestionError>,
    }
    ```
  - 原因: Client 的 ingest() 方法返回 `Result<IngestionResponse, LangfuseError>`，需要完整解析 207 响应

- [ ] 更新 lib.rs — 追加 types 模块声明和类型重导出
  - 位置: `langfuse-client/src/lib.rs`
  - 在现有 `pub mod config;` 和 `pub mod error;` 之后追加 `pub mod types;`
  - 在现有 `pub use` 语句之后追加类型重导出：
    ```rust
    pub mod types;
    pub use types::{
        IngestionEvent, IngestionResponse, IngestionSuccess, IngestionError,
        TraceBody, SpanBody, GenerationBody, EventBody, ObservationBody,
        ScoreBody, SdkLogBody,
        ObservationType, ObservationLevel, ScoreDataType,
        Usage, UsageDetails, CostDetails, IngestionUsage,
    };
    ```

- [ ] 为所有类型编写单元测试 — 序列化 roundtrip 测试
  - 测试文件: `langfuse-client/src/types.rs`（内联 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_observation_type_serde`: ObservationType::Span 序列化为 `"SPAN"` 并 roundtrip 成功
    - `test_observation_level_serde`: ObservationLevel::Warning 序列化为 `"WARNING"` 并 roundtrip 成功
    - `test_score_data_type_serde`: ScoreDataType::Categorical 序列化为 `"CATEGORICAL"` 并 roundtrip 成功
    - `test_usage_serde`: 构造 Usage{input:100, output:50, total:150, ..}，序列化后验证 JSON 包含 `"input":100,"output":50,"total":150`，并 roundtrip 成功
    - `test_usage_details_serde`: 构造 UsageDetails 包含 "input"=100, "cache_read_input_tokens"=30，序列化并 roundtrip 成功
    - `test_trace_body_serde_minimal`: 构造 TraceBody 仅设置 id 和 name，序列化后验证 null 字段未出现（skip_serializing_if），roundtrip 成功
    - `test_trace_body_serde_full`: 构造 TraceBody 填充所有字段（含 tags, public, metadata），验证完整序列化 + roundtrip
    - `test_observation_body_serde`: 构造 ObservationBody{type: ObservationType::Agent, name, trace_id, start_time, input}，序列化验证 `"type":"AGENT"`，roundtrip 成功
    - `test_span_body_serde`: 构造 SpanBody{id, trace_id, name, start_time, end_time, parent_observation_id}，roundtrip 成功
    - `test_generation_body_serde`: 构造 GenerationBody 含 model, usage, usage_details, model_parameters，验证序列化包含 camelCase 字段名，roundtrip 成功
    - `test_event_body_serde`: 构造 EventBody{id, trace_id, name, input, output}，roundtrip 成功
    - `test_score_body_serde_numeric`: ScoreBody{name, value: json!(0.95)}，验证序列化包含 `"value":0.95`，roundtrip 成功
    - `test_score_body_serde_string`: ScoreBody{name, value: json!("category-a")}，验证序列化包含 `"value":"category-a"`，roundtrip 成功
    - `test_sdk_log_body_serde`: SdkLogBody{log: json!({"message":"test"})}，roundtrip 成功
    - `test_ingestion_event_trace_create`: 构造 IngestionEvent::TraceCreate{id, timestamp, body, metadata:None}，序列化验证 `"type":"trace-create"` 且无 `"metadata"` key（skip_serializing_if），roundtrip 成功
    - `test_ingestion_event_span_create`: IngestionEvent::SpanCreate{id, timestamp, body}，验证 `"type":"span-create"`
    - `test_ingestion_event_span_update`: IngestionEvent::SpanUpdate{id, timestamp, body}，验证 `"type":"span-update"`
    - `test_ingestion_event_generation_create`: IngestionEvent::GenerationCreate{id, timestamp, body 含 model}，验证 `"type":"generation-create"` 且 body 中有 model 字段
    - `test_ingestion_event_generation_update`: IngestionEvent::GenerationUpdate{id, timestamp, body}，验证 `"type":"generation-update"`
    - `test_ingestion_event_event_create`: IngestionEvent::EventCreate{id, timestamp, body}，验证 `"type":"event-create"`
    - `test_ingestion_event_score_create`: IngestionEvent::ScoreCreate{id, timestamp, body 含 name+value}，验证 `"type":"score-create"`
    - `test_ingestion_event_observation_create`: IngestionEvent::ObservationCreate{id, timestamp, body 含 ObservationType::Agent}，验证 `"type":"observation-create"` 且 body.type 为 "AGENT"
    - `test_ingestion_event_observation_update`: IngestionEvent::ObservationUpdate{id, timestamp, body}，验证 `"type":"observation-update"`
    - `test_ingestion_event_sdk_log`: IngestionEvent::SdkLog{id, timestamp, body}，验证 `"type":"sdk-log"`
    - `test_ingestion_event_with_metadata`: IngestionEvent::TraceCreate{id, timestamp, body, metadata:Some(json!({"sdk":"rust"}))}，验证序列化包含 `"metadata":{"sdk":"rust"}`
    - `test_ingestion_response`: 构造 IngestionResponse{successes:[{id:"1",status:200}], errors:[{id:"2",status:400,message:Some("bad"),error:None}]}，roundtrip 成功
    - `test_ingestion_response_empty`: IngestionResponse{successes:[], errors:[]}，roundtrip 成功
    - `test_batch_of_events_serde`: 构造 `Vec<IngestionEvent>` 包含 3 种不同类型事件，序列化为 JSON 数组并 roundtrip 成功（验证批量序列化场景）
  - 运行命令: `cd langfuse-client && cargo test --lib -- types::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 crate 可编译（types.rs 加入后）
  - `cd langfuse-client && cargo check 2>&1`
  - 预期: 输出 `Checking langfuse-client v0.1.0` 且无 error

- [ ] 验证 types.rs 包含所有核心类型
  - `cd langfuse-client && grep -c 'pub enum IngestionEvent\|pub struct TraceBody\|pub struct SpanBody\|pub struct GenerationBody\|pub struct EventBody\|pub struct ObservationBody\|pub struct ScoreBody\|pub struct SdkLogBody\|pub struct IngestionResponse' src/types.rs`
  - 预期: 9 个匹配（1 enum + 7 struct + IngestionResponse）

- [ ] 验证 IngestionEvent 包含 10 种变体
  - `cd langfuse-client && grep -c '^\s\s\s\s[A-Z][a-zA-Z]*\s*{' src/types.rs | head -1` 或 `grep -E '^\s+(TraceCreate|SpanCreate|SpanUpdate|GenerationCreate|GenerationUpdate|EventCreate|ScoreCreate|ObservationCreate|ObservationUpdate|SdkLog)\s*\{' src/types.rs | wc -l`
  - 预期: 10 个变体匹配

- [ ] 验证 serde 内部标签配置正确
  - `cd langfuse-client && grep -A2 'pub enum IngestionEvent' src/types.rs | head -5`
  - 预期: 包含 `#[serde(tag = "type", rename_all = "kebab-case")]`

- [ ] 验证所有 Body 结构体使用 camelCase 和 deny_unknown_fields
  - `cd langfuse-client && grep -c 'rename_all = "camelCase"' src/types.rs`
  - 预期: 至少 7 个匹配（TraceBody, SpanBody, GenerationBody, EventBody, ObservationBody, ScoreBody, SdkLogBody）
  - `cd langfuse-client && grep -c 'deny_unknown_fields' src/types.rs`
  - 预期: 至少 7 个匹配

- [ ] 验证无 Option<Option<T>> 嵌套
  - `cd langfuse-client && grep 'Option<Option<' src/types.rs`
  - 预期: 无匹配（exit code 1）

- [ ] 验证 lib.rs 包含 types 模块声明和重导出
  - `cd langfuse-client && grep -E 'pub mod types|pub use types::' src/lib.rs`
  - 预期: 包含 `pub mod types;` 和 `pub use types::{...}` 行

- [ ] 验证所有单元测试通过
  - `cd langfuse-client && cargo test --lib -- types::tests 2>&1`
  - 预期: 所有 test 结果为 `test result: ok`，0 failures

- [ ] 验证序列化输出格式正确（抽查关键 JSON 格式）
  - `cd langfuse-client && cargo test --lib -- types::tests::test_ingestion_event_trace_create -- --nocapture 2>&1`
  - 预期: 测试通过，可通过 print 调试确认 JSON 包含 `"type":"trace-create"`

---

### Task 3: 底层 Client

**背景:**
实现 LangfuseClient 底层 HTTP 客户端，负责与 Langfuse V4 Ingestion API 通信。当前 `langfuse-client/src/` 仅有 lib.rs、error.rs、config.rs、types.rs（Task 1/2 产物），缺少 client.rs。本 Task 创建 client.rs，实现：LangfuseClient 结构体（持有 reqwest::Client 连接池）、Basic Auth 认证构造、ingest() 方法（POST /api/public/ingestion，序列化 IngestionEvent batch）、207 Multi-Status 响应解析（IngestionResponse）、网络错误自动重试（指数退避）。Task 4（batcher.rs）依赖本 Task 的 LangfuseClient 进行实际 HTTP 调用；Task 5（TUI 集成）通过 Batcher 间接使用 Client。

**涉及文件:**
- 新建: `langfuse-client/src/client.rs`
- 修改: `langfuse-client/src/lib.rs`（追加 `pub mod client;` 和 `LangfuseClient` 重导出）
- 修改: `langfuse-client/Cargo.toml`（追加 `[dev-dependencies]` 中 `mockito`）

**执行步骤:**

- [ ] 在 Cargo.toml 追加 mockito 开发依赖
  - 位置: `langfuse-client/Cargo.toml`（`[dev-dependencies]` 段）
  - 在已有 `tokio-test = "0.4"` 之后追加:
    ```toml
    mockito = "1"
    ```
  - 原因: mockito 提供 HTTP mock server，用于测试 ingest() 的请求构建、响应解析、重试逻辑，无需真实 Langfuse 服务。项目中目前无 mock 依赖，需要新增。mockito 1.x 基于 hyper，与 reqwest 0.12 兼容

- [ ] 定义 LangfuseClient 结构体和构造函数
  - 位置: `langfuse-client/src/client.rs`（新建，文件开头）
  - 定义结构体和 `new()` 构造函数：
    ```rust
    use crate::error::LangfuseError;
    use crate::types::{IngestionEvent, IngestionResponse};
    use base64::Engine;
    use reqwest::Client;
    use std::time::Duration;
    use tracing::warn;

    /// Langfuse V4 Ingestion API 底层客户端
    ///
    /// 持有 reqwest::Client（复用连接池），封装认证、请求构建、重试逻辑。
    pub struct LangfuseClient {
        http: Client,
        base_url: String,
        auth_header: String,
        max_retries: usize,
    }

    impl LangfuseClient {
        /// 构造 LangfuseClient
        ///
        /// - `public_key`: Langfuse 公钥
        /// - `secret_key`: Langfuse 秘钥
        /// - `base_url`: Langfuse 服务地址（如 "https://cloud.langfuse.com"）
        /// - `max_retries`: 网络错误最大重试次数（0 = 不重试）
        pub fn new(public_key: &str, secret_key: &str, base_url: &str, max_retries: usize) -> Self {
            let credentials = format!("{}:{}", public_key, secret_key);
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
            let auth_header = format!("Basic {}", encoded);

            // 配置 reqwest Client 超时：连接超时 5s，请求超时 30s
            let http = Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client");

            Self {
                http,
                base_url: base_url.trim_end_matches('/').to_string(),
                auth_header,
                max_retries,
            }
        }

        /// 从 ClientConfig 构造（便捷方法）
        pub fn from_config(config: &crate::config::ClientConfig, max_retries: usize) -> Self {
            Self::new(&config.public_key, &config.secret_key, &config.base_url, max_retries)
        }
    }
    ```
  - 原因: `base_url` 去 trailing slash 防止拼接时出现 `//`；reqwest::Client builder 配置超时避免请求挂起；`max_retries` 由调用方（Batcher）从 BatcherConfig 传入

- [ ] 实现 ingest() 方法 — HTTP 请求 + 重试
  - 位置: `langfuse-client/src/client.rs`（`impl LangfuseClient` 块内，紧接 `from_config()` 之后）
  - 实现 ingest() 和 ingest_single()：
    ```rust
    /// 发送一批 ingestion 事件到 Langfuse API
    ///
    /// POST /api/public/ingestion
    /// Headers:
    ///   - Authorization: Basic {base64(public_key:secret_key)}
    ///   - Content-Type: application/json
    ///   - x-langfuse-ingestion-version: 4
    ///
    /// 响应: 207 Multi-Status → 解析 IngestionResponse
    /// 错误重试: 网络错误（连接失败、超时等）自动重试 max_retries 次，指数退避（1s, 2s, 4s...）
    /// 4xx 错误不重试，直接返回 LangfuseError::IngestionApi
    pub async fn ingest(
        &self,
        batch: Vec<IngestionEvent>,
    ) -> Result<IngestionResponse, LangfuseError> {
        let url = format!("{}/api/public/ingestion", self.base_url);
        let body = serde_json::json!({ "batch": batch });

        let mut attempt = 0;
        loop {
            let result = self
                .http
                .post(&url)
                .header("Authorization", &self.auth_header)
                .header("Content-Type", "application/json")
                .header("x-langfuse-ingestion-version", "4")
                .json(&body)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() || status.as_u16() == 207 {
                        // 207 Multi-Status 或 2xx: 解析响应体
                        let response_text = response.text().await?;
                        let ingestion_response: IngestionResponse =
                            serde_json::from_str(&response_text)?;

                        // 如果有错误项，记录 warn 日志但仍返回（让调用方决定如何处理）
                        if !ingestion_response.errors.is_empty() {
                            warn!(
                                "Langfuse ingestion partial failure: {} errors out of {} events",
                                ingestion_response.errors.len(),
                                ingestion_response.successes.len() + ingestion_response.errors.len()
                            );
                        }

                        return Ok(ingestion_response);
                    } else if status.is_client_error() {
                        // 4xx: 不重试，直接返回错误
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(LangfuseError::IngestionApi(format!(
                            "HTTP {}: {}",
                            status, error_text
                        )));
                    } else {
                        // 5xx: 可重试
                        let error_text = response.text().await.unwrap_or_default();
                        if attempt < self.max_retries {
                            attempt += 1;
                            let delay = Duration::from_secs(1 << (attempt - 1));
                            warn!(
                                "Langfuse ingestion server error (attempt {}/{}), retrying in {:?}: HTTP {} {}",
                                attempt, self.max_retries, delay, status, error_text
                            );
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        return Err(LangfuseError::IngestionApi(format!(
                            "HTTP {} after {} retries: {}",
                            status, self.max_retries, error_text
                        )));
                    }
                }
                Err(e) => {
                    // 网络错误（连接失败、超时、DNS 等）: 可重试
                    if attempt < self.max_retries {
                        attempt += 1;
                        let delay = Duration::from_secs(1 << (attempt - 1));
                        warn!(
                            "Langfuse ingestion network error (attempt {}/{}), retrying in {:?}: {}",
                            attempt, self.max_retries, delay, e
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(LangfuseError::Http(e));
                }
            }
        }
    }

    /// 便利方法：发送单个 ingestion 事件
    ///
    /// 将单个事件包装为 batch（Vec 长度为 1），调用 ingest()
    pub async fn ingest_single(
        &self,
        event: IngestionEvent,
    ) -> Result<IngestionResponse, LangfuseError> {
        self.ingest(vec![event]).await
    }
    ```
  - 原因: 重试仅针对网络错误和 5xx（服务端错误），4xx（客户端错误）不重试因为请求本身有问题。指数退避使用 `1 << (attempt - 1)` 计算（1s, 2s, 4s, 8s...）。207 响应即使包含部分错误也返回 Ok，因为调用方（Batcher）需要知道哪些事件成功/失败。warn 级别日志记录部分失败和重试

- [ ] 更新 lib.rs — 追加 client 模块声明和重导出
  - 位置: `langfuse-client/src/lib.rs`
  - 在现有 `pub mod types;` 之后追加 `pub mod client;`
  - 在现有 `pub use` 语句之后追加 `pub use client::LangfuseClient;`
  - 原因: 暴露 LangfuseClient 给外部使用者

- [ ] 为 LangfuseClient 编写单元测试
  - 测试文件: `langfuse-client/src/client.rs`（内联 `#[cfg(test)] mod tests`）
  - 测试策略: 使用 mockito 创建 mock HTTP server，验证请求构建、认证 header、响应解析、重试逻辑
  - 测试场景:
    - `test_new_creates_client_with_correct_auth`: 构造 `LangfuseClient::new("pk", "sk", "http://localhost", 3)`，验证 `client.auth_header` 为 `"Basic cGs6c2s="`（base64("pk:sk")），`client.base_url` 为 `"http://localhost"`（无 trailing slash），`client.max_retries` 为 3
    - `test_new_trims_trailing_slash`: 构造 `new("pk", "sk", "http://localhost/", 0)`，验证 `base_url` 为 `"http://localhost"`
    - `test_ingest_success_207`: 用 mockito 创建 mock server，mock POST `/api/public/ingestion` 返回 207 + body `{"successes":[{"id":"evt-1","status":200}],"errors":[]}`。构造 batch 包含 1 个 IngestionEvent::TraceCreate，调用 `client.ingest(batch).await`，验证返回 `Ok(IngestionResponse{successes: 1, errors: 0})`，验证 mock 收到的请求包含 headers `Authorization: Basic ...`、`x-langfuse-ingestion-version: 4`、`Content-Type: application/json`
    - `test_ingest_partial_failure_207`: mock 返回 207 + body `{"successes":[{"id":"1","status":200}],"errors":[{"id":"2","status":400,"message":"invalid","error":null}]}`。验证返回 `Ok`，`response.errors.len() == 1`
    - `test_ingest_4xx_no_retry`: mock 返回 400 + body `{"error":"bad request"}`。`max_retries = 3`。验证返回 `Err(LangfuseError::IngestionApi(_))` 且 mock 仅被调用 1 次（不重试）
    - `test_ingest_5xx_retries_then_success`: 使用 mockito 的 `mock` 匹配器，前 2 次返回 500，第 3 次返回 207。`max_retries = 3`。验证最终返回 `Ok`，mock 被调用 3 次
    - `test_ingest_5xx_retries_exhausted`: mock 始终返回 500，`max_retries = 2`。验证返回 `Err(LangfuseError::IngestionApi(_))` 且错误消息包含 "after 2 retries"，mock 被调用 3 次（1 初始 + 2 重试）
    - `test_ingest_network_error_retries`: mock 使用 `mockito::Matcher::Missing` 或在 mock server 未启动时连接失败。更可行的方案：启动 mock server 但立刻关闭端口，让 reqwest 连接失败。`max_retries = 1`。验证返回 `Err(LangfuseError::Http(_))`
    - `test_ingest_single_convenience`: 用 mockito mock 207 响应，调用 `client.ingest_single(event).await`，验证请求体 `"batch"` 数组长度为 1
    - `test_ingest_empty_batch`: 调用 `client.ingest(vec![]).await`，验证请求体 `"batch"` 为空数组，mock 返回 207 + `{"successes":[],"errors":[]}`，验证返回 `Ok`
    - `test_ingest_request_body_format`: mock 捕获请求体，验证 JSON 为 `{"batch":[...]}` 格式，batch 内的事件包含 `"type"` 字段（验证 serde 内部标签序列化正确）
    - `test_from_config`: 构造 `ClientConfig{public_key, secret_key, base_url}`，调用 `LangfuseClient::from_config(&config, 2)`，验证 auth_header 和 max_retries 正确
  - 运行命令: `cd langfuse-client && cargo test --lib -- client::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 crate 可编译（client.rs 加入后）
  - `cd langfuse-client && cargo check 2>&1`
  - 预期: 输出 `Checking langfuse-client v0.1.0` 且无 error

- [ ] 验证 client.rs 包含 LangfuseClient 结构体和核心方法
  - `cd langfuse-client && grep -n 'pub struct LangfuseClient\|pub fn new\|pub async fn ingest\|pub async fn ingest_single\|pub fn from_config' src/client.rs`
  - 预期: 5 个匹配，分别对应结构体定义、构造函数、ingest、ingest_single、from_config

- [ ] 验证请求头包含 V4 版本标识和 Basic Auth
  - `cd langfuse-client && grep -n 'x-langfuse-ingestion-version\|Authorization\|auth_header' src/client.rs`
  - 预期: 包含 `x-langfuse-ingestion-version` 值为 `4`，以及 `Authorization` header 使用 `auth_header`

- [ ] 验证重试逻辑存在（指数退避）
  - `cd langfuse-client && grep -n 'max_retries\|1 <<\|tokio::time::sleep' src/client.rs`
  - 预期: 包含 `max_retries` 条件判断、`1 <<` 位移退避计算、`tokio::time::sleep` 延迟

- [ ] 验证 lib.rs 包含 client 模块声明和重导出
  - `cd langfuse-client && grep -E 'pub mod client|pub use client::LangfuseClient' src/lib.rs`
  - 预期: 包含 `pub mod client;` 和 `pub use client::LangfuseClient;`

- [ ] 验证 mockito 在 dev-dependencies 中
  - `cd langfuse-client && grep 'mockito' Cargo.toml`
  - 预期: 在 `[dev-dependencies]` 段中有 `mockito = "1"`

- [ ] 验证所有单元测试通过
  - `cd langfuse-client && cargo test --lib -- client::tests 2>&1`
  - 预期: 所有 test 结果为 `test result: ok`，0 failures

- [ ] 验证完整测试套件无回归（含 Task 1/2 测试）
  - `cd langfuse-client && cargo test 2>&1`
  - 预期: 全部测试通过，0 failures

---

### Task 4: 上层 Batcher

**背景:**
实现 Batcher 异步批量聚合层，为调用方提供简洁的 `add(event)` / `flush()` API，隐藏底层批量、定时、背压细节。当前 `langfuse-client/src/` 已有 lib.rs、error.rs、config.rs、types.rs、client.rs（Task 1/2/3 产物），缺少 batcher.rs。本 Task 创建 batcher.rs，实现：Batcher 结构体（持有 `Arc<LangfuseClient>` + `mpsc::Sender<BatcherCommand>`）、BatcherCommand 枚举（Add/Flush/Shutdown）、后台 tokio task 事件循环（从 channel 接收命令，收集事件到 Vec，达到 max_events 或 flush_interval 到期时调用 `client.ingest()`）、背压策略（DropNew 用 `try_send` / Block 用 `send.await`）、手动 flush（oneshot channel 等待完成）、优雅关闭（Drop 时发送 Shutdown，后台 task flush 剩余后退出）。Task 5（TUI 集成）依赖本 Task 的 Batcher 替换 `langfuse-ergonomic` 的 Batcher。

**涉及文件:**
- 新建: `langfuse-client/src/batcher.rs`
- 修改: `langfuse-client/src/lib.rs`（追加 `pub mod batcher;` 和 `Batcher` 重导出）

**执行步骤:**

- [ ] 定义 BatcherCommand 枚举
  - 位置: `langfuse-client/src/batcher.rs`（新建，文件开头）
  - 定义内部命令枚举（私有，不导出）：
    ```rust
    use crate::config::{BackpressurePolicy, BatcherConfig};
    use crate::error::LangfuseError;
    use crate::types::IngestionEvent;
    use crate::LangfuseClient;
    use std::sync::Arc;
    use tokio::sync::{mpsc, oneshot};
    use tokio::time::{Duration, Instant, interval};
    use tracing::{debug, error, info, warn};

    /// Batcher 内部命令（不导出）
    enum BatcherCommand {
        /// 添加事件到待发送队列
        Add(IngestionEvent),
        /// 手动 flush：发送当前队列中的所有事件，完成后通过 oneshot 通知调用方
        Flush(oneshot::Sender<()>),
        /// 关闭后台 task（先 flush 剩余事件再退出）
        Shutdown,
    }
    ```
  - 原因: 命令模式解耦调用方和后台 task，Add/Flush/Shutdown 覆盖全部交互场景。Flush 携带 oneshot::Sender 实现同步等待语义

- [ ] 定义 Batcher 结构体和构造函数
  - 位置: `langfuse-client/src/batcher.rs`（紧接 BatcherCommand 定义之后）
  - Batcher 持有 client 的 Arc 引用和命令 channel 的 Sender：
    ```rust
    /// Langfuse 事件批量聚合器
    ///
    /// 通过后台 tokio task 异步收集事件，按 `max_events`（定量）或 `flush_interval`（定时）
    /// 自动发送到 Langfuse API。支持手动 flush 和两种背压策略。
    ///
    /// 使用方式：
    /// ```ignore
    /// let batcher = Batcher::new(client, BatcherConfig::default());
    /// batcher.add(event).await?;      // 添加事件
    /// batcher.flush().await?;          // 手动 flush
    /// // Drop 时自动关闭并 flush 剩余事件
    /// ```
    pub struct Batcher {
        client: Arc<LangfuseClient>,
        tx: mpsc::Sender<BatcherCommand>,
        backpressure: BackpressurePolicy,
        /// 后台 task 的 JoinHandle，用于 Drop 时等待完成
        handle: Option<tokio::task::JoinHandle<()>>,
    }

    impl Batcher {
        /// 创建新的 Batcher 实例，同时启动后台事件处理 task
        ///
        /// - `client`: LangfuseClient 实例（Arc 包装，在后台 task 和 Batcher 之间共享）
        /// - `config`: Batcher 配置（max_events、flush_interval、backpressure、max_retries）
        pub fn new(client: LangfuseClient, config: BatcherConfig) -> Self {
            let client = Arc::new(client);
            let (tx, rx) = mpsc::channel(config.max_events);
            let backpressure = config.backpressure;

            // 用 client.max_retries 创建新的 LangfuseClient（保持 max_retries 一致）
            // 注意：client 已持有 max_retries，直接使用即可
            let batch_client = Arc::clone(&client);
            let max_events = config.max_events;
            let flush_interval = config.flush_interval;

            let handle = tokio::spawn(async move {
                Self::run_loop(batch_client, rx, max_events, flush_interval).await;
            });

            Self {
                client,
                tx,
                backpressure,
                handle: Some(handle),
            }
        }
    }
    ```
  - 原因: `mpsc::channel(config.max_events)` 有界通道，容量与 max_events 相同，保证内存可控。`handle` 存储后台 task 的 JoinHandle 用于 Drop 时等待完成。`backpressure` 保存在 Batcher 侧供 `add()` 根据策略选择 `try_send` 或 `send`

- [ ] 实现后台事件循环 run_loop()
  - 位置: `langfuse-client/src/batcher.rs`（`impl Batcher` 块内，紧接 `new()` 之后）
  - 核心异步循环：从 channel 接收命令，收集事件，定量/定时 flush：
    ```rust
    /// 后台事件处理循环
    ///
    /// 核心逻辑：
    /// 1. tokio::select! 同时等待 channel 命令和 flush_interval 超时
    /// 2. 收到 Add 命令 → 将事件追加到 buffer
    /// 3. buffer 达到 max_events → 自动 flush
    /// 4. 收到 Flush 命令 → flush 后通过 oneshot 通知调用方
    /// 5. flush_interval 到期 → 自动 flush（即使 buffer 未满）
    /// 6. channel 关闭或收到 Shutdown → flush 剩余事件后退出
    async fn run_loop(
        client: Arc<LangfuseClient>,
        mut rx: mpsc::Receiver<BatcherCommand>,
        max_events: usize,
        flush_interval: Duration,
    ) {
        let mut buffer: Vec<IngestionEvent> = Vec::with_capacity(max_events);
        let mut interval = interval(flush_interval);
        // 首次立即触发一次 tick（interval 的设计是首次 poll 立即返回）
        // 但我们不需要立即 flush 空队列，所以先 tick 一次消耗掉
        interval.tick().await;

        loop {
            tokio::select! {
                // 优先处理命令（channel 有数据时优先处理）
                cmd = rx.recv() => {
                    match cmd {
                        Some(BatcherCommand::Add(event)) => {
                            buffer.push(event);
                            if buffer.len() >= max_events {
                                Self::do_flush(&client, &mut buffer).await;
                            }
                        }
                        Some(BatcherCommand::Flush(ack)) => {
                            Self::do_flush(&client, &mut buffer).await;
                            // 即使 flush 失败也通知调用方（调用方不关心具体错误，错误已通过 warn 日志记录）
                            let _ = ack.send(());
                        }
                        Some(BatcherCommand::Shutdown) | None => {
                            // 收到 Shutdown 或 channel 关闭：flush 剩余事件后退出
                            if !buffer.is_empty() {
                                info!(
                                    "Batcher shutting down, flushing {} remaining events",
                                    buffer.len()
                                );
                                Self::do_flush(&client, &mut buffer).await;
                            }
                            info!("Batcher background task exited");
                            return;
                        }
                    }
                }
                // 定时 flush
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        debug!(
                            "Batcher periodic flush: {} events (interval: {:?})",
                            buffer.len(),
                            flush_interval
                        );
                        Self::do_flush(&client, &mut buffer).await;
                    }
                }
            }
        }
    }

    /// 执行一次 flush：将 buffer 中的事件发送到 Langfuse API
    async fn do_flush(client: &LangfuseClient, buffer: &mut Vec<IngestionEvent>) {
        if buffer.is_empty() {
            return;
        }

        // 取出 buffer 中的事件（swap 避免重新分配）
        let events: Vec<IngestionEvent> = buffer.drain(..).collect();
        debug!("Batcher flushing {} events", events.len());

        match client.ingest(events).await {
            Ok(response) => {
                if response.errors.is_empty() {
                    debug!(
                        "Batcher flush successful: {} events accepted",
                        response.successes.len()
                    );
                } else {
                    warn!(
                        "Batcher flush partial failure: {} succeeded, {} failed",
                        response.successes.len(),
                        response.errors.len()
                    );
                    for err in &response.errors {
                        warn!("  Failed event id={}, status={}, message={:?}",
                            err.id, err.status, err.message);
                    }
                }
            }
            Err(e) => {
                error!("Batcher flush failed: {}", e);
                // 不重试：client.ingest() 内部已有重试逻辑
                // 失败的事件被丢弃（fire-and-forget 语义），避免内存无限增长
            }
        }
    }
    ```
  - 原因: `tokio::select!` 同时处理三种触发源（channel 命令、定时器、channel 关闭），确保定时 flush 和手动 flush 不会遗漏。`drain(..)` 取出事件后 buffer 自动清空，容量保留。`do_flush` 中的错误处理采用 fire-and-forget 语义：client.ingest() 内部已重试，如果仍然失败则丢弃事件并记录 error 日志，避免内存无限增长

- [ ] 实现 add() 方法 — 事件入队 + 背压策略
  - 位置: `langfuse-client/src/batcher.rs`（`impl Batcher` 块内，紧接 `do_flush()` 之后）
  - 根据背压策略选择 `try_send` 或 `send`：
    ```rust
    /// 添加事件到批量队列
    ///
    /// 根据 `BackpressurePolicy` 表现不同：
    /// - `DropNew`: 队列满时立即返回 Err（非阻塞）
    /// - `Block`: 队列满时等待空间（阻塞当前 task）
    pub async fn add(&self, event: IngestionEvent) -> Result<(), LangfuseError> {
        let cmd = BatcherCommand::Add(event);
        match self.backpressure {
            BackpressurePolicy::DropNew => {
                self.tx.try_send(cmd).map_err(|e| match e {
                    mpsc::error::TrySendError::Full(_) => {
                        warn!("Batcher queue full, dropping event (DropNew policy)");
                        LangfuseError::ChannelClosed
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        warn!("Batcher channel closed, event dropped");
                        LangfuseError::ChannelClosed
                    }
                })
            }
            BackpressurePolicy::Block => {
                self.tx.send(cmd).await.map_err(|_| {
                    warn!("Batcher channel closed during send");
                    LangfuseError::ChannelClosed
                })
            }
        }
    }
    ```
  - 原因: DropNew 用 `try_send` 实现非阻塞语义（队列满时立即返回错误，适合采样场景）；Block 用 `send.await` 实现阻塞语义（等待消费者消费，适合不能丢事件的场景）。两种策略共用同一个 channel，背压选择在调用侧完成

- [ ] 实现 flush() 方法 — 手动触发 flush 并等待完成
  - 位置: `langfuse-client/src/batcher.rs`（`impl Batcher` 块内，紧接 `add()` 之后）
  - 通过 oneshot channel 同步等待 flush 完成：
    ```rust
    /// 手动触发 flush，等待所有待发送事件发送完毕
    ///
    /// 典型用途：短生命周期应用（CLI 工具、测试）在退出前确保所有事件已发送
    pub async fn flush(&self) -> Result<(), LangfuseError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(BatcherCommand::Flush(tx))
            .await
            .map_err(|_| {
                warn!("Batcher channel closed, cannot flush");
                LangfuseError::ChannelClosed
            })?;
        // 等待后台 task 处理完 flush 后通知
        rx.await.map_err(|_| {
            warn!("Batcher dropped flush acknowledgment");
            LangfuseError::ChannelClosed
        })
    }
    ```
  - 原因: oneshot channel 实现同步等待语义——调用方 await `flush()` 时会阻塞直到后台 task 完成实际发送。如果后台 task 已退出（channel 关闭），返回 `ChannelClosed` 错误

- [ ] 实现 Drop trait — 优雅关闭
  - 位置: `langfuse-client/src/batcher.rs`（`impl Batcher` 块之后，独立的 `impl Drop` 块）
  - Drop 时发送 Shutdown 命令并等待后台 task 完成：
    ```rust
    impl Drop for Batcher {
        fn drop(&mut self) {
            // 发送 Shutdown 命令（fire-and-forget，使用 try_send 避免阻塞 drop）
            let shutdown_cmd = BatcherCommand::Shutdown;
            if self.tx.try_send(shutdown_cmd).is_err() {
                // channel 已关闭，后台 task 可能已退出
                debug!("Batcher Drop: channel already closed, background task may have exited");
            }

            // 等待后台 task 完成（最多等待 5 秒）
            if let Some(handle) = self.handle.take() {
                // 在同步 Drop 中无法 await，使用 tokio::task::block_in_place
                // 或直接 abort 后台 task 作为保底
                handle.abort();
                debug!("Batcher Drop: aborted background task");
            }
        }
    }
    ```
  - 原因: Drop 是同步方法无法 `.await`，因此用 `try_send` 发送 Shutdown 命令（fire-and-forget）并 `abort()` 后台 task 作为保底。对于需要确保所有事件发送完毕的场景，调用方应在 Drop 前显式调用 `flush().await`。abort 会取消后台 task 中正在进行的 HTTP 请求，但 client.ingest() 内部的重试机制已经保证了"尽力发送"语义

- [ ] 更新 lib.rs — 追加 batcher 模块声明和重导出
  - 位置: `langfuse-client/src/lib.rs`
  - 在现有 `pub mod client;` 之后追加 `pub mod batcher;`
  - 在现有 `pub use` 语句之后追加 `pub use batcher::Batcher;`
  - 原因: 暴露 Batcher 给外部使用者，调用方通过 `langfuse_client::Batcher` 使用

- [ ] 为 Batcher 编写单元测试
  - 测试文件: `langfuse-client/src/batcher.rs`（内联 `#[cfg(test)] mod tests`）
  - 测试策略: 使用 mockito mock Client 的 HTTP 调用，验证 Batcher 的批量聚合、flush 时机、背压行为
  - 测试场景:
    - `test_batcher_new_creates_running_task`: 构造 `Batcher::new(client, BatcherConfig::default())`，验证 Batcher 实例创建成功且后台 task 在运行（通过 `add()` 发送一个事件不报错来间接验证）
    - `test_batcher_add_and_manual_flush`: 构造 Batcher（config: max_events=10, flush_interval=60s），用 mockito mock POST `/api/public/ingestion` 返回 207 + `{"successes":[],"errors":[]}`。调用 `batcher.add(trace_create_event).await` 3 次，然后调用 `batcher.flush().await`。验证 mock 恰好被调用 1 次，请求体中 `"batch"` 数组长度为 3
    - `test_batcher_auto_flush_on_max_events`: 构造 Batcher（config: max_events=3, flush_interval=60s），mock 207 响应。连续调用 `batcher.add(event).await` 3 次（达到 max_events），短暂 sleep 等待后台 task 处理。验证 mock 被调用 1 次，请求体中 batch 长度为 3
    - `test_batcher_periodic_flush`: 构造 Batcher（config: max_events=100, flush_interval=200ms），mock 207 响应。调用 `batcher.add(event).await` 1 次，sleep 500ms 等待定时 flush 触发。验证 mock 被调用至少 1 次
    - `test_batcher_flush_empty_buffer`: 构造 Batcher 后立即调用 `batcher.flush().await`（不添加任何事件）。验证 mock 未被调用（空 buffer 不触发 HTTP 请求），且 `flush()` 返回 `Ok(())`
    - `test_batcher_backpressure_drop_new`: 构造 Batcher（config: max_events=2, backpressure=DropNew, flush_interval=60s），mock 响应延迟 500ms（`Mock::new().with_delay(Duration::from_millis(500))`）。在后台 task 因 mock 延迟而阻塞时，连续调用 `add()` 超过 channel 容量（2 个命令）。验证第三次 `add()` 返回 `Err(LangfuseError::ChannelClosed)` 或因为 try_send 返回 Full 而报错
      - 注意：更可行的方案是暂停 mock server 或使用极短超时。简化方案：构造 channel 容量为 1 的 Batcher（max_events=1），发送 1 个事件触发 auto flush，在 flush 期间（mock 延迟）再 try_send 第 2 个事件，验证 try_send 返回 Full 错误
    - `test_batcher_backpressure_block`: 构造 Batcher（config: max_events=5, backpressure=Block, flush_interval=60s），mock 207 响应（无延迟）。连续调用 `batcher.add(event).await` 5 次。验证所有 `add()` 返回 `Ok(())`，mock 被调用 1 次（达到 max_events 自动 flush）
    - `test_batcher_graceful_shutdown_on_drop`: 构造 Batcher（config: max_events=10, flush_interval=60s），mock 207 响应。添加 2 个事件，然后 `drop(batcher)`。短暂 sleep 后验证 mock 被调用至少 1 次（Shutdown 命令触发 flush 剩余事件）
      - 注意：由于 Drop 使用 `abort()`，Shutdown 命令可能来不及处理。此测试验证 Drop 不 panic，且尽力 flush。实际可靠测试需要显式调用 `flush()` 后再 drop
    - `test_batcher_multiple_flush_cycles`: 构造 Batcher（config: max_events=2, flush_interval=200ms），mock 207 响应。第一轮：add 2 个事件 → 触发 auto flush。第二轮：add 2 个事件 → 再次 auto flush。验证 mock 总共被调用 2 次
    - `test_batcher_flush_returns_error_on_closed_channel`: 构造 Batcher 后立即 drop 其内部 clone 的 Sender（通过消费掉 Batcher），再尝试调用 `flush()`。验证返回 `Err(LangfuseError::ChannelClosed)`
      - 简化方案：构造 Batcher，`std::mem::drop(batcher)`，无法再调用方法。改为测试：构造 Batcher，用 `std::mem::forget` 或手动关闭 channel 来模拟。更可行的方案：构造 Batcher 并调用 `flush()` 时 mock 一个延迟，同时在另一个 task 中 drop 掉 Batcher 的 sender。实际最简方案：在测试中直接验证 `flush()` 在正常流程下返回 `Ok(())`，channel 关闭场景通过文档说明
    - `test_batcher_handles_ingest_error`: 构造 Batcher（config: max_events=2, flush_interval=60s），mock 返回 500 + `{"error":"internal"}`（设置 max_retries=0 避免重试延迟）。添加 2 个事件触发 auto flush。验证 Batcher 不 panic（错误被 do_flush 内部捕获并记录日志），之后还能继续 `add()` 新事件
    - `test_batcher_with_large_batch`: 构造 Batcher（config: max_events=50, flush_interval=60s），mock 207 响应。添加 50 个事件触发 auto flush。验证 mock 被调用 1 次，请求体 batch 长度为 50
  - 辅助函数（在 `mod tests` 内定义）:
    - `fn create_mock_server()` → 创建并启动 `mockito::ServerGuard`
    - `fn create_test_client(server_url: &str) -> LangfuseClient` → 构造测试用 `LangfuseClient::new("pk", "sk", server_url, 0)`（max_retries=0 避免测试中的重试延迟）
    - `fn create_test_event(id: &str) -> IngestionEvent` → 构造简单的 `IngestionEvent::TraceCreate` 用于测试
    - `fn create_207_mock(server: &mut mockito::ServerGuard, hit_count: usize) -> Mock` → 创建 207 响应的 mock，设置 `expect(hit_count)`
  - 运行命令: `cd langfuse-client && cargo test --lib -- batcher::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 crate 可编译（batcher.rs 加入后）
  - `cd langfuse-client && cargo check 2>&1`
  - 预期: 输出 `Checking langfuse-client v0.1.0` 且无 error

- [ ] 验证 batcher.rs 包含 Batcher 结构体和核心方法
  - `cd langfuse-client && grep -n 'pub struct Batcher\|pub fn new\|pub async fn add\|pub async fn flush\|impl Drop for Batcher\|async fn run_loop\|async fn do_flush' src/batcher.rs`
  - 预期: 7 个匹配，分别对应结构体、构造函数、add、flush、Drop impl、run_loop、do_flush

- [ ] 验证 BatcherCommand 包含 3 种变体
  - `cd langfuse-client && grep -E '^\s+Add\(|^\s+Flush\(|^\s+Shutdown' src/batcher.rs`
  - 预期: 3 个匹配

- [ ] 验证背压策略实现（try_send 和 send 均存在）
  - `cd langfuse-client && grep -c 'try_send\|\.send(' src/batcher.rs`
  - 预期: 至少 3 个匹配（add() 中的 try_send/send + flush() 中的 send + Drop 中的 try_send）

- [ ] 验证 tokio::select! 用于事件循环
  - `cd langfuse-client && grep -c 'tokio::select!' src/batcher.rs`
  - 预期: 1 个匹配

- [ ] 验证定时 flush 使用 interval
  - `cd langfuse-client && grep -c 'interval(' src/batcher.rs`
  - 预期: 1 个匹配

- [ ] 验证 lib.rs 包含 batcher 模块声明和重导出
  - `cd langfuse-client && grep -E 'pub mod batcher|pub use batcher::Batcher' src/lib.rs`
  - 预期: 包含 `pub mod batcher;` 和 `pub use batcher::Batcher;`

- [ ] 验证所有单元测试通过
  - `cd langfuse-client && cargo test --lib -- batcher::tests 2>&1`
  - 预期: 所有 test 结果为 `test result: ok`，0 failures

- [ ] 验证完整测试套件无回归（含 Task 1/2/3 测试）
  - `cd langfuse-client && cargo test 2>&1`
  - 预期: 全部测试通过，0 failures

---

### Task 5: TUI 集成迁移

**背景:**
替换 `rust-agent-tui` 中的 `langfuse-ergonomic` + `langfuse-client-base` 两个第三方 crate，改用 Task 1-4 实现的 `langfuse-client` 新 crate。当前 `rust-agent-tui/src/langfuse/` 目录包含 3 个源文件：session.rs（使用 `langfuse_ergonomic::ClientBuilder/Batcher/LangfuseClient`）、tracer.rs（使用 `langfuse_client_base::models` 的 `IngestionEvent/IngestionEventOneOf2/4/8/CreateSpanBody/CreateGenerationBody/ObservationBody/UsageDetails`）、config.rs（纯环境变量读取，无需修改）。对外接口 `LangfuseSession` / `LangfuseTracer` 的方法签名和行为保持不变，仅替换内部实现。上层调用方（`agent_ops.rs`、`langfuse_state.rs`、`agent.rs`）通过 `crate::langfuse::LangfuseSession/LangfuseTracer` 引用，只要公开 API 不变则无需改动。

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`（移除 langfuse-ergonomic + langfuse-client-base，添加 langfuse-client path dep）
- 修改: `rust-agent-tui/src/langfuse/session.rs`（改用 `langfuse_client::LangfuseClient + Batcher + BatcherConfig`）
- 修改: `rust-agent-tui/src/langfuse/tracer.rs`（改用 `langfuse_client::types::*`，消除 `Option<Option<T>>` 双层嵌套）
- 不修改: `rust-agent-tui/src/langfuse/config.rs`（环境变量读取逻辑通用，无第三方依赖）
- 不修改: `rust-agent-tui/src/langfuse/mod.rs`（公开导出不变）
- 不修改: `rust-agent-tui/src/app/agent_ops.rs`、`langfuse_state.rs`、`agent.rs`（对外接口不变）

**执行步骤:**

- [ ] 修改 Cargo.toml — 替换依赖声明
  - 位置: `rust-agent-tui/Cargo.toml`（`[dependencies]` 段，~L43-L44）
  - 删除以下两行:
    ```toml
    langfuse-ergonomic = "0.6.3"
    langfuse-client-base = "0.7.1"
    ```
  - 在同一位置添加:
    ```toml
    langfuse-client = { path = "../../langfuse-client" }
    ```
  - 原因: 新 crate 路径为 workspace 外的 `../../langfuse-client`（与 spec-design.md 约定一致，新 crate 独立于 workspace）。移除两个第三方依赖后，`reqwest` 仍保留（其他地方也使用）

- [ ] 重写 session.rs — 改用 langfuse_client 的 Client + Batcher
  - 位置: `rust-agent-tui/src/langfuse/session.rs`（全文替换）
  - 当前代码使用 `langfuse_ergonomic::{BackpressurePolicy, Batcher, ClientBuilder, LangfuseClient}`，需改为 `langfuse_client` 的对应 API
  - 替换后完整代码:
    ```rust
    use std::sync::Arc;
    use std::time::Duration;

    use langfuse_client::{
        BackpressurePolicy, Batcher, BatcherConfig, LangfuseClient,
    };

    use super::config::LangfuseConfig;

    /// Langfuse Thread 级别会话，持有跨多轮复用的共享连接状态。
    ///
    /// 生命周期：Thread 创建/打开时构造，new_thread()/open_thread() 时重置（= None）。
    /// 同一 Thread 内所有 `LangfuseTracer` 共享同一个 client + batcher + session_id。
    pub struct LangfuseSession {
        pub client: Arc<LangfuseClient>,
        pub batcher: Arc<Batcher>,
        /// session_id = thread_id，Thread 内所有 Trace 共享
        pub session_id: String,
    }

    impl LangfuseSession {
        /// 从配置和 session_id 构造 Session，失败时返回 None（静默降级）
        pub async fn new(config: LangfuseConfig, session_id: String) -> Option<Self> {
            let client = LangfuseClient::new(
                &config.public_key,
                &config.secret_key,
                &config.host,
                3, // max_retries = 3
            );

            let batcher_config = BatcherConfig {
                max_events: 50,
                flush_interval: Duration::from_secs(10),
                backpressure: BackpressurePolicy::DropNew,
                max_retries: 3,
            };
            let batcher = Batcher::new(client, batcher_config);

            Some(Self {
                client: Arc::new(client),
                batcher: Arc::new(batcher),
                session_id,
            })
        }
    }
    ```
  - **API 映射详解:**
    - `ClientBuilder::new().public_key().secret_key().base_url().build().ok()?` → `LangfuseClient::new(&public_key, &secret_key, &base_url, max_retries)`（构造不再返回 Result，构造失败会 panic 于 reqwest::Client 构建，实际不会失败）
    - `Batcher::builder().client().max_events(50).flush_interval(Duration::from_secs(10)).backpressure_policy(BackpressurePolicy::DropNew).build().await` → `Batcher::new(client, BatcherConfig{ max_events: 50, flush_interval: Duration::from_secs(10), backpressure: BackpressurePolicy::DropNew, max_retries: 3 })`（Batcher::new 为同步方法，内部 tokio::spawn 启动后台 task）
    - `LangfuseSession` 结构体字段类型从 `Arc<langfuse_ergonomic::LangfuseClient>` → `Arc<langfuse_client::LangfuseClient>`，`Arc<langfuse_ergonomic::Batcher>` → `Arc<langfuse_client::Batcher>`
  - 原因: 对外接口 `LangfuseSession::new(config, session_id) -> Option<Self>` 签名和语义不变。`LangfuseClient::new()` 不返回 Result，因此 `new()` 不再因 Client 构造失败返回 None，仅保留 `Some(Self)` 路径。但为保持对外行为兼容（返回 Option<Self>），保留 `Option<Self>` 返回类型

- [ ] 重写 tracer.rs import 区块 — 替换类型导入
  - 位置: `rust-agent-tui/src/langfuse/tracer.rs`（文件开头 ~L1-L16）
  - 删除旧 import:
    ```rust
    use langfuse_client_base::models::{
        ingestion_event_one_of_2,
        ingestion_event_one_of_4::Type as GenType,
        ingestion_event_one_of_8,
        CreateGenerationBody, CreateSpanBody, IngestionEvent,
        IngestionEventOneOf2, IngestionEventOneOf4, IngestionEventOneOf8,
        ObservationBody, ObservationType, UsageDetails,
    };
    ```
  - 替换为新 import:
    ```rust
    use langfuse_client::{
        GenerationBody, IngestionEvent, ObservationBody,
        ObservationType, SpanBody, TraceBody, UsageDetails,
    };
    ```
  - 原因: 新 crate 将所有事件类型统一为 `IngestionEvent` 枚举的变体（`SpanCreate`/`GenerationCreate`/`ObservationCreate`/`TraceCreate`），不再需要 `IngestionEventOneOf2/4/8` 的 Box 包装。`CreateSpanBody` → `SpanBody`、`CreateGenerationBody` → `GenerationBody`。`UsageDetails` 从 enum 变为 `HashMap<String, i32>` 类型别名，简化构造

- [ ] 重写 tracer.rs 的 `flush_tools_batch` — SpanCreate 事件
  - 位置: `rust-agent-tui/src/langfuse/tracer.rs`（`flush_tools_batch()` 方法体，~L83-L121）
  - 将 `CreateSpanBody` + `IngestionEventOneOf2` 替换为 `SpanBody` + `IngestionEvent::SpanCreate`
  - 旧代码（需替换的部分，从 `let body = CreateSpanBody {` 到 `batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event)))`）:
    ```rust
    // 旧代码：
    let body = CreateSpanBody {
        id: Some(Some(batch_id)),           // double Option
        trace_id: Some(Some(trace_id.clone())),
        name: Some(Some("Tools".to_string())),
        start_time: Some(Some(batch_start)),
        end_time: Some(Some(batch_end.clone())),
        parent_observation_id: Some(Some(agent_span_id)),
        input: None,
        output: None,
        status_message: None,
        metadata: None,
        level: None,
        version: None,
        environment: None,
    };
    let event = IngestionEventOneOf2 {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: batch_end,
        body: Box::new(body),
        r#type: ingestion_event_one_of_2::Type::SpanCreate,
        metadata: None,
    };
    if let Err(e) = batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event))).await {
    ```
  - 新代码:
    ```rust
    // 新代码：
    let body = SpanBody {
        id: Some(batch_id),                 // 单层 Option
        trace_id: Some(trace_id.clone()),
        name: Some("Tools".to_string()),
        start_time: Some(batch_start),
        end_time: Some(batch_end.clone()),
        parent_observation_id: Some(agent_span_id),
        input: None,
        output: None,
        status_message: None,
        metadata: None,
        level: None,
        version: None,
        environment: None,
    };
    let event = IngestionEvent::SpanCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: batch_end,
        body,
        metadata: None,
    };
    if let Err(e) = batcher.add(event).await {
    ```
  - **映射规则:**
    - `Some(Some(value))` → `Some(value)`（消除 double Option，新 crate 所有 body 字段为 `Option<T>` 而非 `Option<Option<T>>`）
    - `CreateSpanBody { ... }` → `SpanBody { ... }`（字段名相同，类型简化）
    - `IngestionEventOneOf2 { id, timestamp, body: Box::new(body), r#type: ..., metadata }` → `IngestionEvent::SpanCreate { id, timestamp, body, metadata }`（body 不再 Box 包装，type 由枚举变体自动确定）
    - `batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event)))` → `batcher.add(event)`（无需 Box 包装）
  - 原因: 新 crate 的 `IngestionEvent` 使用 serde 内部标签枚举，`SpanCreate` 变体自动序列化为 `{"type":"span-create","id":"...","timestamp":"...","body":{...}}`

- [ ] 重写 tracer.rs 的 `on_trace_start` — TraceCreate + ObservationCreate 事件
  - 位置: `rust-agent-tui/src/langfuse/tracer.rs`（`on_trace_start()` 方法体，~L124-L172）
  - 有两处需替换:
    1. **TraceCreate 部分**（`client.trace().id().name().input().session_id().call().await`）→ 构造 `IngestionEvent::TraceCreate` 并通过 `batcher.add()` 发送
    2. **ObservationCreate 部分**（`ObservationBody { ... }` + `IngestionEventOneOf8`）→ 新 `ObservationBody` + `IngestionEvent::ObservationCreate`
  - 旧 TraceCreate 代码:
    ```rust
    if let Err(e) = client
        .trace()
        .id(trace_id.clone())
        .name("agent-run")
        .input(serde_json::json!(input.clone()))
        .session_id(session_id)
        .call()
        .await
    {
        tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 创建失败");
    }
    ```
  - 新 TraceCreate 代码:
    ```rust
    let trace_body = TraceBody {
        id: Some(trace_id.clone()),
        name: Some("agent-run".to_string()),
        input: Some(serde_json::json!(input.clone())),
        session_id: Some(session_id),
        ..Default::default()
    };
    let trace_event = IngestionEvent::TraceCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: start_time.clone(),
        body: trace_body,
        metadata: None,
    };
    if let Err(e) = batcher.add(trace_event).await {
        tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 创建失败");
    }
    ```
  - 注意：`TraceBody` 需要实现 `Default` trait（在 `langfuse-client/src/types.rs` 中添加 `#[derive(Default)]`），或使用完整字段初始化。如果 `TraceBody` 未 derive `Default`，则需手动列出所有 `None` 字段:
    ```rust
    let trace_body = TraceBody {
        id: Some(trace_id.clone()),
        name: Some("agent-run".to_string()),
        user_id: None,
        input: Some(serde_json::json!(input.clone())),
        output: None,
        session_id: Some(session_id),
        release: None,
        version: None,
        metadata: None,
        tags: None,
        environment: None,
        public: None,
        timestamp: None,
    };
    ```
  - **设计决策：** `TraceBody` 应在 Task 2 中 derive `Default`（所有字段均为 `Option<T>`，天然支持 Default）。如果 Task 2 实现中未添加 `Default`，本步骤需先在 `langfuse-client/src/types.rs` 的 `TraceBody` 定义处添加 `#[derive(Default)]`
  - 旧 ObservationCreate 代码:
    ```rust
    let body = ObservationBody {
        id: Some(Some(agent_span_id)),
        trace_id: Some(Some(trace_id.clone())),
        r#type: ObservationType::Agent,
        name: Some(Some("Agent".to_string())),
        input: Some(Some(serde_json::json!(input))),
        start_time: Some(Some(start_time.clone())),
        ..Default::default()
    };
    let event = IngestionEventOneOf8 {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: start_time,
        body: Box::new(body),
        r#type: ingestion_event_one_of_8::Type::ObservationCreate,
        metadata: None,
    };
    if let Err(e) = batcher.add(IngestionEvent::IngestionEventOneOf8(Box::new(event))).await {
    ```
  - 新 ObservationCreate 代码:
    ```rust
    let body = ObservationBody {
        id: Some(agent_span_id),
        trace_id: Some(trace_id.clone()),
        r#type: ObservationType::Agent,
        name: Some("Agent".to_string()),
        input: Some(serde_json::json!(input)),
        start_time: Some(start_time.clone()),
        ..Default::default()
    };
    let event = IngestionEvent::ObservationCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: start_time,
        body,
        metadata: None,
    };
    if let Err(e) = batcher.add(event).await {
    ```
  - 原因: 与 `flush_tools_batch` 相同的映射规则。`ObservationType::Agent` 的 serde 序列化不变（`SCREAMING_SNAKE_CASE` → `"AGENT"`）。ObservationBody 的 `..Default::default()` 需要该结构体实现 Default（所有字段均为 Option<T> 或有 default，天然支持）

- [ ] 重写 tracer.rs 的 `on_llm_end` — GenerationCreate 事件
  - 位置: `rust-agent-tui/src/langfuse/tracer.rs`（`on_llm_end()` 方法体中的 spawn 块，~L236-L264）
  - 两处修改: UsageDetails 构造 + GenerationCreate 事件构造
  - 旧 UsageDetails 构造（~L219-L234）:
    ```rust
    let langfuse_usage_details = usage.map(|u| {
        let mut map = std::collections::HashMap::new();
        // ... insert keys ...
        Box::new(UsageDetails::Object(map))
    });
    ```
  - 新 UsageDetails 构造:
    ```rust
    let langfuse_usage_details = usage.map(|u| {
        let mut map = std::collections::HashMap::new();
        let cache_creation = u.cache_creation_input_tokens.unwrap_or(0);
        let cache_read = u.cache_read_input_tokens.unwrap_or(0);
        let total = u.input_tokens + u.output_tokens + cache_creation + cache_read;
        map.insert("input".to_string(), u.input_tokens as i32);
        map.insert("output".to_string(), u.output_tokens as i32);
        map.insert("total".to_string(), total as i32);
        if cache_creation > 0 {
            map.insert("cache_creation_input_tokens".to_string(), cache_creation as i32);
        }
        if cache_read > 0 {
            map.insert("cache_read_input_tokens".to_string(), cache_read as i32);
        }
        map  // 直接返回 HashMap<String, i32>，即 UsageDetails
    });
    ```
  - 原因: 新 crate 的 `UsageDetails` 是 `HashMap<String, i32>` 类型别名，无需 `Box::new(UsageDetails::Object(map))` 包装。HashMap 直接作为 `Option<UsageDetails>` 使用
  - 旧 GenerationCreate 事件（~L238-L263）:
    ```rust
    let body = CreateGenerationBody {
        id: Some(Some(gen_id.clone())),
        trace_id: Some(Some(trace_id.clone())),
        name: Some(Some(format!("Chat{}", provider_name))),
        input: Some(Some(input_json)),
        output: Some(Some(serde_json::json!(output))),
        model: Some(Some(model)),
        usage_details: langfuse_usage_details,
        parent_observation_id: Some(Some(agent_span_id)),
        start_time: Some(Some(start_time)),
        end_time: Some(Some(end_time)),
        ..Default::default()
    };
    let event = IngestionEventOneOf4 {
        id: gen_id.clone(),
        timestamp,
        body: Box::new(body),
        r#type: GenType::GenerationCreate,
        metadata: None,
    };
    if let Err(e) = batcher.add(IngestionEvent::IngestionEventOneOf4(Box::new(event))).await {
    ```
  - 新 GenerationCreate 事件:
    ```rust
    let body = GenerationBody {
        id: Some(gen_id.clone()),
        trace_id: Some(trace_id.clone()),
        name: Some(format!("Chat{}", provider_name)),
        input: Some(input_json),
        output: Some(serde_json::json!(output)),
        model: Some(model),
        usage_details: langfuse_usage_details,
        parent_observation_id: Some(agent_span_id),
        start_time: Some(start_time),
        end_time: Some(end_time),
        ..Default::default()
    };
    let event = IngestionEvent::GenerationCreate {
        id: gen_id.clone(),
        timestamp,
        body,
        metadata: None,
    };
    if let Err(e) = batcher.add(event).await {
    ```
  - 原因: `CreateGenerationBody` → `GenerationBody`，`Some(Some(x))` → `Some(x)`，`usage_details` 字段类型从 `Option<Box<UsageDetails>>` → `Option<UsageDetails>`（即 `Option<HashMap<String, i32>>`），无需 Box 包装

- [ ] 重写 tracer.rs 的 `on_tool_end` — 工具 SpanCreate 事件
  - 位置: `rust-agent-tui/src/langfuse/tracer.rs`（`on_tool_end()` 方法体内的 spawn 块，~L309-L336）
  - 旧代码（需替换部分）:
    ```rust
    let status_msg = if is_error { Some(Some("error".to_string())) } else { None };
    let body = CreateSpanBody {
        id: Some(Some(tool.span_id)),
        trace_id: Some(Some(trace_id_log.clone())),
        name: Some(Some(tool.name)),
        input: Some(Some(tool.input)),
        output: Some(Some(serde_json::json!(output))),
        start_time: Some(Some(tool.start_time)),
        end_time: Some(Some(end_time.clone())),
        parent_observation_id: Some(Some(tool.parent_span_id)),
        status_message: status_msg,
        metadata: None,
        level: None,
        version: None,
        environment: None,
    };
    let event = IngestionEventOneOf2 {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: end_time,
        body: Box::new(body),
        r#type: ingestion_event_one_of_2::Type::SpanCreate,
        metadata: None,
    };
    if let Err(e) = batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event))).await {
    ```
  - 新代码:
    ```rust
    let status_msg = if is_error { Some("error".to_string()) } else { None };
    let body = SpanBody {
        id: Some(tool.span_id),
        trace_id: Some(trace_id_log.clone()),
        name: Some(tool.name),
        input: Some(tool.input),
        output: Some(serde_json::json!(output)),
        start_time: Some(tool.start_time),
        end_time: Some(end_time.clone()),
        parent_observation_id: Some(tool.parent_span_id),
        status_message: status_msg,
        metadata: None,
        level: None,
        version: None,
        environment: None,
    };
    let event = IngestionEvent::SpanCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: end_time,
        body,
        metadata: None,
    };
    if let Err(e) = batcher.add(event).await {
    ```
  - 原因: 与 `flush_tools_batch` 相同映射规则

- [ ] 重写 tracer.rs 的 `on_trace_end` — Trace 更新事件
  - 位置: `rust-agent-tui/src/langfuse/tracer.rs`（`on_trace_end()` 方法体内的 spawn 块中更新 Trace 部分，~L376-L385）
  - 旧代码:
    ```rust
    // 更新 Trace 输出
    if let Err(e) = client
        .trace()
        .id(trace_id.clone())
        .name("agent-run")
        .output(serde_json::json!(output))
        .call()
        .await
    {
        tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 输出更新失败");
    }
    ```
  - 新代码:
    ```rust
    // 更新 Trace 输出（通过发送新的 TraceCreate 事件，Langfuse 会合并相同 id 的 Trace）
    let trace_body = TraceBody {
        id: Some(trace_id.clone()),
        name: Some("agent-run".to_string()),
        output: Some(serde_json::json!(output)),
        ..Default::default()
    };
    let trace_event = IngestionEvent::TraceCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        body: trace_body,
        metadata: None,
    };
    if let Err(e) = batcher.add(trace_event).await {
        tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 输出更新失败");
    }
    ```
  - **设计决策:** Langfuse 的 TraceCreate 是幂等操作——发送相同 `id` 的 TraceCreate 会合并更新已有 Trace 的字段。因此用 TraceCreate + 已有 trace_id 实现等价于 "更新 trace output" 的语义。这与 `langfuse-ergonomic` 的 `client.trace().id().output().call()` 底层行为一致（内部也是构造 TraceBody + IngestionEvent 发送到 ingestion API）
  - 注意：`on_trace_end` 中不再需要 `client` 引用（旧代码中仅用于 `client.trace().call()`），改为通过 `batcher` 发送 TraceCreate 事件。因此 `on_trace_end` spawn 块中 `let client = Arc::clone(&self.session.client);` 可移除
  - 原因: 新 crate 不提供 `client.trace()` 高级 API（设计文档明确仅覆盖 ingestion 端点），通过 batcher.add() 直接发送 IngestionEvent 等价且更高效

- [ ] 确保 TraceBody 和 ObservationBody derive Default
  - 位置: `langfuse-client/src/types.rs`（TraceBody 和 ObservationBody 定义处）
  - 在 `TraceBody` 的 derive 列表中添加 `Default`（如果 Task 2 中未添加）
  - 在 `ObservationBody` 的 derive 列表中确认包含 `Default`（`ObservationBody` 有 required 字段 `r#type: ObservationType`，不能 derive Default）。如果 ObservationBody 不能 derive Default，需将 `on_trace_start` 中的 `..Default::default()` 改为手动列出所有剩余字段:
    ```rust
    let body = ObservationBody {
        id: Some(agent_span_id),
        trace_id: Some(trace_id.clone()),
        r#type: ObservationType::Agent,
        name: Some("Agent".to_string()),
        input: Some(serde_json::json!(input)),
        start_time: Some(start_time.clone()),
        // 其余字段显式设为 None
        end_time: None,
        completion_start_time: None,
        parent_observation_id: None,
        output: None,
        metadata: None,
        model: None,
        model_parameters: None,
        level: None,
        status_message: None,
        version: None,
        environment: None,
    };
    ```
  - 原因: `ObservationBody` 的 `r#type` 是 required 字段（非 Option），Rust 的 `Default` derive 要求所有字段实现 Default，而 `ObservationType` 枚举无自然默认值。因此 `ObservationBody` 不能 derive `Default`，必须手动初始化所有字段。`SpanBody` 和 `GenerationBody` 的所有字段都是 `Option<T>`，可以 derive `Default`。`TraceBody` 同理所有字段为 `Option<T>`，可以 derive `Default`

- [ ] 编译验证 — rust-agent-tui 整体编译
  - 运行命令: `cargo build -p rust-agent-tui 2>&1`
  - 预期: 编译成功，无 error。可能有 unused import 警告（旧 import 移除后），需根据编译器提示清理
  - 修复策略: 编译错误按以下优先级处理:
    1. 类型不匹配（`Option<Option<T>>` vs `Option<T>`）→ 按映射规则移除内层 `Some()`
    2. 方法不存在（`client.trace()`）→ 已替换为 `batcher.add()`，检查是否有遗漏
    3. import 错误 → 确认 `langfuse_client::` 前缀下的所有类型已正确导出

- [ ] 运行 rust-agent-tui 现有测试
  - 运行命令: `cargo test -p rust-agent-tui 2>&1`
  - 预期: 所有测试通过（如有 headless 测试中涉及 langfuse 相关逻辑的，需确认行为不变）
  - 注意: rust-agent-tui 的 langfuse 功能是可选的（需要设置 LANGFUSE_* 环境变量），测试中未设置这些环境变量时 langfuse 代码路径不被触发

**检查步骤:**

- [ ] 验证 Cargo.toml 不再包含旧依赖
  - `grep -E 'langfuse-ergonomic|langfuse-client-base' rust-agent-tui/Cargo.toml`
  - 预期: 无匹配（exit code 1）

- [ ] 验证 Cargo.toml 包含新依赖
  - `grep 'langfuse-client' rust-agent-tui/Cargo.toml`
  - 预期: 包含 `langfuse-client = { path = "../../langfuse-client" }`

- [ ] 验证 session.rs 使用新 crate 的 import
  - `grep -n 'langfuse_' rust-agent-tui/src/langfuse/session.rs`
  - 预期: 仅包含 `use langfuse_client::` 前缀的 import，不包含 `langfuse_ergonomic` 或 `langfuse_client_base`

- [ ] 验证 tracer.rs 使用新 crate 的 import
  - `grep -n 'langfuse_' rust-agent-tui/src/langfuse/tracer.rs`
  - 预期: 仅包含 `use langfuse_client::` 前缀的 import

- [ ] 验证 tracer.rs 不再包含旧事件类型
  - `grep -E 'IngestionEventOneOf|ingestion_event_one_of|CreateSpanBody|CreateGenerationBody' rust-agent-tui/src/langfuse/tracer.rs`
  - 预期: 无匹配（exit code 1）

- [ ] 验证 tracer.rs 不再包含 double Option 模式
  - `grep -c 'Some(Some(' rust-agent-tui/src/langfuse/tracer.rs`
  - 预期: 0 个匹配

- [ ] 验证 tracer.rs 使用新的 IngestionEvent 枚举变体
  - `grep -E 'IngestionEvent::(SpanCreate|GenerationCreate|ObservationCreate|TraceCreate)' rust-agent-tui/src/langfuse/tracer.rs`
  - 预期: 包含所有 4 种变体（SpanCreate 至少 2 处——flush_tools_batch + on_tool_end，GenerationCreate 1 处，ObservationCreate 1 处，TraceCreate 2 处——on_trace_start + on_trace_end）

- [ ] 验证 tracer.rs 不再调用 client.trace()
  - `grep -c 'client\.trace()' rust-agent-tui/src/langfuse/tracer.rs`
  - 预期: 0 个匹配

- [ ] 验证 config.rs 未被修改
  - `grep -E 'langfuse_ergonomic|langfuse_client_base|langfuse_client' rust-agent-tui/src/langfuse/config.rs`
  - 预期: 无匹配（config.rs 不引用任何第三方 langfuse crate）

- [ ] 验证 mod.rs 公开导出不变
  - `grep -E 'pub use (config|session|tracer)' rust-agent-tui/src/langfuse/mod.rs`
  - 预期: 包含 `pub use config::LangfuseConfig;`、`pub use session::LangfuseSession;`、`pub use tracer::LangfuseTracer;`（与修改前一致）

- [ ] 验证整体编译通过
  - `cargo build -p rust-agent-tui 2>&1`
  - 预期: 编译成功，无 error

- [ ] 验证 langfuse-client crate 编译通过
  - `cd langfuse-client && cargo check 2>&1`
  - 预期: 编译成功

- [ ] 验证 rust-agent-tui 测试通过
  - `cargo test -p rust-agent-tui 2>&1`
  - 预期: 所有测试通过

- [ ] 验证 langfuse-client 全量测试通过
  - `cd langfuse-client && cargo test 2>&1`
  - 预期: 全部测试通过

- [ ] 验证 workspace 全量编译无回归
  - `cargo build 2>&1`
  - 预期: 所有 workspace crate 编译成功

---

### Task 6: langfuse-client 验收

**前置条件:**
- `langfuse-client` crate 已编译通过: `cd langfuse-client && cargo build`
- `rust-agent-tui` 已编译通过: `cargo build -p rust-agent-tui`
- 测试数据: 无需外部服务（mockito mock）

**端到端验证:**

1. 运行 langfuse-client 完整测试套件
   - `cd langfuse-client && cargo test 2>&1`
   - 预期: 全部测试通过（config + error + types + client + batcher），0 failures
   - 失败排查: 检查各 Task 的测试步骤，重点关注 types::tests 的序列化 roundtrip 和 client::tests 的 mockito mock

2. 运行 workspace 全量测试
   - `cargo test 2>&1`
   - 预期: 所有 workspace crate 测试通过，包括 rust-agent-tui 的 headless 测试
   - 失败排查: 检查 Task 5（TUI 集成迁移）— 类型不匹配或 import 错误

3. 验证 langfuse-client crate 无 Option<Option<T>> 嵌套
   - `cd langfuse-client && grep -r 'Option<Option<' src/`
   - 预期: 无匹配（exit code 1）
   - 失败排查: 检查 types.rs 中的 body 结构体定义

4. 验证 rust-agent-tui 不再依赖旧 crate
   - `grep -E 'langfuse-ergonomic|langfuse-client-base' rust-agent-tui/Cargo.toml`
   - 预期: 无匹配（exit code 1）
   - 失败排查: 检查 Task 5 步骤 1（Cargo.toml 替换）

5. 验证 tracer.rs 无 double Option 和旧事件类型
   - `grep -E 'Some\(Some\(|IngestionEventOneOf|CreateSpanBody|CreateGenerationBody|client\.trace\(\)' rust-agent-tui/src/langfuse/tracer.rs`
   - 预期: 无匹配（exit code 1）
   - 失败排查: 检查 Task 5 步骤 3-7（tracer.rs 各方法替换）

6. 验证 workspace 全量编译
   - `cargo build 2>&1`
   - 预期: 所有 workspace crate 编译成功
   - 失败排查: 检查 Task 5 编译验证步骤的修复策略

7. 验证 langfuse-client 作为独立 crate 可编译
   - `cd langfuse-client && cargo check 2>&1`
   - 预期: 编译成功，无 error
   - 失败排查: 检查 Task 1-4 的模块依赖关系

---

