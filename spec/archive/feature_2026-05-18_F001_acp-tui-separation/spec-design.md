# Feature: 2026-05-18_F001 - ACP/TUI 分层解耦

## 需求背景

当前 `peri-tui` 是单一 binary crate，内部同时承载 TUI 模式（ratatui 终端界面）和 ACP 模式（stdio JSON-RPC 服务端）。两种模式共用 `build_bare_agent()` 构建 Agent，但 event handler、interaction broker、LSP、Langfuse、hooks 等组装逻辑在 TUI 层与 ACP 层各有重复实现。

问题：
- **TUI 直接依赖 `peri-agent` 和 `peri-middlewares`**，构建 middleware chain、注册工具、处理事件——本质上是"前端直接连后端核心"
- **ACP 模式无法被 TUI 复用**：ACP 通过 stdio 对接 IDE，TUI 通过内部 mpsc channel 对接 Agent，两套通信路径互不相通
- **代码分散**：`agent.rs`（1049 行）同时包含 TUI 和 ACP 的 Agent 构建逻辑、event handler、Langfuse tracer、map_executor_event 等，职责重叠

目标：将 ACP 提升为独立服务层，TUI 降级为纯 ACP client 前端，二者通过 in-memory transport 通信。

## 目标

- **TUI 成为纯前端**：不直接依赖 `peri-agent`、`peri-middlewares`，通过 ACP 协议（in-memory transport）驱动 Agent
- **`peri-acp` 独立 crate**：承载 Session 管理、Agent 构建、Middleware Chain、LSP、Langfuse、Hooks、插件等全部后端逻辑
- **ACP 协议全覆盖**：所有 TUI 命令、HITL 审批、AskUser 问答均通过 ACP JSON-RPC 2.0 协议流转
- **保留 stdio ACP 路径**：`peri-acp` 同样可通过 stdio transport 对接 IDE，与内存 transport 共用同一个 `AcpTransport` trait

## 方案设计

### 1. Crate 边界与依赖

```
┌─────────────────────────────────────────────────┐
│ peri-tui (binary)                                │
│  ┌──────────────┐    ACP JSON-RPC     ┌────────┐│
│  │ TUI Frontend  │◄──in-memory──────►│ ACP    ││
│  │ (ratatui UI,  │  (mpsc channel)   │ Server ││
│  │  event loop,  │                    │        ││
│  │  panels)      │                    └────────┘│
│  └──────────────┘                         │     │
│                                            │     │
└────────────────────────────────────────────┼─────┘
                          peri-acp (lib)    │
                         ┌──────────────────┼─────┐
                         │  Session Manager │     │
                         │  Agent Builder   │     │
                         │  Middleware Chain│     │
                         │  Hooks/LSP/      │     │
                         │  Langfuse        │     │
                         │       │          │     │
                         │  peri-agent      │     │
                         │  peri-middlewares │     │
                         │  peri-lsp        │     │
                         │  langfuse-client │     │
                         └──────────────────┘─────┘
```

**依赖关系变更**：

| Crate | 原依赖 | 新依赖 |
|-------|--------|--------|
| `peri-tui` | peri-agent, peri-middlewares, peri-lsp, langfuse-client, peri-widgets | **peri-acp**, peri-widgets |
| `peri-acp` (新) | — | peri-agent, peri-middlewares, peri-lsp, langfuse-client, agent-client-protocol |

**`peri-tui` 保留职责**：ratatui UI 渲染、CommandRegistry（命令输入解析，转为 ACP 请求）、MessagePipeline（消费 SessionNotification）、HITL/AskUser 弹窗、crossterm 事件循环、面板组件。

**`peri-acp`（新 crate）职责**：

| 模块 | 来源 |
|------|------|
| ACP Server（JSON-RPC over in-memory transport） | 新建 |
| Session 生命周期管理 | 迁移自 `peri-tui/src/acp/session.rs` |
| Agent 构建（`build_acp_agent`） | 迁移自 `peri-tui/src/app/agent.rs:build_bare_agent()` |
| 权限桥接（RequestPermission + elicitation/create） | 重构自 `peri-tui/src/acp/broker.rs` |
| 事件映射（ExecutorEvent → SessionUpdate） | 迁移自 `peri-tui/src/acp/event_mapper.rs` |
| Hooks 系统（14 种事件） | 迁移自 `peri-tui` |
| LSP 中间件 | 迁移自 `peri-tui/src/app/agent.rs:586-609` |
| Langfuse tracer | 迁移自 `peri-tui/src/langfuse/` |
| 系统提示词构建 | 迁移自 `peri-tui/src/prompt.rs` |
| Provider/Model 解析 | 迁移自 `peri-tui/src/app/provider.rs` |
| ToolSearch 索引 | 迁移 |
| 上下文压缩 | 迁移 |

