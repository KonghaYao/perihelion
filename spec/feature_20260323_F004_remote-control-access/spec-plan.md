# 远程控制访问 执行计划

**目标:** 通过中心化 Relay Server 实现多 Agent 远程访问控制，Web 前端 Tab 切换操控，等同 TUI 全功能

**技术栈:** Rust (Axum, tokio-tungstenite, DashMap, rust-embed, serde), HTML + Vanilla JS

**设计文档:** ./spec-design.md

---

### Task 1: rust-relay-server Crate 骨架

**涉及文件:**
- 新建: `rust-relay-server/Cargo.toml`
- 新建: `rust-relay-server/src/main.rs`
- 新建: `rust-relay-server/src/lib.rs`
- 修改: `Cargo.toml`（workspace members）

**执行步骤:**
- [x] 在 workspace 根 `Cargo.toml` 的 members 中添加 `"rust-relay-server"`
- [x] 创建 `rust-relay-server/Cargo.toml`，配置两个 feature flag：
  - `server`（默认）：依赖 axum, axum-extra, tokio-tungstenite, tower-http, rust-embed, dashmap, tracing
  - `client`：仅依赖 tokio-tungstenite, tokio, serde, serde_json, tracing
  - 共享依赖（无条件）：serde, serde_json, uuid, tokio
  - 依赖 `rust-create-agent`（path 引用，获取 AgentEvent 类型）
- [x] 创建 `rust-relay-server/src/lib.rs`，使用条件编译导出模块：
  - `#[cfg(feature = "server")] pub mod relay;`
  - `#[cfg(feature = "server")] pub mod auth;`
  - `#[cfg(feature = "server")] pub mod static_files;`
  - `pub mod protocol;`（共享消息类型，server 和 client 都用）
  - `#[cfg(feature = "client")] pub mod client;`
- [x] 创建 `rust-relay-server/src/main.rs`，仅在 `feature = "server"` 下编译：
  - `#[cfg(not(feature = "server"))] compile_error!("需要 server feature");`
  - 读取环境变量 `RELAY_TOKEN`（必填）、`RELAY_PORT`（默认 8080）
  - 初始化 tracing
  - 占位：启动 Axum server（后续 Task 填充）

**检查步骤:**
- [x] workspace 构建成功
  - `cargo build -p rust-relay-server`
  - 预期: 编译通过无错误
- [x] client feature 单独编译成功
  - `cargo build -p rust-relay-server --no-default-features --features client`
  - 预期: 编译通过，不引入 axum 依赖
- [x] workspace 全量构建成功
  - `cargo build`
  - 预期: 所有 crate 编译通过

---

### Task 2: 消息协议类型与 AgentEvent 序列化

**涉及文件:**
- 新建: `rust-relay-server/src/protocol.rs`
- 修改: `rust-create-agent/src/agent/events.rs`（为 AgentEvent 添加 Serialize 派生）

**执行步骤:**
- [x] 在 `rust-create-agent/src/agent/events.rs` 中为 `AgentEvent` 及其内部类型添加 `#[derive(serde::Serialize, serde::Deserialize)]`
  - 检查 AgentEvent 的所有变体字段类型是否都已支持 Serialize（serde_json::Value 已支持，BaseMessage 检查是否需要补充）
  - 若 BaseMessage 或 ContentBlock 缺少 Serialize 派生，需补充
- [x] 创建 `rust-relay-server/src/protocol.rs`，定义通信协议类型：
  - `RelayMessage` 枚举（Agent→Relay→Web 方向）：AgentEvent, SessionId, Ping
  - `WebMessage` 枚举（Web→Relay→Agent 方向）：UserInput, HitlDecision, AskUserResponse, ClearThread, Pong
  - `BroadcastMessage` 枚举（Relay→所有Web）：AgentOnline, AgentOffline, AgentsList
  - `RelayError` 枚举：SessionNotFound, AuthFailed
  - 所有类型派生 `Serialize, Deserialize`，使用 `#[serde(tag = "type", rename_all = "snake_case")]` 实现 JSON 类型标签
