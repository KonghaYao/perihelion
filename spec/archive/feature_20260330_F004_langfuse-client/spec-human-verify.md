# langfuse-client 人工验收清单

**生成时间:** 2026-03-30
**关联计划:** spec/feature_20260330_F004_langfuse-client/spec-plan.md
**关联设计:** spec/feature_20260330_F004_langfuse-client/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version`
- [ ] [AUTO] 全 workspace 编译: `cargo build 2>&1 | tail -5`

### 测试数据准备
- [ ] 无需外部服务（mockito mock 覆盖所有 HTTP 测试）

---

## 验收项目

### 场景 1：langfuse-client crate 结构完整性

#### - [x] 1.1 模块文件齐全
- **来源:** spec-plan.md Task 1-4 / spec-design.md 模块结构
- **目的:** 确认 6 个源文件均存在
- **操作步骤:**
  1. [A] `ls langfuse-client/src/lib.rs langfuse-client/src/error.rs langfuse-client/src/config.rs langfuse-client/src/types.rs langfuse-client/src/client.rs langfuse-client/src/batcher.rs` → 期望包含: 无报错，6 个文件均存在

#### - [x] 1.2 Cargo.toml 依赖正确
- **来源:** spec-plan.md Task 1/3
- **目的:** 确认依赖声明完整且版本合理
- **操作步骤:**
  1. [A] `grep -E 'reqwest|serde |serde_json|tokio |thiserror|chrono |base64 |tracing' langfuse-client/Cargo.toml` → 期望包含: 7 个依赖
  2. [A] `grep 'mockito' langfuse-client/Cargo.toml` → 期望包含: `mockito = "1"`（dev-dependencies）
  3. [A] `grep 'temp-env' langfuse-client/Cargo.toml` → 期望包含: `temp-env = "0.3"`（dev-dependencies）

#### - [x] 1.3 lib.rs 重导出完整
- **来源:** spec-plan.md Task 2/3/4
- **目的:** 确认所有核心类型可从 crate root 直接引用
- **操作步骤:**
  1. [A] `grep -E 'pub mod|pub use' langfuse-client/src/lib.rs` → 期望包含: config, error, types, client, batcher 模块 + LangfuseClient, Batcher, IngestionEvent 等重导出

---

### 场景 2：类型系统正确性

#### - [x] 2.1 IngestionEvent 包含 10 种变体
- **来源:** spec-plan.md Task 2 / spec-design.md IngestionEvent
- **目的:** 确认 10 种事件类型全部定义
- **操作步骤:**
  1. [A] `grep -E '^\s+(TraceCreate|SpanCreate|SpanUpdate|GenerationCreate|GenerationUpdate|EventCreate|ScoreCreate|ObservationCreate|ObservationUpdate|SdkLog)\s*\{' langfuse-client/src/types.rs | wc -l` → 期望精确: 10

#### - [x] 2.2 serde 内部标签配置正确
- **来源:** spec-plan.md Task 2
- **目的:** 确认 `type` 判别字段和 kebab-case 自动转换
- **操作步骤:**
  1. [A] `grep -A2 'pub enum IngestionEvent' langfuse-client/src/types.rs | head -5` → 期望包含: `#[serde(tag = "type", rename_all = "kebab-case")]`

#### - [x] 2.3 无 Option<Option<T>> 嵌套
- **来源:** spec-design.md 实现要点 2
- **目的:** 确认消除双层 Option
- **操作步骤:**
  1. [A] `grep -r 'Option<Option<' langfuse-client/src/` → 期望包含: 无匹配（exit code 1）

#### - [x] 2.4 Body 结构体使用 camelCase + deny_unknown_fields
- **来源:** spec-plan.md Task 2
- **目的:** 确认 API JSON 格式兼容性
- **操作步骤:**
  1. [A] `grep -c 'rename_all = "camelCase"' langfuse-client/src/types.rs` → 期望包含: ≥ 7
  2. [A] `grep -c 'deny_unknown_fields' langfuse-client/src/types.rs` → 期望包含: ≥ 7

