# Relay Server 日志增强 人工验收清单

**生成时间:** 2026-03-26 08:15
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

> ℹ️ 所有验收项均可自动化执行，无需人工参与（0 个 [H] 步骤）。

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 编译 relay-server 二进制: `cargo build -p rust-relay-server 2>&1 | tail -3`
- [ ] [AUTO] 确认二进制存在: `test -f target/debug/relay-server && echo "OK"`

### 测试辅助脚本准备

- [ ] [AUTO] 写入 WebSocket 测试脚本（用于认证失败 + 连接测试）:
  ```bash
  cat > /tmp/ws_relay_test.py << 'PYEOF'
  import socket, base64, time, json, sys

  def ws_connect(host, port, path):
      s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
      s.connect((host, port))
      key = base64.b64encode(b'0' * 16).decode()
      req = f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
      s.sendall(req.encode())
      resp = s.recv(1024).decode(errors='replace')
      return s, resp.split('\r\n')[0]

  def ws_recv_frame(s):
      s.settimeout(2.0)
      try:
          header = s.recv(2)
          if len(header) < 2: return None
          payload_len = header[1] & 0x7F
          if payload_len == 126:
              payload_len = int.from_bytes(s.recv(2), 'big')
          data = b''
          while len(data) < payload_len:
              chunk = s.recv(payload_len - len(data))
              if not chunk: break
              data += chunk
          return data.decode(errors='replace')
      except: return None

  def ws_send_text(s, text):
      data = text.encode()
      mask = b'\x01\x02\x03\x04'
      masked = bytes([b ^ mask[i % 4] for i, b in enumerate(data)])
      frame = bytes([0x81, 0x80 | len(data)]) + mask + masked
      s.sendall(frame)

  MODE = sys.argv[1] if len(sys.argv) > 1 else 'help'
  PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 19998
  TOKEN = sys.argv[3] if len(sys.argv) > 3 else 'test-token'

  if MODE == 'auth_fail_agent':
      s, status = ws_connect('127.0.0.1', PORT, '/agent/ws?token=wrong-token')
      print(status)
      s.close()
  elif MODE == 'auth_fail_web':
      s, status = ws_connect('127.0.0.1', PORT, '/web/ws?token=wrong-token')
      print(status)
      s.close()
  elif MODE == 'mgmt_connect':
      s, status = ws_connect('127.0.0.1', PORT, f'/web/ws?token={TOKEN}')
      print(status)
      time.sleep(0.3)
      s.close()
      print('disconnected')
  elif MODE == 'session_connect':
      # Connect agent first to get session_id
      agent_s, _ = ws_connect('127.0.0.1', PORT, f'/agent/ws?token={TOKEN}&name=verify-agent')
      session_msg = ws_recv_frame(agent_s)
      session_id = json.loads(session_msg)['session_id']
      print('session_id:', session_id)
      # Connect web session WS
      web_s, status = ws_connect('127.0.0.1', PORT, f'/web/ws?token={TOKEN}&session={session_id}')
      print('web session:', status)
      time.sleep(0.3)
      web_s.close()
      time.sleep(0.1)
      agent_s.close()
      print('done')
  elif MODE == 'trace_forward':
      # Test message forwarding (trace log)
      agent_s, _ = ws_connect('127.0.0.1', PORT, f'/agent/ws?token={TOKEN}&name=trace-test')
      session_msg = ws_recv_frame(agent_s)
      session_id = json.loads(session_msg)['session_id']
      web_s, _ = ws_connect('127.0.0.1', PORT, f'/web/ws?token={TOKEN}&session={session_id}')
      time.sleep(0.2)
      # Agent sends to web
      data = '{"type":"ping"}'.encode()
      mask = b'\x00\x00\x00\x00'
      frame = bytes([0x81, 0x80 | len(data)]) + mask + data
      agent_s.sendall(frame)
      time.sleep(0.2)
      # Web sends to agent
      ws_send_text(web_s, '{"type":"pong"}')
      time.sleep(0.2)
      web_s.close()
      agent_s.close()
      print('trace_test_done')
  PYEOF
  echo "脚本写入完成"
  ```

---

## 验收项目

### 场景 1：代码完整性

#### - [x] 1.1 认证失败 warn! 日志代码已写入 main.rs

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n 'warn!.*认证失败' rust-relay-server/src/main.rs` → 期望: 输出至少 2 行，分别包含 "/agent/ws" 和 "/web/ws"
  2. [A] `grep -c 'warn!.*认证失败' rust-relay-server/src/main.rs` → 期望: 输出 `2` 或 `3`（含可选的 /agents 端点）
- **异常排查:**
  - 如果 0 行: 检查 `rust-relay-server/src/main.rs` 中 `validate_token` 分支是否已修改

#### - [x] 1.2 Web 连接/断开 info! 和消息转发 trace! 日志代码已写入 relay.rs

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -c 'Web 管理端已连接\|Web 管理端已断开\|Web 会话端已连接\|Web 会话端已断开' rust-relay-server/src/relay.rs` → 期望: 输出 `4`
  2. [A] `grep -c 'Agent→Web 消息转发\|Web→Agent 消息转发' rust-relay-server/src/relay.rs` → 期望: 输出 `2`
  3. [A] `grep -n 'tracing::trace!' rust-relay-server/src/relay.rs` → 期望: 2 行，含 `bytes =` 字段
