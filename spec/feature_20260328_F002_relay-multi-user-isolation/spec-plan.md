# relay-multi-user-isolation 执行计划

**目标:** 为 Relay Server 引入 UserNamespace 分层，支持多用户完全隔离，匿名账号通过服务端生成 UUID 实现

**技术栈:** Rust / axum 0.8 / dashmap / tokio / reqwest（注册 HTTP 调用）/ Preact Signals（前端）

**设计文档:** ./spec-design.md

---

### Task 1: UserNamespace 核心分层重构

**涉及文件:**
- 修改: `rust-relay-server/src/relay.rs`

**执行步骤:**
- [x] 新增 `UserNamespace` 结构体，包含 `sessions: DashMap<String, Arc<SessionEntry>>` 和 `broadcast_txs: RwLock<Vec<mpsc::UnboundedSender<String>>>`
  - 所有字段均使用 `pub`，与 `SessionEntry` 保持一致风格
- [x] 删除 `RelayState.sessions` 和 `RelayState.broadcast_txs` 字段，替换为 `users: DashMap<String, Arc<UserNamespace>>`
  - 添加辅助方法 `get_or_create_namespace(user_id)` → `Arc<UserNamespace>`，使用 `entry().or_insert_with(|| Arc::new(UserNamespace::new()))` 懒创建
- [x] 更新 `RelayState::agents_list()` → 接收 `user_id: &str` 参数，仅返回该 namespace 下的 sessions
- [x] 更新 `RelayState::broadcast()` → 接收 `user_id: &str`，仅广播到该 namespace 的 `broadcast_txs`（包含清理失效 tx 逻辑）
- [x] 更新 `RelayState::forward_to_web()` → 接收 `user_id: &str` + `session_id: &str`，先查 `users[user_id]`，再查 `sessions[session_id]`；找不到 user namespace 时静默返回
- [x] 更新 `handle_agent_ws(ws, state, name, user_id)` 签名，连接时通过 `get_or_create_namespace(user_id)` 懒创建 namespace，session 注册到该 namespace
- [x] 更新 `handle_web_management_ws(ws, state, user_id)` 签名，broadcast_tx 注册到对应 namespace
- [x] 更新 `handle_web_session_ws(ws, state, session_id, user_id)` 签名，在 `users[user_id].sessions` 中查找 session；user_id 不匹配时返回 `session_not_found` 错误并关闭连接
- [x] 更新 `spawn_session_cleanup`：清理 session 后，额外检查 namespace 是否为空（`sessions.is_empty()`），若为空则从 `users` 中移除该 namespace

**检查步骤:**
- [x] 单元测试通过
  - `cargo test -p rust-relay-server --lib 2>&1 | tail -20`
  - 预期: 输出 `test result: ok.` 无 FAILED
- [x] 编译通过
  - `cargo build -p rust-relay-server 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出（无编译错误）

---

### Task 2: /register 端点 + user_id 参数校验

**涉及文件:**
- 修改: `rust-relay-server/src/main.rs`

**执行步骤:**
- [x] 新增 `register_handler`：`POST /register?token=` → 生成 UUID v4（`uuid::Uuid::new_v4().to_string()`）→ 返回 `axum::Json(serde_json::json!({"user_id": uuid}))`
  - 验证 token（与现有 handler 保持一致）；token 缺失/错误返回 401
- [x] 更新 `AgentWsQuery` 增加 `user_id: Option<String>` 字段
- [x] 更新 `WebWsQuery` 增加 `user_id: Option<String>` 字段
- [x] 更新 `TokenQuery` 增加 `user_id: Option<String>` 字段（`/agents` 端点用）
- [x] 在 `agent_ws_handler` 中：`user_id` 缺失时返回 `StatusCode::BAD_REQUEST`；存在时传递给 `relay::handle_agent_ws`
- [x] 在 `web_ws_handler` 中：`user_id` 缺失时返回 `StatusCode::BAD_REQUEST`；存在时传递给对应 handler
- [x] 在 `agents_handler` 中：`user_id` 缺失时返回 `StatusCode::BAD_REQUEST`；存在时传递给 `state.agents_list(user_id)`
- [x] 在 `Router` 中注册 `POST /register` 路由：`.route("/register", post(register_handler))`
  - `Cargo.toml` 中 `uuid` 依赖已存在于 `rust-create-agent`；在 `rust-relay-server/Cargo.toml` 中确认或添加 `uuid = { version = "1", features = ["v4"] }`

**检查步骤:**
- [x] /register 端点返回合法 UUID
  - `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server &; sleep 2; curl -s -X POST "http://localhost:8080/register?token=test-token" | jq .user_id`
  - 预期: 输出一个符合 UUID v4 格式的字符串，如 `"550e8400-e29b-41d4-..."`
- [x] 缺少 user_id 时返回 400
  - `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/web/ws?token=test-token"`（HTTP 层，不升级 WS）
  - 预期: `400`