### 2. Transport 层

定义 `AcpTransport` trait，代表 ACP JSON-RPC 2.0 双向通道：

```rust
#[async_trait]
pub trait AcpTransport: Send + Sync {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value>;
    async fn send_notification(&self, method: &str, params: Value) -> Result<()>;
    async fn recv(&self) -> Option<IncomingMessage>;
}

pub enum IncomingMessage {
    Request { id: RequestId, method: String, params: Value },
    Notification { method: String, params: Value },
    Response { id: RequestId, result: Result<Value, AcpError> },
}
```

两种实现：
- **`MpscTransport`**：基于两对 `tokio::mpsc::unbounded_channel` 的内存通道，TUI 和 ACP 各持一端
- **`StdioTransport`**：保留当前 stdio 实现，用于 IDE 对接场景

```rust
pub fn mpsc_transport_pair() -> (MpscClientTransport, MpscServerTransport);
```

### 3. Session 与 Agent 生命周期

**Session 创建**：`SessionNew{cwd, model?}` → ACP 创建 ThreadStore entry，构建 Agent，返回 `session_id`。

**Prompt 执行流程**：

```
TUI                                  ACP Server
 │── Prompt{session_id, msg} ────────►
 │                                    ├─ executor.execute(input, state)
 │◄── SessionNotification: TextChunk ─│ (流式)
 │◄── SessionNotification: ToolStart ─│
 │◄── RequestPermission ──────────────│ (HITL)
 │── Respond(id, decision) ──────────►│
 │◄── SessionNotification: ToolEnd ───│
 │◄── SessionNotification: Done ──────│
```

**Session 操作**：`session/load`（加载历史）、`session/resume`（恢复暂停）、`$/cancel_request`（单请求取消）、`session/close`（销毁）。

### 4. Middleware Chain 迁移

`build_acp_agent(cfg: AcpAgentConfig)` 接收独立类型，不依赖 TUI 类型。中间件链顺序不变：

1. AgentsMdMiddleware → 2. AgentDefineMiddleware → 3. SkillsMiddleware → 4. SkillPreloadMiddleware → 5. FilesystemMiddleware → 6. GitAttributionMiddleware → 7. TerminalMiddleware → 8. WebMiddleware → 9. TodoMiddleware → 10. CronMiddleware → 11. HookMiddleware (per group) → 12. HumanInTheLoopMiddleware → 13. SubAgentMiddleware → 14. McpMiddleware → 15. ToolSearchMiddleware → [+ LspMiddleware]

TUI 层类型（`LlmProvider`、`PeriConfig`、`PromptFeatures`、`child_event_tx`、`FnEventHandler` 等）迁移到 `peri-acp`。

SubAgent 子事件通过 `peri-acp` 内部 session 事件总线分发，`source_agent_id` 标记源 agent。

### 5. 事件映射与 TUI 管线

**新流程**：

```
ExecutorEvent (peri-agent, in peri-acp)
  → map_executor_to_updates() → SessionUpdate
  → AcpTransport: SessionNotification(prompt_id, updates)
  → peri-tui: 接收 SessionNotification
  → MessagePipeline → View Models → Render
```

**TUI 不再定义 `AgentEvent` 枚举**。改为消费 ACP `SessionUpdate` 变体（`ToolCall`、`AssistantMessage`、`Reasoning`、`Status`、`TodoList`、`ContextUsage`、`StateSync`）。

`event_mapper.rs` 和 `map_executor_event()` 迁移到 `peri-acp`，TUI 只消费最终的 `SessionNotification`。

### 6. HITL 与 AskUser 协议分流

| 场景 | ACP 方法 | 弹窗 |
|------|---------|------|
| 工具审批 | `RequestPermission` RPC | HITL 弹窗 (Approve/Reject) |
| 用户问答 | `elicitation/create` RPC (unstable) | Elicitation 表单弹窗 |

**elicitation/create 流程**：`AskUserQuestion` 工具入参映射为 `ElicitationSchema`（单选 → `StringPropertySchema` + `oneOf`，多选 → `MultiSelectPropertySchema`，自定义输入 → `StringPropertySchema`），TUI 渲染表单 → 用户填写 → 返回 `CreateElicitationResponse{action: "accept", content: {...}}`。

`AcpTransportBroker` 实现 `UserInteractionBroker` trait，内部通过 `AcpTransport` 发送对应 RPC。