#### - [x] 2.5 ObservationType 包含 10 种变体
- **来源:** spec-design.md ObservationType
- **目的:** 确认 V4 全部观测类型
- **操作步骤:**
  1. [A] `grep -cE '^\s+(Span|Generation|Event|Agent|Tool|Chain|Retriever|Evaluator|Embedding|Guardrail),' langfuse-client/src/types.rs` → 期望精确: 10

---

### 场景 3：底层 Client 功能

#### - [x] 3.1 LangfuseClient 结构体和方法完整
- **来源:** spec-plan.md Task 3 / spec-design.md 底层 Client
- **目的:** 确认核心 API 齐全
- **操作步骤:**
  1. [A] `grep -n 'pub struct LangfuseClient\|pub fn new\|pub async fn ingest\|pub async fn ingest_single\|pub fn from_config' langfuse-client/src/client.rs` → 期望包含: 5 个匹配

#### - [x] 3.2 V4 ingestion header 存在
- **来源:** spec-design.md 底层 Client 设计
- **目的:** 确认 `x-langfuse-ingestion-version: 4` 请求头
- **操作步骤:**
  1. [A] `grep 'x-langfuse-ingestion-version' langfuse-client/src/client.rs` → 期望包含: `"4"`

#### - [x] 3.3 重试逻辑存在（指数退避）
- **来源:** spec-plan.md Task 3 / spec-design.md 关键设计点
- **目的:** 确认网络错误和 5xx 重试机制
- **操作步骤:**
  1. [A] `grep -E 'max_retries|1 <<|tokio::time::sleep' langfuse-client/src/client.rs` → 期望包含: 三个关键元素

#### - [x] 3.4 4xx 不重试
- **来源:** spec-plan.md Task 3
- **目的:** 确认客户端错误直接返回
- **操作步骤:**
  1. [A] `grep 'is_client_error' langfuse-client/src/client.rs` → 期望包含: 1 处匹配

#### - [x] 3.5 Basic Auth 认证构造
- **来源:** spec-design.md 底层 Client 设计
- **目的:** 确认 Base64 编码认证
- **操作步骤:**
  1. [A] `grep -E 'base64|auth_header|Authorization' langfuse-client/src/client.rs` → 期望包含: Basic Auth 构造和请求头

---

### 场景 4：上层 Batcher 功能

#### - [x] 4.1 Batcher 结构体和方法完整
- **来源:** spec-plan.md Task 4 / spec-design.md 上层 Batcher
- **目的:** 确认核心 API 齐全
- **操作步骤:**
  1. [A] `grep -n 'pub struct Batcher\|pub fn new\|pub async fn add\|pub async fn flush\|impl Drop for Batcher\|async fn run_loop\|async fn do_flush' langfuse-client/src/batcher.rs` → 期望包含: 7 个匹配

#### - [x] 4.2 BatcherCommand 3 种变体
- **来源:** spec-plan.md Task 4 / spec-design.md
- **目的:** 确认命令模式完整
- **操作步骤:**
  1. [A] `grep -E '^\s+Add\(|^\s+Flush\(|^\s+Shutdown' langfuse-client/src/batcher.rs` → 期望包含: 3 个匹配

#### - [x] 4.3 背压策略实现
- **来源:** spec-design.md 上层 Batcher 核心机制 5
- **目的:** 确认 DropNew 和 Block 两种策略
- **操作步骤:**
  1. [A] `grep -c 'try_send' langfuse-client/src/batcher.rs` → 期望包含: ≥ 1（DropNew）
  2. [A] `grep '\.send(' langfuse-client/src/batcher.rs` → 期望包含: Block 策略和 flush 命令发送

#### - [x] 4.4 tokio::select! 事件循环
- **来源:** spec-plan.md Task 4
- **目的:** 确认定时 flush + 命令处理并发
- **操作步骤:**
  1. [A] `grep -c 'tokio::select!' langfuse-client/src/batcher.rs` → 期望精确: 1
  2. [A] `grep 'interval(' langfuse-client/src/batcher.rs` → 期望包含: 定时 flush interval 创建