- **异常排查:**
  - 如果计数不符: 检查 `rust-relay-server/src/relay.rs` 相应函数是否已添加日志行

---

### 场景 2：认证失败日志

> **前置条件:** 需要运行中的 relay-server（RUST_LOG=warn）。以下步骤需在服务器启动后执行。
>
> 启动命令: `RELAY_TOKEN=test-token RELAY_PORT=19998 RUST_LOG=warn ./target/debug/relay-server > /tmp/relay-verify-warn.log 2>&1 &`
> 健康检查: `curl -s http://127.0.0.1:19998/health` 返回 `OK`

#### - [x] 2.1 /agent/ws 使用错误 token 时输出认证失败 warn 日志

- **来源:** Task 3 验证项 2 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 启动服务器（若未运行）: `RELAY_TOKEN=test-token RELAY_PORT=19998 RUST_LOG=warn ./target/debug/relay-server > /tmp/relay-verify-warn.log 2>&1 & sleep 1 && curl -s http://127.0.0.1:19998/health` → 期望: 输出 `OK`
  2. [A] 发送错误 token 连接请求并检查日志: `python3 /tmp/ws_relay_test.py auth_fail_agent 19998; sleep 0.3; grep '认证失败.*agent/ws\|agent/ws.*认证失败' /tmp/relay-verify-warn.log` → 期望: 输出 1 行，含 `WARN` 和 `endpoint="/agent/ws"`
- **异常排查:**
  - 如果日志中无 WARN: 确认 `RUST_LOG=warn`，检查 main.rs agent_ws_handler 的 warn! 是否在正确位置（return 之前）

#### - [x] 2.2 /web/ws 使用错误 token 时输出认证失败 warn 日志

- **来源:** Task 3 验证项 3 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 发送错误 token 连接请求: `python3 /tmp/ws_relay_test.py auth_fail_web 19998; sleep 0.3` → 期望: 脚本打印 `HTTP/1.1 401 Unauthorized`
  2. [A] 检查服务器日志: `grep '认证失败.*web/ws\|web/ws.*认证失败' /tmp/relay-verify-warn.log` → 期望: 输出 1 行，含 `WARN` 和 `endpoint="/web/ws"`
- **异常排查:**
  - 如果无日志: 检查 main.rs web_ws_handler 对应的 warn! 是否写入

---

### 场景 3：Web 连接/断开日志

> **前置条件:** 服务器以 RUST_LOG=info 运行，日志输出到 /tmp/relay-verify-info.log
>
> 停止上一个实例: `pkill -f 'relay-server' 2>/dev/null; sleep 0.5`
> 重启命令: `RELAY_TOKEN=test-token RELAY_PORT=19998 RUST_LOG=info ./target/debug/relay-server > /tmp/relay-verify-info.log 2>&1 & sleep 1 && curl -s http://127.0.0.1:19998/health`

#### - [x] 3.1 Web 管理端 WebSocket 连接/断开时输出 info 日志（含 active_web 字段）

- **来源:** Task 3 验证项 4 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 停止并重启服务器（info 级）: `pkill -f 'relay-server' 2>/dev/null; sleep 0.5; RELAY_TOKEN=test-token RELAY_PORT=19998 RUST_LOG=info ./target/debug/relay-server > /tmp/relay-verify-info.log 2>&1 & sleep 1; curl -s http://127.0.0.1:19998/health` → 期望: `OK`
  2. [A] 建立并关闭管理端连接后检查日志: `python3 /tmp/ws_relay_test.py mgmt_connect 19998; sleep 0.3; grep 'Web 管理端已连接\|Web 管理端已断开' /tmp/relay-verify-info.log` → 期望: 2 行，分别含 `active_web=1` 和 `active_web=0`
- **异常排查:**
  - 如果只有 1 行（无断开）: 检查 handle_web_management_ws 末尾 fetch_sub 之后的 info! 位置
  - 如果无 active_web 字段: 检查 info! 宏中是否写了 `active_web = state.active_web_conns.load(Ordering::Relaxed)`

#### - [x] 3.2 Web 会话端 WebSocket 连接/断开时输出 info 日志（含 session 和 active_web 字段）

- **来源:** Task 3 验证项 5 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 建立 Agent + Web 会话端连接后检查日志: `python3 /tmp/ws_relay_test.py session_connect 19998; sleep 0.3; grep 'Web 会话端已连接\|Web 会话端已断开' /tmp/relay-verify-info.log` → 期望: 2 行，连接行含 `session=<uuid>` 和 `active_web=` 字段，断开行含 `session=<uuid>`
  2. [A] 同时确认 Agent 连接/断开日志仍正常（原有功能不受影响）: `grep 'Agent connected\|Agent disconnected' /tmp/relay-verify-info.log` → 期望: 2 行，含 session UUID 和 name 字段