- [x] token 错误时返回 401
  - `curl -s -o /dev/null -w "%{http_code}" -X POST "http://localhost:8080/register?token=wrong"`
  - 预期: `401`

---

### Task 3: RelayClient connect() 增加 user_id 参数

**涉及文件:**
- 修改: `rust-relay-server/src/client/mod.rs`

**执行步骤:**
- [x] `connect(url, token, name, user_id: &str)` 函数签名新增 `user_id` 参数
  - WS URL 拼接：`format!("{}/agent/ws?token={}&user_id={}", url, token, user_id)`
  - 若有 name，继续追加 `&name={}`
- [x] 检查所有调用 `RelayClient::connect` 的位置（仅 `rust-agent-tui/src/app/mod.rs`），更新调用签名

**检查步骤:**
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -5`
  - 预期: 无输出

---

### Task 4: config/types.rs + relay_panel.rs — 配置字段与面板状态

**涉及文件:**
- 修改: `rust-agent-tui/src/config/types.rs`
- 修改: `rust-agent-tui/src/app/relay_panel.rs`

**执行步骤:**
- [x] `RemoteControlConfig` 增加 `user_id: Option<String>` 字段，`#[serde(default, skip_serializing_if = "Option::is_none")]`
- [x] `relay_panel.rs` 的 `RelayPanel` 增加 `web_access_url: Option<String>` 字段（只读展示，不参与编辑）
  - `from_config` 时初始化为 `None`（连接成功后由 relay_ops 填充）
- [x] 新增 `RelayPanel::set_web_access_url(&mut self, url: Option<String>)` 方法

**检查步骤:**
- [x] RemoteControlConfig 序列化不含 user_id（当为 None）
  - `cargo test -p rust-agent-tui --lib test_remote_control_config_skip_when_none 2>&1 | tail -5`
  - 预期: `test result: ok.`
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -5`
  - 预期: 无输出

---

### Task 5: TUI 注册与连接流程

**涉及文件:**
- 修改: `rust-agent-tui/src/app/relay_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] `relay_ops.rs` 中新增异步函数 `get_or_register_user_id(base_url: &str, token: &str, existing: Option<&str>) -> anyhow::Result<String>`：
  - 若 `existing.is_some()` → 直接返回
  - 否则：`reqwest::Client::new().post(format!("{}/register?token={}", base_url, token)).send().await` → 解析 `{"user_id": "..."}` → 返回 user_id
  - base_url 从 ws URL 转换为 http URL（`ws://` → `http://`，`wss://` → `https://`，去掉末尾 `/`）
- [x] `app/mod.rs` 的 `relay_params` 类型从 `Option<(String, String, Option<String>)>` 改为 `Option<(String, String, Option<String>, String)>`（增加 user_id）
- [x] `init_relay_connection`（mod.rs 中约 line 300+ 的连接初始化逻辑）：
  - 调用 `get_or_register_user_id(base_url, token, existing_user_id)` 获取 user_id
  - 若注册成功且 config 中 user_id 为 None：更新 `zen_config.config.remote_control.user_id = Some(uid)` 并调用 `config_store.save()`
  - 将 user_id 加入 `relay_params` 和 `RelayClient::connect` 调用
- [x] `check_relay_reconnect`（relay_ops.rs）：从 `relay_params` 取出 user_id，传入 `RelayClient::connect`
- [x] 连接成功后：通过 `app.relay_panel.as_mut().map(|p| p.set_web_access_url(...))` 设置 Web 接入 URL
  - 格式：从 relay URL 构造 HTTP URL，加上 `#user_id={user_id}` 路径，例如 `http://localhost:8080/web/#user_id=xxx`

**检查步骤:**
- [x] 首次连接后 settings.json 中包含 user_id 字段
  - `cat ~/.zen-code/settings.json | jq '.config.remote_control.user_id'`
  - 预期: 输出一个 UUID 字符串（非 null）
- [x] 二次启动复用同一 user_id（不重新注册）
  - 记录第一次 user_id；重启 TUI 连接同一 relay；再次检查 settings.json
  - 预期: user_id 与第一次相同

---

### Task 6: 前端 connection.js — URL hash user_id 支持

**涉及文件:**
- 修改: `rust-relay-server/web/connection.js`
- 修改: `rust-relay-server/src/static_files.rs`（touch 触发重编译）