#### - [x] 4.5 优雅关闭
- **来源:** spec-plan.md Task 4 / spec-design.md 核心机制 6
- **目的:** 确认 Drop 时发送 Shutdown 并 abort
- **操作步骤:**
  1. [A] `grep -A10 'impl Drop for Batcher' langfuse-client/src/batcher.rs` → 期望包含: Shutdown 命令 + abort

---

### 场景 5：TUI 集成迁移

#### - [x] 5.1 Cargo.toml 旧依赖已移除
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认第三方依赖完全替换
- **操作步骤:**
  1. [A] `grep -E 'langfuse-ergonomic|langfuse-client-base' rust-agent-tui/Cargo.toml` → 期望包含: 无匹配（exit code 1）

#### - [x] 5.2 Cargo.toml 新依赖已添加
- **来源:** spec-plan.md Task 5
- **目的:** 确认 path dependency 正确
- **操作步骤:**
  1. [A] `grep 'langfuse-client' rust-agent-tui/Cargo.toml` → 期望包含: `langfuse-client = { path = "../langfuse-client" }`

#### - [x] 5.3 session.rs 仅使用新 crate
- **来源:** spec-plan.md Task 5
- **目的:** 确认无旧 crate 引用
- **操作步骤:**
  1. [A] `grep 'langfuse_' rust-agent-tui/src/langfuse/session.rs` → 期望包含: 仅 `langfuse_client::` 前缀

#### - [x] 5.4 tracer.rs 无旧事件类型
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认 IngestionEventOneOf 等旧类型已清除
- **操作步骤:**
  1. [A] `grep -E 'IngestionEventOneOf|ingestion_event_one_of|CreateSpanBody|CreateGenerationBody' rust-agent-tui/src/langfuse/tracer.rs` → 期望包含: 无匹配（exit code 1）

#### - [x] 5.5 tracer.rs 无 double Option
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认 Some(Some(...)) 模式已消除
- **操作步骤:**
  1. [A] `grep -c 'Some(Some(' rust-agent-tui/src/langfuse/tracer.rs` → 期望精确: 0

#### - [x] 5.6 tracer.rs 使用新 IngestionEvent 枚举变体
- **来源:** spec-plan.md Task 5
- **目的:** 确认 4 种事件类型使用正确
- **操作步骤:**
  1. [A] `grep -E 'IngestionEvent::(SpanCreate|GenerationCreate|ObservationCreate|TraceCreate)' rust-agent-tui/src/langfuse/tracer.rs` → 期望包含: 4 种变体

#### - [x] 5.7 tracer.rs 不再调用 client.trace()
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认旧高级 API 已替换为 batcher.add()
- **操作步骤:**
  1. [A] `grep -c 'client\.trace()' rust-agent-tui/src/langfuse/tracer.rs` → 期望精确: 0

#### - [x] 5.8 config.rs 未被修改
- **来源:** spec-plan.md Task 5
- **目的:** 确认配置读取逻辑保持不变
- **操作步骤:**
  1. [A] `grep -E 'langfuse_ergonomic|langfuse_client_base|langfuse_client' rust-agent-tui/src/langfuse/config.rs` → 期望包含: 无匹配（exit code 1）

#### - [x] 5.9 mod.rs 公开导出不变
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认对外接口一致
- **操作步骤:**
  1. [A] `grep -E 'pub use' rust-agent-tui/src/langfuse/mod.rs` → 期望包含: LangfuseConfig, LangfuseSession, LangfuseTracer

---

### 场景 6：编译与测试

#### - [x] 6.1 全 workspace 编译通过
- **来源:** spec-plan.md Task 6
- **目的:** 确认所有 crate 编译无 error
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -E 'error|Finished'` → 期望包含: `Finished` 且无 error

#### - [x] 6.2 langfuse-client 全量测试通过
- **来源:** spec-plan.md Task 6 端到端验证 1
- **目的:** 确认 59 个测试全部通过
- **操作步骤:**
  1. [A] `cargo test -p langfuse-client 2>&1 | grep 'test result'` → 期望包含: `ok. 59 passed; 0 failed`

#### - [x] 6.3 全 workspace 测试通过
- **来源:** spec-plan.md Task 6 端到端验证 2
- **目的:** 确认无回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | grep 'test result'` → 期望包含: 所有行均为 `ok` 且 `0 failed`

