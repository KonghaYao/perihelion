# ACP 状态变更后不发通知：ConfigOptionUpdate / AvailableCommandsUpdate / SessionInfoUpdate

**状态**：Resolved
**优先级**：中
**创建日期**：2026-05-19
**解决日期**：2026-05-19

## 问题描述

`acp_server.rs` 中 `session/set_mode`、`session/set_config_option`、`session/set_model` 等请求处理器在变更状态后只返回 response，不推送 `session/update` 通知。多客户端场景（如 IDE + TUI 同时连接）下，其他客户端无法感知变更，UI 控件不会更新。此外 `AvailableCommandsUpdate` 和 `SessionInfoUpdate` 在整个生命周期中从未发送过。

## 症状详情

### 缺口 1：set_mode / set_config_option 变更后不发通知

当前流程（`acp_server.rs`）：

```
Client → session/set_mode → 修改 permission_mode → 返回 response（无通知）
Client → session/set_config_option → 修改 mode/model/thinking → 返回 response（无通知）
```

ACP 规范要求变更后主动推送 `ConfigOptionUpdate` 通知，使所有连接的客户端同步状态：

```
Client → session/set_mode → 修改状态 → 返回 response + 推送 ConfigOptionUpdate 通知
```

### 缺口 2：AvailableCommandsUpdate 从未发送

ACP 标准 `SessionUpdate::AvailableCommandsUpdate` 变体从未被发送。IDE 客户端不知道 Agent 支持哪些斜杠命令（`/help`、`/compact`、`/cost` 等）。

可用命令列表在 `peri-tui/src/command/` 中定义。

### 缺口 3：SessionInfoUpdate 从未在 prompt 完成后发送

prompt 执行完成后不更新会话标题、状态或时间戳。IDE 无法展示有意义的会话标题。

## 涉及文件

- `peri-tui/src/acp_server.rs` — 请求处理器，需在 set_mode/set_config_option/set_model 后发送通知
- `peri-acp/src/event/mapper.rs` — 可能需要新增通知变体的构造辅助
- `peri-tui/src/command/` — 命令定义，AvailableCommandsUpdate 的数据源

## 修复摘要

### 缺口 1：ConfigOptionUpdate（已修复）

- **`acp_server.rs`**：`handle_request` 签名新增 `transport` 参数。`set_model`/`set_mode`/`set_config_option`/`set_thinking` 处理器在状态变更后调用 `send_config_option_update()`，构建当前配置快照并推送 `session/update` 通知。
- **`acp_stdio.rs`**：`set_mode`/`set_model`/`set_config_option` handler 通过 `cx.send_notification()` 推送 `SessionNotification<ConfigOptionUpdate>`。

### 缺口 2：AvailableCommandsUpdate（已修复）

- **`acp_server.rs`**：`session/new` 处理器在返回 response 后调用 `send_available_commands_update()`，推送 22 个可用斜杠命令。
- **`acp_stdio.rs`**：`session/new` handler 在 `responder.respond()` 后通过 `cx.send_notification()` 推送 `SessionNotification<AvailableCommandsUpdate>`。
- 命令列表定义于两个独立的 `build_available_commands()` / `build_stdio_available_commands()` 函数（无需依赖 LcRegistry）。

### 缺口 3：SessionInfoUpdate（已修复）

- **`acp_server.rs`**：prompt/compact background task 在 `send_response` 后调用 `send_session_info_update()`，推送带 `updatedAt`（ISO 8601 时间戳）的会话更新。
- **`acp_stdio.rs`**：prompt handler 在 `responder.respond()` 后通过 `event_sink.send_update()` 推送 `SessionInfoUpdate`。
- **`peri-acp/src/session/event_sink.rs`**：为 `StdioEventSink` 新增 `send_update(SessionUpdate)` 公开方法，支持执行后通知发送。