- **异常排查:**
  - 如果无会话端日志: 检查 handle_web_session_ws 中 `txs.push` 块之后的 info! 是否写入
  - 如果 session 字段缺失: 检查 info! 宏中是否使用 `session = %session_id` 格式

---

### 场景 4：消息转发 trace 日志

> **前置条件:** 服务器以 RUST_LOG=trace 运行，日志输出到 /tmp/relay-verify-trace.log
>
> 停止并重启: `pkill -f 'relay-server' 2>/dev/null; sleep 0.5; RELAY_TOKEN=test-token RELAY_PORT=19998 RUST_LOG=trace ./target/debug/relay-server > /tmp/relay-verify-trace.log 2>&1 & sleep 1 && curl -s http://127.0.0.1:19998/health`

#### - [x] 4.1 RUST_LOG=trace 时 Agent↔Web 消息转发产生 trace 日志（含 bytes 字段）

- **来源:** Task 3 验证项 6 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 停止并重启服务器（trace 级）: `pkill -f 'relay-server' 2>/dev/null; sleep 0.5; RELAY_TOKEN=test-token RELAY_PORT=19998 RUST_LOG=trace ./target/debug/relay-server > /tmp/relay-verify-trace.log 2>&1 & sleep 1; curl -s http://127.0.0.1:19998/health` → 期望: `OK`
  2. [A] 执行消息转发测试并检查 trace 日志: `python3 /tmp/ws_relay_test.py trace_forward 19998; sleep 0.3; grep 'Agent→Web 消息转发\|Web→Agent 消息转发' /tmp/relay-verify-trace.log` → 期望: 2 行，分别含 `TRACE`、`bytes=15`、`session=<uuid>`
- **异常排查:**
  - 如果无 trace 日志: 确认 RUST_LOG=trace 是否生效（日志中应有大量 TRACE 行）
  - 如果 bytes 字段缺失: 检查 trace! 宏中 `bytes = text.len()` 或 `bytes = text_str.len()` 是否写入

#### - [x] 4.2 默认 RUST_LOG=info 时不输出 trace 日志（无泄漏）

- **来源:** Task 3 验证项 7 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 检查 info 级别日志中无 TRACE 行: `grep -c 'TRACE' /tmp/relay-verify-info.log` → 期望: 输出 `0`
- **异常排查:**
  - 如果有 TRACE 行: 检查 relay.rs 中使用的是 `tracing::trace!` 而非 `tracing::info!`

---

### 场景 5：构建与测试完整性

#### - [x] 5.1 cargo build 编译通过（无新依赖、无语法错误）

- **来源:** Task 1/2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-relay-server 2>&1 | tail -3` → 期望: 输出包含 `Finished` 且无 `error`
- **异常排查:**
  - 如果编译失败: 检查 main.rs 和 relay.rs 的 tracing! 宏语法，确认 `Ordering` 已导入（relay.rs 头部有 `use std::sync::atomic::Ordering`）

#### - [x] 5.2 cargo test 全量通过（无协议序列化测试破坏）

- **来源:** Task 3 验证项 1 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-relay-server --lib 2>&1 | tail -5` → 期望: 输出包含 `test result: ok` 且 `failed: 0`
- **异常排查:**
  - 如果测试失败: 日志改动不应影响协议序列化，检查是否误改了 protocol.rs 文件

---

## 验收后清理

- [ ] [AUTO] 停止测试服务器: `pkill -f 'relay-server' 2>/dev/null && echo "已停止" || echo "无运行实例"`
- [ ] [AUTO] 清理临时文件: `rm -f /tmp/relay-verify-warn.log /tmp/relay-verify-info.log /tmp/relay-verify-trace.log /tmp/ws_relay_test.py`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 代码完整性 | 1.1 | warn! 日志代码存在性 | 2 | 0 | ✅ | |
| 代码完整性 | 1.2 | info!/trace! 日志代码存在性 | 3 | 0 | ✅ | |
| 认证失败日志 | 2.1 | /agent/ws 认证失败 warn 日志 | 2 | 0 | ✅ | |
| 认证失败日志 | 2.2 | /web/ws 认证失败 warn 日志 | 2 | 0 | ✅ | |
| Web 连接/断开日志 | 3.1 | Web 管理端连接/断开 info 日志 | 2 | 0 | ✅ | |
| Web 连接/断开日志 | 3.2 | Web 会话端连接/断开 info 日志 | 2 | 0 | ✅ | |
| 消息转发 trace 日志 | 4.1 | Agent↔Web 消息转发 trace 日志 | 2 | 0 | ✅ | |
| 消息转发 trace 日志 | 4.2 | 默认 info 无 trace 泄漏 | 1 | 0 | ✅ | |
| 构建与测试 | 5.1 | cargo build 通过 | 1 | 0 | ✅ | |
| 构建与测试 | 5.2 | cargo test 全量通过 | 1 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