#### - [x] 6.4 langfuse-client 独立编译
- **来源:** spec-plan.md Task 6 端到端验证 7
- **目的:** 确认 crate 可独立使用
- **操作步骤:**
  1. [A] `cargo check -p langfuse-client 2>&1 | grep -E 'error|Finished|Checking'` → 期望包含: `Checking langfuse-client` 且无 error

---

### 场景 7：边界与回归

#### - [x] 7.1 langfuse-client 已加入 workspace members
- **来源:** 执行过程修正（crate 实际加入 workspace）
- **目的:** 确认根 Cargo.toml 包含 langfuse-client
- **操作步骤:**
  1. [A] `grep 'langfuse-client' Cargo.toml` → 期望包含: `"langfuse-client"` 在 members 列表中

#### - [x] 7.2 LangfuseClient 实现 Clone
- **来源:** 执行过程修正（session.rs 需要 clone client 给 Batcher）
- **目的:** 确认 Clone trait 已实现
- **操作步骤:**
  1. [A] `grep -B2 'pub struct LangfuseClient' langfuse-client/src/client.rs` → 期望包含: `#[derive(Clone)]` 或 `Clone`

#### - [x] 7.3 rust-agent-tui 测试通过
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认 TUI 集成后测试无回归
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep 'test result'` → 期望包含: `0 failed`

---

## 验收后清理

- [ ] [AUTO] 无需终止服务（本 feature 不涉及长期运行的服务进程）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 模块文件齐全 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | Cargo.toml 依赖正确 | 3 | 0 | ✅ |
| 场景 1 | 1.3 | lib.rs 重导出完整 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | IngestionEvent 10 变体 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | serde 内部标签配置 | 1 | 0 | ✅ |
| 场景 2 | 2.3 | 无 Option<Option<T>> | 1 | 0 | ✅ |
| 场景 2 | 2.4 | camelCase + deny_unknown_fields | 2 | 0 | ✅ |
| 场景 2 | 2.5 | ObservationType 10 变体 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | LangfuseClient 方法完整 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | V4 ingestion header | 1 | 0 | ✅ |
| 场景 3 | 3.3 | 重试逻辑 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | 4xx 不重试 | 1 | 0 | ✅ |
| 场景 3 | 3.5 | Basic Auth 认证 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | Batcher 方法完整 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | BatcherCommand 3 变体 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | 背压策略实现 | 2 | 0 | ✅ |
| 场景 4 | 4.4 | tokio::select! 事件循环 | 2 | 0 | ✅ |
| 场景 4 | 4.5 | 优雅关闭 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | 旧依赖已移除 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | 新依赖已添加 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | session.rs 仅用新 crate | 1 | 0 | ✅ |
| 场景 5 | 5.4 | tracer.rs 无旧事件类型 | 1 | 0 | ✅ |
| 场景 5 | 5.5 | tracer.rs 无 double Option | 1 | 0 | ✅ |
| 场景 5 | 5.6 | tracer.rs 新枚举变体 | 1 | 0 | ✅ |
| 场景 5 | 5.7 | 无 client.trace() | 1 | 0 | ✅ |
| 场景 5 | 5.8 | config.rs 未修改 | 1 | 0 | ✅ |
| 场景 5 | 5.9 | mod.rs 导出不变 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | 全 workspace 编译 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | langfuse-client 59 测试 | 1 | 0 | ✅ |
| 场景 6 | 6.3 | 全 workspace 测试 | 1 | 0 | ✅ |
| 场景 6 | 6.4 | 独立编译 | 1 | 0 | ✅ |
| 场景 7 | 7.1 | workspace members | 1 | 0 | ✅ |
| 场景 7 | 7.2 | LangfuseClient Clone | 1 | 0 | ✅ |
| 场景 7 | 7.3 | TUI 测试通过 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过