- [x] 定义 `AgentInfo` 结构体：`session_id: String, name: Option<String>, connected_at: String`

**检查步骤:**
- [x] AgentEvent 序列化往返正确
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: 现有测试仍通过
- [x] protocol 类型编译通过
  - `cargo build -p rust-relay-server`
  - 预期: 编译通过
- [x] RelayMessage JSON 序列化格式正确
  - `cargo test -p rust-relay-server --lib 2>&1 | tail -5`
  - 预期: 测试通过，JSON 包含 `"type": "agent_event"` 等标签

---

### Task 3: Relay Server 核心实现

**涉及文件:**
- 新建: `rust-relay-server/src/relay.rs`
- 新建: `rust-relay-server/src/auth.rs`
- 新建: `rust-relay-server/src/static_files.rs`
- 修改: `rust-relay-server/src/main.rs`

**执行步骤:**
- [x] 实现 `auth.rs`：
  - `validate_token(query_params, expected_token) -> Result<()>` 函数
  - 从 WebSocket upgrade 请求的 query 参数中提取 `token` 并验证
- [x] 实现 `relay.rs` 核心数据结构：
  - `SessionEntry { agent_tx, web_txs: Vec<UnboundedSender>, name, created_at, last_active }`
  - `RelayState { sessions: DashMap<String, SessionEntry>, broadcast_txs: RwLock<Vec<UnboundedSender>>, token: String }`
  - `RelayState::new(token) -> Arc<Self>`
- [x] 实现 Agent WS 处理 `handle_agent_ws(ws, state, token, name)`：
  - 验证 token
  - 生成 UUID v4 session_id
  - 将 agent_tx 注册到 sessions
  - 返回 `{ "type": "session_id", "session_id": "..." }` 给 Agent
  - 广播 `agent_online` 给所有 broadcast_txs
  - 启动双循环：读 Agent 消息 → 转发给该 session 的 web_txs；读 agent_tx 的消息 → 发给 Agent WS
  - 连接断开时：从 sessions 移除（或保留 30 分钟），广播 `agent_offline`
- [x] 实现 Web WS 处理：
  - 不带 `session` 参数：管理 WS，注册到 broadcast_txs，下发 `agents_list`
  - 带 `session` 参数：专属 WS，注册到对应 SessionEntry 的 web_txs，接收该 Agent 的事件；读 Web 消息 → 转发给 agent_tx
  - 连接断开时：从 web_txs/broadcast_txs 中移除
- [x] 实现 ping/pong 心跳：
  - 每 30s 发送 `{"type":"ping"}`，超时 60s 未收到 pong 则断开
- [x] 实现 `/agents` HTTP 接口：
  - 验证 query 中的 token
  - 遍历 sessions，返回 `[{ session_id, name, connected_at }]` JSON 数组
- [x] 实现 `/health` HTTP 接口：返回 `200 OK`
- [x] 实现 `static_files.rs`：
  - 使用 `rust-embed` 内嵌 `web/` 目录
  - 对 `/web/` 路径返回 index.html，对 `/web/<file>` 返回对应文件
  - 设置正确的 Content-Type
- [x] 更新 `main.rs`：
  - 组装 Axum Router：`/agent/ws`, `/web/ws`, `/agents`, `/health`, `/web/*`
  - 绑定 `0.0.0.0:PORT` 启动服务
  - 启动后打印：`Relay Server 已启动，监听 0.0.0.0:{PORT}`
- [x] 实现 session 超时清理：
  - tokio::spawn 定时任务（每 5 分钟检查），清理 Agent 断开超过 30 分钟的 session

**检查步骤:**
- [x] Relay Server 可启动
  - `RELAY_TOKEN=test cargo run -p rust-relay-server &; sleep 2; curl -s http://localhost:8080/health; kill %1`
  - 预期: 返回 200 OK
