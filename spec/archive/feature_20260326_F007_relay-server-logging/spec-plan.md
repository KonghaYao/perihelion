# Relay Server 日志增强 执行计划

**目标:** 为 Relay Server 补充认证失败、Web 客户端连接/断开、消息转发三类日志

**技术栈:** Rust 2021 / tracing 0.1 / axum 0.8

**设计文档:** ./spec-design.md

---

### Task 1: 认证失败 warn 日志

**涉及文件:**
- 修改: `rust-relay-server/src/main.rs`

**执行步骤:**
- [x] 在 `agent_ws_handler` 中，`validate_token` 返回 `Err(code)` 的分支，`return` 之前插入 warn 日志
  - 日志格式：`tracing::warn!(endpoint = "/agent/ws", "认证失败，返回 {}", code);`
- [x] 在 `web_ws_handler` 中，`validate_token` 返回 `Err(code)` 的分支，`return` 之前插入 warn 日志
  - 日志格式：`tracing::warn!(endpoint = "/web/ws", "认证失败，返回 {}", code);`
- [x] （可选）在 `agents_handler` 中同样位置插入 warn 日志
  - 日志格式：`tracing::warn!(endpoint = "/agents", "认证失败，返回 {}", code);`

**检查步骤:**
- [x] 确认认证失败 warn 日志已写入 agent_ws_handler
  - `grep -n 'warn!.*认证失败' rust-relay-server/src/main.rs`
  - 预期: 至少 2 行输出，包含 "/agent/ws" 和 "/web/ws"
- [x] cargo build 通过
  - `cargo build -p rust-relay-server 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error

---

### Task 2: Web 连接/断开与消息转发日志

**涉及文件:**
- 修改: `rust-relay-server/src/relay.rs`

**执行步骤:**
- [x] 在 `handle_web_management_ws` 中，`fetch_add` 之后插入连接 info 日志
  - 日志格式：`tracing::info!(active_web = state.active_web_conns.load(Ordering::Relaxed), "Web 管理端已连接");`
- [x] 在 `handle_web_management_ws` 函数末尾，`fetch_sub` 之后插入断开 info 日志
  - 日志格式：`tracing::info!(active_web = state.active_web_conns.load(Ordering::Relaxed), "Web 管理端已断开");`
- [x] 在 `handle_web_session_ws` 中，`txs.push(web_tx.clone());` 所在的块关闭后插入连接 info 日志
  - 日志格式：`tracing::info!(session = %session_id, active_web = state.active_web_conns.load(Ordering::Relaxed), "Web 会话端已连接");`
- [x] 在 `handle_web_session_ws` 函数末尾，`fetch_sub` 之后插入断开 info 日志
  - 日志格式：`tracing::info!(session = %session_id, "Web 会话端已断开");`
- [x] 在 `handle_agent_ws` 的 `Message::Text` 分支，`forward_to_web` 调用之前插入 trace 日志
  - 日志格式：`tracing::trace!(session = %sid2, bytes = text.len(), "Agent→Web 消息转发");`
  - 注意：此处变量为 `text`（`Utf8Bytes` 类型），`.len()` 返回字节数
- [x] 在 `handle_web_session_ws` 的 `Message::Text` 分支，`entry.agent_tx.send(text_str)` 之前插入 trace 日志
  - 日志格式：`tracing::trace!(session = %session_id, bytes = text_str.len(), "Web→Agent 消息转发");`

**检查步骤:**
- [x] 确认 Web 管理端连接/断开日志已写入
  - `grep -n 'Web 管理端已连接\|Web 管理端已断开' rust-relay-server/src/relay.rs`
  - 预期: 2 行输出
- [x] 确认 Web 会话端连接/断开日志已写入
  - `grep -n 'Web 会话端已连接\|Web 会话端已断开' rust-relay-server/src/relay.rs`
  - 预期: 2 行输出
- [x] 确认消息转发 trace 日志已写入
  - `grep -n 'Agent→Web 消息转发\|Web→Agent 消息转发' rust-relay-server/src/relay.rs`
  - 预期: 2 行输出
- [x] cargo build 通过
  - `cargo build -p rust-relay-server 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error