**执行步骤:**
- [x] `connection.js` 文件顶部添加 `getUserId()` 函数：解析 `window.location.hash`（去掉 `#`），使用 `URLSearchParams` 提取 `user_id`；若为空返回 `null`
  ```js
  function getUserId() {
    const hash = window.location.hash.slice(1)
    return new URLSearchParams(hash).get('user_id') || null
  }
  ```
- [x] `connectManagement()` 中：获取 `userId = getUserId()`；若为 null，在页面展示提示（通过 `connectionStatus.value = 'no_user_id'` 或类似 signal）；若有值，URL 拼接 `&user_id=${userId}`
- [x] `connectSession()` 中：同样获取 userId 并拼入 session WS URL
- [x] 在 `state.js` 中新增 `noUserIdSignal = signal(false)` 或复用 `connectionStatus` 信号，在 App.js / Pane.js 适当位置渲染提示："请从 TUI 复制完整的接入 URL（包含 #user_id=...）"
- [x] `touch rust-relay-server/src/static_files.rs` 以触发 `rust-embed` 重新内嵌前端文件
- [x] 重新编译 relay-server：`cargo build -p rust-relay-server --features server`

**检查步骤:**
- [x] 含 user_id hash 的 URL 可成功建立管理端 WS 连接
  - `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server &; sleep 2; node -e "const WebSocket = require('ws'); const ws = new WebSocket('ws://localhost:8080/web/ws?token=test-token&user_id=test-uuid'); ws.on('open', () => { console.log('connected'); process.exit(0); }); ws.on('error', e => { console.error(e.message); process.exit(1); });"`
  - 预期: 输出 `connected`
- [x] 缺少 user_id 时 WS 连接被拒绝（400）
  - `curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/web/ws?token=test-token`
  - 预期: `400`（或 WS 握手失败）

---

### Task 7: relay-multi-user-isolation Acceptance

**Prerequisites:**
- 启动命令: `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server`
- 等待服务启动: `sleep 2`
- 准备 WebSocket 客户端: `node -e "const WebSocket = require('ws'); ..."`（或使用 `wscat`）
- user_id_A 和 user_id_B 预先注册：
  ```bash
  UID_A=$(curl -s -X POST "http://localhost:8080/register?token=test-token" | jq -r .user_id)
  UID_B=$(curl -s -X POST "http://localhost:8080/register?token=test-token" | jq -r .user_id)
  ```

**端到端验证:**

1. [x] /register 返回合法 UUID v4
   - `curl -s -X POST "http://localhost:8080/register?token=test-token" | jq '.user_id | test("^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")'`
   - Expected: `true`
   - On failure: check Task 2 register_handler

2. [x] user_id=A 的 agent 连接成功，/agents?user_id=A 只返回该 agent
   - 先连接 agent：`wscat -c "ws://localhost:8080/agent/ws?token=test-token&user_id=$UID_A&name=agent-a"`（后台保持连接）
   - `curl -s "http://localhost:8080/agents?token=test-token&user_id=$UID_A" | jq 'length'`
   - Expected: `1`（仅 agent-a）
   - On failure: check Task 1 agents_list 隔离逻辑，Task 2 参数传递

3. [x] user_id=B 的管理端看不到 user_id=A 的 AgentOnline 广播
   - 连接 B 的管理端，监听消息；连接 A 的新 agent；B 端收不到 AgentOnline
   - `node test/isolation_broadcast_test.js` （脚本：ws B 管理端连接，记录 10 秒内消息；另一进程 ws A agent 连接；断言 B 端无 agent_online）
   - Expected: B 端收到消息数为 0 或不含 `agent_online`
   - On failure: check Task 1 broadcast 隔离逻辑

4. [x] user_id=B 尝试连接 user_id=A 的 session 时返回 session_not_found
   - 获取 A 的 session_id：通过连接 A 管理端或解析 A agent 连接时的 session_id
   - 用 B 的 user_id 连接 A 的 session：`wscat -c "ws://localhost:8080/web/ws?token=test-token&user_id=$UID_B&session=$SESSION_A"`
   - Expected: 收到 `{"type":"error","code":"session_not_found","message":"..."}` 后断开
   - On failure: check Task 1 handle_web_session_ws 双重匹配逻辑

5. [x] 缺少 user_id 参数时返回 400
   - `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/agents?token=test-token"`
   - Expected: `400`
   - On failure: check Task 2 user_id 缺失校验

6. [ ] TUI 首次连接自动注册并持久化 user_id
   - 删除 settings 中的 user_id（或使用新配置）；启动 TUI 并连接 relay
   - `cat ~/.zen-code/settings.json | jq '.config.remote_control.user_id // empty'`
   - Expected: 输出合法 UUID（非空）
   - On failure: check Task 5 get_or_register_user_id 与保存逻辑