- [x] /agents 接口返回空列表
  - `RELAY_TOKEN=test cargo run -p rust-relay-server &; sleep 2; curl -s "http://localhost:8080/agents?token=test"; kill %1`
  - 预期: 返回 `[]` 或 `{"agents":[]}`
- [x] token 验证拒绝无效请求
  - `RELAY_TOKEN=test cargo run -p rust-relay-server &; sleep 2; curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/agents?token=wrong"; kill %1`
  - 预期: 返回 401 或 403
- [x] 编译无警告
  - `cargo build -p rust-relay-server 2>&1 | grep -c "warning\[" || echo 0`
  - 预期: 0 个 warning

---

### Task 4: RelayClient 模块（client feature）

**涉及文件:**
- 新建: `rust-relay-server/src/client/mod.rs`

**执行步骤:**
- [x] 创建 `rust-relay-server/src/client/mod.rs`：
  - `RelayClient` 结构体：持有 WS write half 的 Sender handle
  - `RelayEventRx` 类型别名：`mpsc::UnboundedReceiver<WebMessage>`
- [x] 实现 `RelayClient::connect(url, token, name) -> Result<(Self, RelayEventRx)>`：
  - 拼接 WS URL：`{url}/agent/ws?token={token}&name={name}`
  - 使用 `tokio-tungstenite::connect_async` 建立连接
  - 解析首条 WS 消息获取 `session_id`
  - 启动后台读循环 task：读 WS 消息 → 解析为 WebMessage → 发到 RelayEventRx channel
  - 返回 `(RelayClient, relay_event_rx)`
- [x] 实现 `RelayClient::send_agent_event(&self, event: &AgentEvent)`：
  - 序列化为 RelayMessage::AgentEvent JSON → 通过 WS write half 发送
- [x] 实现 `RelayClient::send_raw(&self, msg: &str)` 用于发送 pong 等控制消息
- [x] 实现心跳响应：后台读循环中收到 `ping` → 自动回复 `pong`
- [x] 实现断线重连逻辑：
  - 后台 task 检测到 WS 断开 → 指数退避重连（2s, 4s, 8s, ..., 最大 60s）
  - 重连成功后重新注册，获取新 session_id → 通过 channel 通知调用方

**检查步骤:**
- [x] client feature 单独编译通过
  - `cargo build -p rust-relay-server --no-default-features --features client`
  - 预期: 编译通过
- [x] 不引入 axum 依赖
  - `cargo tree -p rust-relay-server --no-default-features --features client 2>/dev/null | grep -c axum`
  - 预期: 输出 0
- [x] 全量编译通过
  - `cargo build`
  - 预期: 编译通过无错误

---

### Task 5: Web 前端（HTML + Vanilla JS）

**涉及文件:**
- 新建: `rust-relay-server/web/index.html`
- 新建: `rust-relay-server/web/app.js`
- 新建: `rust-relay-server/web/style.css`

**执行步骤:**
- [x] 创建 `index.html`：
  - 基础 HTML5 结构，引用 `app.js` 和 `style.css`
  - 布局：Tab 栏 + TODO 面板区域 + 消息区域 + 输入框
  - 从 URL query 参数读取 `token`
  - HITL 弹窗 DOM 结构（默认隐藏）
  - ask_user 弹窗 DOM 结构（默认隐藏）
  - 连接状态指示器
- [x] 创建 `style.css`：
  - Tab 栏样式：在线绿点/断线灰点/🔔角标
  - 消息区域：工具名颜色（cyan + bold）、参数文字暗灰色（#666）
  - TODO 面板：进行中黄色、完成暗灰、待办白色
  - HITL/ask_user 弹窗：模态遮罩 + 居中卡片
  - 输入框：底部固定，圆角
  - 响应式：移动端适配
