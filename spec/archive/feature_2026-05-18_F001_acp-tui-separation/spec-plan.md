# ACP/TUI 分层解耦 - 执行计划

**目标:** 将 ACP 提升为独立 `peri-acp` 服务层 crate，TUI 降级为纯 ACP client 前端，二者通过 in-memory transport 通信

**架构:** 新建 `peri-acp` crate（依赖 peri-agent + peri-middlewares + peri-lsp + langfuse-client），定义 `AcpTransport` trait，TUI 通过 `MpscTransport` 与 `peri-acp` 通信，ACP JSON-RPC 2.0 协议覆盖全部命令/HITL/AskUser。保留 stdio ACP 路径不变。

**技术栈:** Rust 2021, tokio, async-trait, serde_json, agent-client-protocol 0.11, ratatui, parking_lot, tracing

**设计文档:** `spec/feature_2026-05-18_F001_acp-tui-separation/spec-design.md`

---

## 改动总览

本次改动新建 `peri-acp` crate（13 个模块目录），将 `peri-tui` 中与 ACP 和 Agent 构建相关的 ~8 个模块整体迁移到 `peri-acp`，包括：SessionManager、Agent 构建（build_bare_agent）、Langfuse、系统提示词、Provider/Model 解析、事件映射、权限桥接、Hooks、LSP。TUI 端移除对 `peri-agent`/`peri-middlewares`/`peri-lsp`/`langfuse-client` 的直接依赖，改为通过 `AcpTuiClient`（MpscTransport 的 TUI 端封装）消费 ACP `SessionNotification`。Task 之间有严格顺序依赖：Task 1（脚手架）→ Task 2（Transport）→ Task 3（Session/Agent）→ Task 4（事件/交互/Langfuse）→ Task 5（LSP/Hooks 下沉）→ Task 6（TUI 改造）→ Task 7（清理/测试）。

---

## 任务索引

| Task | 名称 | 概要 |
|------|------|------|
| 0 | 环境准备 | 验证构建工具链和测试环境 |
| 1 | peri-acp 脚手架 | 新建 crate，添加 workspace member，创建模块目录骨架 |
| 2 | Transport & Protocol 层 | 定义 AcpTransport trait、MpscTransport 对、IncomingMessage 枚举 |
| 3 | Session & Agent 核心 | 迁移 SessionManager、build_bare_agent()、Provider/Model 解析、系统提示词 |
| 4 | 事件 & 交互 & Langfuse | 迁移 event_mapper、AcpTransportBroker、Langfuse 追踪 |
| 5 | 中间件下沉 (LSP, Hooks) | 将 LSP/Hooks 集成到 peri-acp 的 Agent 构建 |
| 6 | TUI 改造 | ✅ **完成** — 12 个子步骤全部执行，1917 测试通过 |
| **6-detail** | **TUI 改造细化计划** | 详见 [spec-plan-task-6-detail.md](./spec-plan-task-6-detail.md) |
| 7 | 清理 & 测试 | ⚠️ **待执行** — 删除旧依赖、清理死代码、E2E 验证 |
| **验收** | **端到端验证** | 全量测试 + 11 个验证场景 |

---

## Task 6 完成状态

**已完成 (6-a ~ 6-l)**:
- ✅ 6-a: `acp_server.rs` 创建，`main.rs` 中 spawn ACP server + transport pair
- ✅ 6-b: 删除 `peri-tui/src/acp/` 目录；`langfuse/` 改为 bridge re-export
- ✅ 6-c: `App.acp_client` 字段，`AgentComm.acp_notification_rx` 字段
- ✅ 6-d: `agent_submit.rs` → `acp_client.new_session()` + `acp_client.prompt()`
- ✅ 6-e: `agent_ops.rs` → `handle_acp_notification()` bridge (AcpNotification → AgentEvent → handle_agent_event)
- ✅ 6-e: `handle_acp_request_permission()` + `handle_acp_elicitation()` for HITL/AskUser
- ✅ 6-f: `poll_agent()` → reads `acp_notification_rx` (primary) + `agent_rx` (fallback)
- ✅ 6-g: `interrupt()` → `acp_client.cancel()` (primary) + `cancel_token` (fallback)
- ✅ 6-h: `/model`、Ctrl+C 通过 ACP 路由
- ⚠️ 6-i: `agent.rs` 死代码 (`AgentRunConfig`, `build_bare_agent`, `run_universal_agent`) 保留待 Task 7 清理
- ❌ 6-j: 旧依赖 (`peri-agent`, `peri-middlewares`, `peri-lsp`, `langfuse-client`) 仍在 Cargo.toml — 需更新 42 个文件的 imports 方可移除
- ✅ 6-k: `cargo check --workspace` 0 errors
- ✅ 6-l: `cargo test --workspace` 1917 passed, 0 failed

**Task 7 剩余工作**:
1. 从 `peri-tui/Cargo.toml` 移除 `peri-agent`/`peri-middlewares`/`peri-lsp`/`langfuse-client` 直接依赖
2. 更新 42 个文件的 imports (`use peri_agent::*` → `use peri_acp::*`)
3. 清理 `agent.rs` 死代码 (`AgentRunConfig`, `BareAgentConfig`, `BareAgentOutput`, `build_bare_agent`, `run_universal_agent`)
4. 删除 `interaction_broker.rs`（仅被死代码引用）
5. 迁移 `main_acp.rs` stdio 入口到 peri-acp binary
6. E2E 验证 11 个场景