---

### Task 3: Relay Server 日志增强验收

**Prerequisites:**
- 启动命令: `RELAY_TOKEN=test-token RELAY_PORT=9998 RUST_LOG=warn cargo run -p rust-relay-server`
- trace 级验证需: `RELAY_TOKEN=test-token RELAY_PORT=9998 RUST_LOG=trace cargo run -p rust-relay-server`
- 需要工具: `websocat`（WebSocket 命令行客户端，`cargo install websocat` 安装）

**End-to-end verification:**

1. **编译与现有测试通过**
   - `cargo test -p rust-relay-server 2>&1 | tail -5`
   - Expected: 输出包含 "test result: ok" 且无 failed
   - On failure: check Task 1, 2 代码改动是否引入语法错误
   - [x] PASSED: 10 个协议序列化测试全部通过

2. **认证失败 warn 日志 — agent 端**
   - 启动服务器后执行: `websocat ws://127.0.0.1:9998/agent/ws?token=wrong-token 2>&1 | head -3`
   - Expected: 服务器日志出现 `WARN ... endpoint="/agent/ws" 认证失败，返回 401`
   - On failure: check Task 1 agent_ws_handler 改动
   - [x] PASSED: 日志输出 `WARN ... 认证失败，返回 401 Unauthorized endpoint="/agent/ws"`

3. **认证失败 warn 日志 — web 端**
   - 启动服务器后执行: `websocat ws://127.0.0.1:9998/web/ws?token=wrong-token 2>&1 | head -3`
   - Expected: 服务器日志出现 `WARN ... endpoint="/web/ws" 认证失败，返回 401`
   - On failure: check Task 1 web_ws_handler 改动
   - [x] PASSED: 日志输出 `WARN ... 认证失败，返回 401 Unauthorized endpoint="/web/ws"`

4. **Web 管理端连接/断开 info 日志**
   - 启动服务器（RUST_LOG=info），执行: `echo "" | websocat ws://127.0.0.1:9998/web/ws?token=test-token`
   - Expected: 服务器日志出现 `INFO ... active_web=1 Web 管理端已连接` 和 `INFO ... Web 管理端已断开`
   - On failure: check Task 2 handle_web_management_ws 改动
   - [x] PASSED: INFO Web 管理端已连接 active_web=1 和 INFO Web 管理端已断开 active_web=0

5. **Web 会话端连接 info 日志（需已有 Agent 连接建立 session）**
   - 先建立 Agent 连接获取 session_id，再用该 session_id 连接会话端 WS
   - Expected: 服务器日志出现 `INFO ... session=<id> active_web=N Web 会话端已连接`
   - On failure: check Task 2 handle_web_session_ws 改动
   - [x] PASSED: INFO Web 会话端已连接 session=<uuid> active_web=1；断开日志也正常

6. **消息转发 trace 日志**
   - 启动服务器（RUST_LOG=trace），Agent 与 Web 端建立连接后互发消息
   - Expected: 日志出现 `TRACE ... bytes=N Agent→Web 消息转发` 和 `TRACE ... bytes=N Web→Agent 消息转发`
   - On failure: check Task 2 handle_agent_ws 和 handle_web_session_ws 改动
   - [x] PASSED: TRACE Agent→Web 消息转发 bytes=15；TRACE Web→Agent 消息转发 bytes=15

7. **默认 info 级别无 trace 日志泄漏**
   - 启动服务器（RUST_LOG=info），正常收发消息 10 条
   - `RELAY_TOKEN=test-token RELAY_PORT=9998 RUST_LOG=info cargo run -p rust-relay-server 2>&1 | grep -c TRACE`
   - Expected: 输出为 0（无 TRACE 日志）
   - On failure: check trace! 宏级别是否正确
   - [x] PASSED: RUST_LOG=info 日志中 TRACE 行数为 0