- [x] 创建 `app.js` 核心逻辑：
  - **WS 连接管理**：
    - `connectManagement(token)` → 管理 WS（`/web/ws?token=...`），接收 agent_online/offline/agents_list 广播
    - `connectSession(token, sessionId)` → 专属 WS，接收该 Agent 的 agent_event，发送 user_input/hitl_decision 等
    - 自动重连（指数退避）
  - **Tab 管理**：
    - `agents` Map：`{sessionId → {name, status, messages[], todos[], ws, pendingHitl}}`
    - `activeSessionId`：当前活跃 Agent
    - `switchTab(sessionId)`：切换活跃 Agent，渲染对应消息和 TODO
    - 新 Agent 上线时动态添加 Tab DOM
    - Agent 断线时更新 Tab 状态为灰点
  - **消息渲染**：
    - `renderAgentEvent(event)`：根据事件类型追加 DOM
    - AssistantChunk：流式追加到最新消息 div
    - ToolCall：渲染工具块（工具名 cyan + bold，参数 DarkGray）
    - Done/Error：更新状态
  - **TODO 面板**：
    - `renderTodoPanel(todos)`：动态渲染/隐藏，颜色分类
  - **HITL 弹窗**：
    - `showHitlDialog(requests)`：显示弹窗，每个工具调用提供 Approve/Reject 按钮
    - 确认后发送 `hitl_decision` 消息
    - 未激活 Tab 收到时，Tab 角标显示 🔔
  - **ask_user 弹窗**：
    - `showAskUserDialog(questions)`：显示问题列表，单选/多选选项
    - 确认后发送 `ask_user_response` 消息
  - **输入框**：
    - Enter 发送 `user_input` 消息到活跃 Agent 的专属 WS
    - `/clear` → 发送 `clear_thread` 消息

**检查步骤:**
- [x] 静态文件存在且可被 rust-embed 嵌入
  - `ls -la rust-relay-server/web/ | wc -l`
  - 预期: 至少 3 个文件（index.html, app.js, style.css）
- [x] 编译时嵌入成功
  - `cargo build -p rust-relay-server`
  - 预期: 编译通过（rust-embed 嵌入 web/ 目录）
- [x] HTML 语法无误
  - `grep -c "</html>" rust-relay-server/web/index.html`
  - 预期: 输出 1

---

### Task 6: rust-agent-tui 集成 RelayClient

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 在 `rust-agent-tui/Cargo.toml` 添加依赖：
  - `rust-relay-server = { path = "../rust-relay-server", default-features = false, features = ["client"] }`
- [x] 扩展 settings 配置读取：
  - 在 `AppConfig` 的 `extra` 字段中读取 `relay_url`, `relay_token`, `relay_name`（利用 `#[serde(flatten)] extra: Map<String, Value>` 已有机制）
  - 或定义辅助函数 `get_relay_config(config) -> Option<(String, String, Option<String>)>` 从 extra map 提取
- [x] 在 `App` struct 中添加字段：
  - `relay_client: Option<RelayClient>`
  - `relay_event_rx: Option<RelayEventRx>`
- [x] 在 App 初始化流程中（`App::new` 或启动异步任务中）：
  - 读取 relay 配置
  - 若 `relay_url` 存在 → 调用 `RelayClient::connect(url, token, name)`
  - 连接成功 → 日志打印 session_id
  - 连接失败 → 仅日志 warn，不阻塞 TUI 启动
- [x] 在 Agent 事件处理路径中转发事件到 Relay：
  - `handle_agent_event()` 中每收到一个 AgentEvent → 若 relay_client 存在则 `relay_client.send_agent_event(&event)`
  - 将 TUI 层的 AgentEvent 映射为 rust-create-agent 层的 AgentEvent（或直接序列化 TUI AgentEvent）
- [x] 在主事件循环中接收 Relay 事件：
  - poll relay_event_rx → 收到 WebMessage::UserInput → 调用 `submit_message(text)`
  - 收到 WebMessage::HitlDecision → 调用 `hitl_confirm(decisions)`
  - 收到 WebMessage::AskUserResponse → 调用 `ask_user_confirm(answers)`
  - 收到 WebMessage::ClearThread → 调用 `new_thread()` 或 `clear()`