### 7. 命令映射

| TUI 命令 | ACP 请求 |
|---------|---------|
| `/model <alias>` | `session/set_model` |
| Shift+Tab | `session/set_mode` |
| `/history` | `session/list` |
| /history Enter | `session/load` |
| Ctrl+C | `$/cancel_request` |
| `/cost` | `session/usage` (unstable) |
| `/config` | TUI 本地直读 `~/.peri/settings.json`（不走 ACP） |

### 8. 迁移计划

| 阶段 | 内容 |
|------|------|
| 0 | `peri-acp` crate 脚手架（Cargo.toml、workspace member、模块目录） |
| 1 | Transport & Protocol 层（`AcpTransport` trait + `MpscTransport` 对） |
| 2 | Session & Agent 核心（SessionManager、`build_acp_agent`、配置迁移） |
| 3 | 事件 & 交互（event_mapper、AcpTransportBroker、Langfuse） |
| 4 | 中间件下沉（LSP、Hooks） |
| 5 | TUI 改造（移除 `peri-agent` 直接依赖，接入 `AcpTuiClient`，重构命令/事件/弹窗） |
| 6 | 清理 & 测试（清理旧代码、接口测试、端到端测试） |

## 实现要点

- **`AcpTransport` trait 是核心抽象**：同一 trait 支持 `MpscTransport`（TUI 内存通道）和 `StdioTransport`（IDE stdio 通道），确保 ACP binary 路径不受影响
- **`elicitation/unstable_elicitation` feature**：需在 `peri-acp` 的 `agent-client-protocol` 依赖中启用 `unstable_elicitation` feature
- **Session ID 和事件格式完全兼容 ACP 协议**：TUI 消费标准 `SessionNotification`，不定义自定义事件枚举，确保后端可替换（内存 ACP → 远程 ACP）
- **`build_bare_agent()` 整体迁移**：保持 middleware chain 构建逻辑不变，仅替换输入类型为 `AcpAgentConfig`
- **HITL/AskUser 弹窗驱动方式改变**：从 `TuiInteractionBroker` + `oneshot` channel 改为消费 `IncomingMessage::Request("RequestPermission")` / `IncomingMessage::Request("elicitation/create")`

## 约束一致性

本方案与 `spec/global/constraints.md` 和 `spec/global/architecture.md` 的约束对齐：

- **Workspace 多 crate 分层**：新增 `peri-acp` 位于 `peri-middlewares` 之上、`peri-tui` 之下，符合下层不依赖上层的规则
- **异步优先**：`AcpTransport` trait 使用 `#[async_trait]`，与现有风格一致
- **事件驱动通信**：TUI 与 ACP 通过 mpsc channel + JSON-RPC 通信，不共享可变状态，延续现有事件驱动模式
- **Middleware Chain 模式**：不变，全部在 `peri-acp` 内执行
- **工具系统**：`register_tool` 和 `ToolProvider` 机制不变，迁移到 `peri-acp`
- **编码规范**：新 crate 遵循 Rust 2021 + thiserror + tracing + async-trait 约定

**架构偏离**：无。这是对现有架构的重组，不改变约束规则。

## 验收标准

- [ ] `peri-acp` crate 编译通过，依赖 `peri-agent` + `peri-middlewares` + `peri-lsp` + `langfuse-client`
- [ ] `peri-tui` 不再直接依赖 `peri-agent`、`peri-middlewares`、`peri-lsp`、`langfuse-client`
- [ ] `mpsc_transport_pair()` 创建的 transport 对可完成完整的 ACP JSON-RPC 2.0 通信回合
- [ ] TUI 启动 → SessionNew → Agent 执行 → TextChunk/ToolStart/ToolEnd → Done 完整链路通
- [ ] HITL 审批：Bash 工具触发 → RequestPermission → TUI 弹窗 → 用户 Approve → Agent 继续
- [ ] AskUser 问答：Agent 调用 AskUserQuestion → elicitation/create → TUI 表单 → 用户填写 → Agent 收到答案
- [ ] `/model` 切换：`session/set_model` → Agent 下次 prompt 使用新模型
- [ ] Ctrl+C 中断：`$/cancel_request` → Agent 收到 `Interrupted`
- [ ] `/history` 面板：`session/list` → 显示 Thread 列表，Enter → `session/load` → 回放历史
- [ ] ACP stdio 模式不受影响：`peri-acp` 仍可作为 ACP binary 通过 stdio 对接 IDE
- [ ] 现有 TUI 测试（headless）通过
- [ ] 无编译 warning（clippy）
