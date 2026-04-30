# Feature: 20260427_F001 - relay-removal

## 需求背景

Relay Server（远程控制）功能已废弃，未来将使用 ACP Link 统一输出方案替代，界面不由本项目考虑。当前 Relay 相关代码占用了大量维护成本，包括：

- `rust-relay-server` 独立 crate（axum WebSocket 服务端 + tokio-tungstenite 客户端库）
- 23 个前端文件（Preact + Signals + htm，124K）
- TUI 中 20+ 个文件与 Relay 集成（relay_panel、relay_ops、relay_state、relay_adapter、relay command、--remote-control CLI 参数等）
- 配置系统中的 `RemoteControl` 类型

这些代码全部需要移除，减少项目复杂度和维护负担。

## 目标

- 完整删除 `rust-relay-server` crate（含 server feature、client feature、web 前端）
- 清理 TUI 中所有 Relay 集成代码（面板、命令、事件转发、CLI 参数、配置类型）
- 清理 `rust-create-agent` 中仅为 Relay 服务的 `MessageAdded` 事件
- 更新 workspace 配置和全局文档

## 方案设计

### 删除范围清单

#### 1. 整体删除 `rust-relay-server/` 目录

包含 8 个 Rust 源文件和完整的 web/ 前端目录：

| 文件 | 说明 |
|------|------|
| `Cargo.toml` | crate 定义（server/client feature gates） |
| `src/main.rs` | axum Router 入口 |
| `src/lib.rs` | feature-gated 模块声明 |
| `src/protocol.rs` | 协议类型定义 |
| `src/protocol_types.rs` | 协议子类型 |
| `src/relay.rs` | RelayState + WebSocket handler |
| `src/auth.rs` | Token 验证 |
| `src/static_files.rs` | rust-embed 前端打包 |
| `src/client/mod.rs` | RelayClient（TUI 使用） |
| `web/` (23 文件, 124K) | Preact 前端 |

#### 2. 删除 TUI 中的 Relay 专用文件

| 文件 | 说明 |
|------|------|
| `src/app/relay_panel.rs` | /relay 面板状态 |
| `src/app/relay_ops.rs` | Relay 连接/断开/事件转发 |
| `src/app/relay_state.rs` | RelayState 子结构体 |
| `src/relay_adapter.rs` | AgentEvent → RelayMessage 适配器 |
| `src/ui/main_ui/panels/relay.rs` | /relay 面板 UI 渲染 |
| `src/command/relay.rs` | /relay 命令处理 |

#### 3. 修改 TUI 中引用 Relay 的文件

| 文件 | 修改内容 |
|------|----------|
| `Cargo.toml` | 移除 `rust-relay-server` 依赖 |
| `src/main.rs` | 移除 `--remote-control`、`--relay-token`、`--relay-name` CLI 参数；移除 Relay 初始化逻辑 |
| `src/app/mod.rs` | 移除 `RelayState` 字段和转发方法；App 结构体从 4 子结构体缩减为 3（去掉 RelayState） |
| `src/app/agent.rs` | 移除 Relay 事件转发（`relay_adapter.forward_event()`） |
| `src/app/agent_ops.rs` | 移除 Relay 启动/停止调用 |
| `src/app/panel_ops.rs` | 移除 relay panel 打开/关闭逻辑 |
| `src/app/hitl_ops.rs` | 移除 Relay 审批转发 |
| `src/app/ask_user_ops.rs` | 移除 Relay AskUser 转发 |
| `src/app/thread_ops.rs` | 移除 Relay ThreadReset 转发 |
| `src/ui/main_ui.rs` | 移除 relay panel 渲染分发 |
| `src/ui/main_ui/panels/mod.rs` | 移除 `mod relay` 声明 |
| `src/command/mod.rs` | 移除 relay 命令注册 |
| `src/event.rs` | 移除 relay 相关按键/事件处理 |
| `src/config/types.rs` | 移除 `RemoteControl` 结构体 |
| `src/config/mod.rs` | 移除 `remote_control` 配置字段读写 |
| `src/lib.rs` | 移除 relay 相关模块声明 |

#### 4. 修改 `rust-create-agent`

| 文件 | 修改内容 |
|------|----------|
| `src/agent/events.rs` | 评估 `MessageAdded` 变体——若仅被 Relay 使用则移除 |
| `src/agent/executor.rs` | 移除 `MessageAdded` 事件的 emit 调用 |

#### 5. Workspace 级别

| 文件 | 修改内容 |
|------|----------|
| `Cargo.toml` (根) | 从 `members` 中移除 `"rust-relay-server"` |
| `Cargo.lock` | 自动重新生成 |

### App 结构体变更

当前 App 由 4 个子结构体组成：

```
AppCore / AgentComm / RelayState / LangfuseState
```

移除后变为 3 个：

```
AppCore / AgentComm / LangfuseState
```

对外 API 通过转发方法保持不变（仅删除 Relay 相关的转发方法）。

### 配置变更

`settings.json` 中的 `remote_control` 字段不再使用。`RemoteControl` 类型定义和所有读写逻辑一并删除。已存储的配置文件中残留字段不影响功能（serde 反序列化默认忽略未知字段）。

## 实现要点

1. **编译验证**：删除后需确保 `cargo build` 和 `cargo test` 全量通过
2. **依赖清理**：移除 `rust-relay-server` 后，相关的 `axum`、`tokio-tungstenite`、`dashmap`、`rust-embed`、`subtle` 等依赖也应从 Cargo.lock 中清除（可能需要 `cargo update` 或手动检查）
3. **`MessageAdded` 事件评估**：需确认该事件是否仅用于 Relay 转发。若其他功能（如持久化）也依赖，则保留并移除 Relay 消费端即可
4. **Event 枚举清理**：TUI 层 `AgentEvent` 中如有 Relay 专有变体（如 `ApprovalNeeded` 的 Relay 转发路径），需一并清理
5. **无破坏性配置迁移**：已有 `settings.json` 中的 `remote_control` 字段无需主动清理，serde 反序列化自然忽略

## 约束一致性

- **架构约束变更**：workspace 从 4 crate 减为 3 crate，`rust-relay-server` 不再存在。依赖链变为 `rust-create-agent → rust-agent-middlewares → rust-agent-tui`
- **技术栈变更**：移除 axum（Web 框架）和前端 CDN 依赖（preact/htm/signals/marked/highlight.js/DOMPurify），不再需要 `Web 前端 CDN` 和 `Web 前端 Signal 订阅规则` 约束
- **部署方式变更**：不再有 `RELAY_TOKEN` 环境变量和 relay-server 启动命令
- **全局文档更新**：需同步更新 `spec/global/architecture.md`、`spec/global/constraints.md`、`spec/global/features.md` 和 `CLAUDE.md`

## 验收标准

- [ ] `rust-relay-server/` 目录完整删除
- [ ] TUI 中无 `relay`、`remote_control`、`RelayClient` 相关代码残留
- [ ] `--remote-control`、`--relay-token`、`--relay-name` CLI 参数移除
- [ ] `/relay` TUI 命令移除
- [ ] `RemoteControl` 配置类型移除
- [ ] workspace `Cargo.toml` 不再包含 `rust-relay-server`
- [ ] `cargo build` 通过
- [ ] `cargo test` 全量通过
- [ ] 全局文档（architecture.md、constraints.md、features.md、CLAUDE.md）已更新