- [x] 确保无 relay_url 配置时的完全兼容：
  - `relay_client = None`, `relay_event_rx = None`
  - 所有转发路径有 `if let Some(client) = &self.relay_client` 守卫

**检查步骤:**
- [x] TUI 编译通过
  - `cargo build -p rust-agent-tui`
  - 预期: 编译通过无错误
- [x] 无 relay 配置时 TUI 正常启动（无回归）
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 所有测试通过
- [x] 全量编译通过
  - `cargo build`
  - 预期: 编译通过无错误

---

### Task 7: remote-control-access Acceptance

**Prerequisites:**
- 启动 Relay Server: `RELAY_TOKEN=test RELAY_PORT=8080 cargo run -p rust-relay-server`
- 配置 TUI Agent A: `settings.json` 中设置 `relay_url: "ws://localhost:8080"`, `relay_token: "test"`, `relay_name: "Agent-A"`
- 配置 TUI Agent B: 同上，`relay_name: "Agent-B"`（另一终端启动）

**End-to-end verification:**

1. Relay Server 健康检查
   - `curl -s http://localhost:8080/health`
   - Expected: 返回 200 OK
   - On failure: check Task 3 main.rs 路由配置

2. Agent A 连接后 /agents 返回正确列表
   - `curl -s "http://localhost:8080/agents?token=test" | grep -c "Agent-A"`
   - Expected: 输出 1
   - On failure: check Task 3 handle_agent_ws session 注册逻辑

3. 两个 Agent 同时连接，/agents 返回两个条目
   - `curl -s "http://localhost:8080/agents?token=test" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))"`
   - Expected: 输出 2
   - On failure: check Task 3 DashMap 并发注册

4. Web 前端页面加载
   - `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/web/?token=test"`
   - Expected: 返回 200
   - On failure: check Task 5 index.html 和 Task 3 static_files 路由

5. Web 前端 JS 文件可访问
   - `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/web/app.js"`
   - Expected: 返回 200，Content-Type 包含 javascript
   - On failure: check Task 3 static_files.rs rust-embed 配置

6. Token 验证拒绝非法请求
   - `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/agents?token=invalid"`
   - Expected: 返回 401 或 403
   - On failure: check Task 3 auth.rs token 验证逻辑

7. Agent WS 连接建立并收到 session_id
   - `echo '{}' | websocat -1 "ws://localhost:8080/agent/ws?token=test&name=TestAgent" 2>/dev/null | head -1 | grep -c session_id`
   - Expected: 输出 1
   - On failure: check Task 3 handle_agent_ws session_id 返回

8. client feature 不引入 server 依赖
   - `cargo tree -p rust-relay-server --no-default-features --features client 2>/dev/null | grep -c axum`
   - Expected: 输出 0
   - On failure: check Task 1 Cargo.toml feature flag 配置

9. TUI 无 relay 配置时行为无回归
   - `cargo test -p rust-agent-tui 2>&1 | tail -3`
   - Expected: 所有测试通过
   - On failure: check Task 6 relay_client 守卫逻辑

10. Agent 断线后 session 仍保留
    - `curl -s "http://localhost:8080/agents?token=test"` 在 Agent 断开后立即查询
    - Expected: 仍返回该 Agent 条目
    - On failure: check Task 3 session 清理延迟逻辑

11. 全量编译无 warning
    - `cargo build 2>&1 | grep -c "warning\[" || echo 0`
    - Expected: 0
    - On failure: 检查各 Task 新增代码

12. rust-create-agent 现有测试无回归（AgentEvent Serialize 改动）
    - `cargo test -p rust-create-agent 2>&1 | tail -3`
    - Expected: 所有测试通过
    - On failure: check Task 2 Serialize 派生兼容性
