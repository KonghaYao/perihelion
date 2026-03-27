# Langfuse 领域

![Langfuse 领域概览](./images/langfuse-overview.png)

## 领域综述

Langfuse 领域负责 Agent 执行的全链路可观测性，将每次 LLM 调用、工具调用、对话 Session 结构化上报到 Langfuse 监控平台。所有 Langfuse 依赖和上报逻辑封装在 `rust-agent-tui` 层，不侵入 `rust-create-agent` 核心框架。

核心职责：
- Trace 管理：每次用户发送消息对应一个 Trace，多轮消息共享同一 Session
- Generation 追踪：LLM 调用记录（名称、模型、input/output、token 用量）
- Tool 观测：工具调用记录（ObservationType::Tool，嵌套在 Agent Observation 下）
- Agent Observation：每次 Agent 执行在 Trace 下创建一个 Agent 类型 Observation 包裹整个循环
- Session 生命周期：LangfuseSession 与 SQLite Thread 一一对应，切换/新建 Thread 时重置

## 核心流程

### 观测层级结构

```
Langfuse Session (session_id = thread_id)
  └── Trace (trace_id, name="agent-run")
        └── Observation(type=Agent, name="Agent")
              ├── Observation(type=Generation, name="ChatAnthropic"/"ChatOpenAI")
              ├── Observation(type=Tool, name=tool_name)
              ├── Observation(type=Generation, ...)
              └── Observation(type=Tool, ...)
```

### Session 生命周期管理

```
App 启动: langfuse_session = None（懒加载）

submit_message():
  1. ensure_thread_id() → thread_id = UUID
  2. if langfuse_session.is_none():
       LangfuseSession::new(config, session_id=thread_id)  ← 首轮创建
  3. LangfuseTracer::new(session.clone())  ← 每轮新 trace_id，共享 session

new_thread() / open_thread():
  langfuse_session = None  ← 下次发消息时按新/历史 thread_id 重建
```

### 事件→观测映射

| AgentEvent | Langfuse 对象 | 携带数据 |
|-----------|-------------|---------|
| submit_message() | Trace + Agent Observation 开始 | input、session_id |
| LlmCallStart | Generation 创建 | input messages |
| LlmCallEnd | Generation 更新 | model、output、usage |
| ToolStart | Tool Observation 创建 | name、input |
| ToolEnd | Tool Observation 更新 | output、is_error |
| Done | Trace + Agent Observation 结束 | final_answer |

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 客户端库 | langfuse-ergonomic 0.6.3（Batcher 异步批量上报） |
| 背压策略 | BackpressurePolicy::Drop，避免 OOM |
| 线程安全 | `Arc<parking_lot::Mutex<LangfuseTracer>>`，FnEventHandler 闭包与主线程共享 |
| Session 级别 | LangfuseSession 持有 Arc<LangfuseClient> + Arc<Batcher>，跨多轮复用 |
| Observation 类型 | Agent/Tool/Generation via IngestionEventOneOf8（observation-create API） |
| 命名约定 | Generation 名称：ChatOpenAI / ChatAnthropic（与 LangChain 约定一致） |
| 配置方式 | 环境变量 LANGFUSE_PUBLIC_KEY/SECRET_KEY/HOST，未配置时静默跳过 |
| Hook 扩展 | AgentEvent 新增 LlmCallStart/LlmCallEnd 两个变体（向后兼容，调用方加 `_ => {}` 即可）|

## Feature 附录

### feature_20260324_F001_rust-langfuse-client
**摘要:** Langfuse 客户端早期探索（无设计文档）
**关键决策:** — （早期探索，无正式设计）
**归档:** [链接](../../archive/feature_20260324_F001_rust-langfuse-client/)
**归档日期:** 2026-03-27

### feature_20260324_F001_langfuse-tui-monitoring
**摘要:** TUI 层接入 Langfuse 全链路追踪
**关键决策:**
- 侵入最小化：rust-create-agent 仅新增 LlmCallStart/LlmCallEnd 两个 AgentEvent 变体
- Batcher 生命周期：每次 submit_message 创建新 Batcher（Done 后 Drop 触发 flush）
- 工具调用 pending_span FIFO 匹配（工具串行执行，按顺序关联 ToolStart/End）
- 依赖隔离：langfuse-ergonomic 只在 rust-agent-tui 的 Cargo.toml 引入
**归档:** [链接](../../archive/feature_20260324_F001_langfuse-tui-monitoring/)
**归档日期:** 2026-03-27

### feature_20260325_F001_tui-langfuse-session
**摘要:** Thread 级 LangfuseSession 使多轮消息归属同一 Session
**关键决策:**
- LangfuseSession 持有共享 client/batcher，生命周期提升到 Thread 级
- LangfuseTracer 每轮新建（独立 trace_id），共享 session
- 打开历史 Thread 时重置 session，下次发消息用历史 thread_id 创建新 Session
**归档:** [链接](../../archive/feature_20260325_F001_tui-langfuse-session/)
**归档日期:** 2026-03-27

### feature_20260325_F001_langfuse-nested-subagent-trace
**摘要:** Langfuse 嵌套子 Agent 追踪迭代探索（无设计文档）
**关键决策:** — （迭代探索版本）
**归档:** [链接](../../archive/feature_20260325_F001_langfuse-nested-subagent-trace/)
**归档日期:** 2026-03-27

### feature_20260325_F001_langfuse-subagent-nesting
**摘要:** Langfuse 子 Agent 嵌套追踪迭代探索（无设计文档）
**关键决策:** — （迭代探索版本）
**归档:** [链接](../../archive/feature_20260325_F001_langfuse-subagent-nesting/)
**归档日期:** 2026-03-27

### feature_20260325_F003_langfuse-observation-types
**摘要:** 规范化 Langfuse 观测层级与类型命名
**关键决策:**
- Agent Observation 包裹整个 ReAct 循环，所有 Generation/Tool 挂在其下
- 工具调用从 span-create 切换到 observation-create（ObservationType::Tool）
- Generation 名称改为 ChatOpenAI / ChatAnthropic（provider_name 从 LlmProvider::display_name 取）
- parent_observation_id 统一指向 agent_span_id
**归档:** [链接](../../archive/feature_20260325_F003_langfuse-observation-types/)
**归档日期:** 2026-03-27

### feature_20260325_F004_subagent-langfuse-nesting
**摘要:** 子 Agent Langfuse 嵌套追踪最终迭代（无设计文档）
**关键决策:** — （迭代探索版本）
**归档:** [链接](../../archive/feature_20260325_F004_subagent-langfuse-nesting/)
**归档日期:** 2026-03-27

---

## 相关 Feature
- → [tui.md#feature_20260324_F001_langfuse-tui-monitoring](./tui.md#feature_20260324_F001_langfuse-tui-monitoring) — TUI 集成点在 app/agent.rs
- → [agent.md#feature_20260326_F009_relay-message-id-propagation](./agent.md#feature_20260326_F009_relay-message-id-propagation) — message_id 字段由 F006 引入，Langfuse 观测依赖此 ID
