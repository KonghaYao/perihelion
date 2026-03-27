# Feature: 20260326_F007 - relay-server-logging

## 需求背景

Relay Server 目前仅对 Agent 连接/断开有日志记录，但以下三类场景缺乏可观测性：

1. **Web 客户端无连接日志**：Web 管理端（`handle_web_management_ws`）和 Web 会话端（`handle_web_session_ws`）的连接建立与断开完全没有日志，出现多客户端连接问题时无从排查。
2. **认证失败无日志**：`agent_ws_handler` / `web_ws_handler` 的 token 校验失败仅返回 HTTP 401，未记录任何日志，安全审计无法追踪非法访问尝试。
3. **消息转发无 trace**：Agent→Web、Web→Agent 的消息转发路径没有 trace 级日志，深度调试时无法观察消息流量。

## 目标

- 补充 Web 管理端 / 会话端连接 & 断开日志（`info` 级）
- 补充认证失败日志（`warn` 级，含端点路径标识）
- 补充消息转发的 trace 日志（`trace` 级，生产环境默认不输出）

## 方案设计

### 改动文件

仅修改 `rust-relay-server/src/` 下两个文件：`main.rs`（认证失败日志）和 `relay.rs`（连接日志 + 消息转发日志）。

### 改动点详情

#### 1. 认证失败日志（`main.rs`）

在 `agent_ws_handler` 和 `web_ws_handler` 的 `validate_token` 返回 `Err` 时添加 `warn!` 日志：

```rust
// agent_ws_handler
if let Err(code) = auth::validate_token(params.token.as_deref(), &state.token) {
    tracing::warn!(endpoint = "/agent/ws", "认证失败，返回 {}", code);
    return code.into_response();
}

// web_ws_handler
if let Err(code) = auth::validate_token(params.token.as_deref(), &state.token) {
    tracing::warn!(endpoint = "/web/ws", "认证失败，返回 {}", code);
    return code.into_response();
}
```

> `/agents` REST 端点认证失败也建议加一行 `warn!`，但由于是只读接口，优先级较低，可选。

#### 2. Web 管理端连接/断开（`relay.rs: handle_web_management_ws`）

在连接计数递增之后和函数返回之前分别插入日志：

```rust
// 连接建立（active_web_conns 已递增后）
tracing::info!(
    active_web = state.active_web_conns.load(Ordering::Relaxed),
    "Web 管理端已连接"
);

// 函数末尾断开前（active_web_conns 递减后）
tracing::info!(
    active_web = state.active_web_conns.load(Ordering::Relaxed),
    "Web 管理端已断开"
);
```

#### 3. Web 会话端连接/断开（`relay.rs: handle_web_session_ws`）

在通过连接上限检查、成功注册 `web_tx` 之后插入连接日志；在函数末尾断开后插入断开日志：

```rust
// 连接建立（txs.push 之后）
tracing::info!(
    session = %session_id,
    active_web = state.active_web_conns.load(Ordering::Relaxed),
    "Web 会话端已连接"
);

// 函数末尾（active_web_conns 递减后）
tracing::info!(session = %session_id, "Web 会话端已断开");
```

#### 4. 消息转发 trace 日志（`relay.rs`）

**Agent → Web**（`handle_agent_ws` 收到 `Message::Text` 之后、`forward_to_web` 调用之前）：

```rust
tracing::trace!(
    session = %sid2,
    bytes = text.len(),
    "Agent→Web 消息转发"
);
```

**Web → Agent**（`handle_web_session_ws` 收到 `Message::Text` 之后、`agent_tx.send` 调用之前）：

```rust
tracing::trace!(
    session = %session_id,
    bytes = text_str.len(),
    "Web→Agent 消息转发"
);
```

### 日志级别汇总

| 事件 | 级别 | 触发位置 |
|------|------|----------|
| 认证失败 | `warn` | `main.rs` agent/web ws handler |
| Web 管理端连接/断开 | `info` | `relay.rs handle_web_management_ws` |
| Web 会话端连接/断开 | `info` | `relay.rs handle_web_session_ws` |
| 消息转发（Agent→Web） | `trace` | `relay.rs handle_agent_ws` |
| 消息转发（Web→Agent） | `trace` | `relay.rs handle_web_session_ws` |

（已有日志保持不变：Agent 连接/断开 `info`，连接数超限 `warn`，Session 清理 `debug`）

## 实现要点

- 所有日志使用 `tracing` 宏（`warn!`/`info!`/`trace!`），不使用 `println!`，与现有代码风格一致（constraints.md 日志规范）。
- `active_web_conns` 计数在 `fetch_add` / `fetch_sub` 之后读取，确保日志数值准确反映操作后状态。
- `trace` 级消息转发日志仅记录字节数（`text.len()`），不打印消息内容，避免敏感数据泄漏。
- 改动范围极小：仅 `main.rs` + `relay.rs` 两个文件，无新依赖，无接口变更。

## 约束一致性

- 使用 `tracing` 宏，符合 constraints.md「日志规范：使用 tracing 宏，不直接使用 println!/eprintln!」。
- 不引入新 crate，符合 constraints.md 技术栈约束（已有 `tracing 0.1`）。
- 只修改应用层文件（`rust-relay-server/`），不触及核心 lib crate，符合 Workspace 分层约束。

## 验收标准

- [ ] 启动 Relay Server 并用错误 token 连接 `/agent/ws`，日志出现 `WARN ... 认证失败，返回 401`（endpoint="/agent/ws"）
- [ ] 启动 Relay Server 并用错误 token 连接 `/web/ws`，日志出现 `WARN ... 认证失败，返回 401`（endpoint="/web/ws"）
- [ ] 浏览器打开管理端 WebSocket，日志出现 `INFO ... Web 管理端已连接`（含 active_web 字段）；关闭后出现 `INFO ... Web 管理端已断开`
- [ ] 浏览器打开会话端 WebSocket（含 session 参数），日志出现 `INFO ... Web 会话端已连接`（含 session 和 active_web 字段）；关闭后出现 `INFO ... Web 会话端已断开`
- [ ] `RUST_LOG=trace` 启动后，Agent 发送消息时日志出现 `TRACE ... Agent→Web 消息转发`（含 session 和 bytes 字段）
- [ ] `RUST_LOG=trace` 启动后，Web 端发送消息时日志出现 `TRACE ... Web→Agent 消息转发`（含 session 和 bytes 字段）
- [ ] 默认 `RUST_LOG=info` 不出现 trace 日志，不影响正常运行性能
- [ ] `cargo test -p rust-relay-server` 全量通过（无 protocol 序列化测试破坏）
