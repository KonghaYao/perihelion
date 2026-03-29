# Feature: 20260328_F002 - relay-multi-user-isolation

## 需求背景

当前 Relay Server 的 `RelayState` 是全局扁平结构——所有用户的 agent session 共享同一个 `DashMap`，broadcast 广播给所有管理端连接，`/agents` 端点返回全部 agent。这意味着：

- 多人共用一个 Relay 实例时，任何人都能看到其他人的 agent
- 一个 Web 客户端只要知道 session_id 就能连接任何人的 agent
- 没有用户隔离，无法支持多租户场景

需要在保持当前架构（WebSocket 中继、无构建工具前端、异步 Rust）的前提下，支持多人多 agent 连接，且人与人之间完全隔离。

## 目标

- 每个用户拥有独立的命名空间，只能看到和操作自己的 agent
- 用户通过匿名账号体系接入（服务端生成 UUID，客户端保存并复用），无需注册
- 保持向后兼容路径设计（仅新增参数，不破坏协议格式）
- 实现纯内存存储，无需持久化用户数据

## 方案设计

### 核心架构：UserNamespace 分层

在 `RelayState` 中引入 `UserNamespace` 层，将现有的 flat session map 改为按 `user_id` 分组的两级结构：

```
RelayState
  ├── token: String                                // 服务器门禁 Token（不变）
  ├── users: DashMap<user_id, Arc<UserNamespace>>  // ★ 新增
  ├── active_agent_conns: AtomicUsize              // 全局计数（不变）
  ├── active_web_conns: AtomicUsize                // 全局计数（不变）
  ├── max_agent_conns: usize
  └── max_web_conns: usize

UserNamespace（每个用户一份，懒创建）
  ├── sessions: DashMap<session_id, Arc<SessionEntry>>  // 该用户的 agent sessions
  └── broadcast_txs: RwLock<Vec<UnboundedSender<String>>>  // 该用户的管理端 WS
```

原有 `SessionEntry` 结构保持不变。

![系统架构与隔离边界](./images/01-architecture.png)

### 匿名账号：/register 端点

新增无状态注册端点：

```
POST /register?token=RELAY_TOKEN
→ 200 OK  {"user_id": "550e8400-e29b-41d4-a716-446655440000"}
```

服务器生成 UUID v4 并直接返回，**不存储**。服务端重启不影响已有 user_id（因为不做验证，user_id 仅用作命名空间 key，任何合法 UUID 格式均可路由）。

首次使用流程：
1. TUI 检查 `~/.zen-code/settings.json` 中 `relay.user_id` 是否存在
2. 不存在 → 调用 `POST /register?token=RELAY_TOKEN`，拿到 UUID
3. 写入 settings 文件，后续复用

### 连接参数变化

| 端点 | 现有参数 | 新增参数 |
|------|----------|----------|
| `POST /register` | `token` | —（新增端点）|
| `/agent/ws` | `token`, `name` | `user_id`（必填）|
| `/web/ws` | `token`, `session` | `user_id`（必填）|
| `/agents` | `token` | `user_id`（必填）|

`user_id` 缺失时返回 `400 Bad Request`。

### 隔离边界

| 操作 | 改前行为 | 改后行为 |
|------|----------|----------|
| `BroadcastMessage::AgentOnline/Offline` | 广播全体管理端 WS | 仅广播同 `user_id` namespace 内的管理端 WS |
| `AgentsList` | 返回所有 sessions | 仅返回该 `user_id` 的 sessions |
| `forward_to_web` | 按 session_id 查全局 DashMap | 先查 `users[user_id]`，再查 `sessions[session_id]` |
| `handle_web_session_ws` | 任何人知道 session_id 可连 | 必须 `user_id` + `session_id` 双重匹配 |

### 数据流：连接与消息路由

![多用户连接与消息路由流程](./images/02-flow.png)

完整数据流：

```
Web Browser (user_id=A)          Relay Server               TUI Agent (user_id=A)
       │                              │                              │
       │ GET /web/ws?token=X          │                              │
       │    &user_id=A ──────────────→│                              │
       │                              │← user A namespace (lazy创建)  │
       │                              │                              │
       │                              │←── /agent/ws?token=X ────────│
       │                              │         &user_id=A            │
       │                              │         &name=laptop ─────────│
       │                              │                              │
       │←── BroadcastMessage(AgentOnline) ←── 仅推送给 user A 管理端  │
       │                              │                              │
       │ GET /web/ws?token=X          │                              │
       │    &user_id=A                │                              │
       │    &session=S1 ─────────────→│                              │
       │                              │── forward_to_web(user=A, s=S1)│
       │←── MessageBatch ────────────←│←── MessageBatch ─────────────│
```

Web Browser (user_id=B) 完全看不到 user_id=A 的任何消息。

### Web 前端：user_id 传递方式

前端通过 **URL hash** 获取 user_id：

```
http://relay-server/web/#user_id=550e8400-xxxx
```

TUI 的 `/relay` 面板在成功连接后，显示包含当前 `user_id` 的完整 Web 接入 URL，用户复制后在浏览器打开即可看到自己的 agents。

前端实现：
1. `connection.js` 启动时解析 `window.location.hash` 提取 `user_id`
2. 所有 WS 连接 URL 拼入 `&user_id=USER_ID`
3. 如果 hash 中没有 `user_id`，显示提示页面（"请从 TUI 复制完整的接入 URL"）

### UserNamespace 生命周期

- **懒创建**：首个 agent 连接时 `users.entry(user_id).or_insert_with(|| Arc::new(UserNamespace::new()))` 自动创建
- **清理时机**：`spawn_session_cleanup` 额外检查：namespace 下所有 session 均过期后，删除整个 namespace
- **无持久化**：server 重启后 users map 清空，TUI 客户端重连时重建 namespace（user_id 保存在客户端，不受影响）

## 实现要点

1. **relay.rs 重构**：`RelayState.sessions` → `RelayState.users: DashMap<String, Arc<UserNamespace>>`；所有 handler 签名增加 `user_id: String` 参数
2. **main.rs 路由**：新增 `POST /register` 路由；现有 ws handler 增加 `user_id` query 参数解析
3. **auth.rs 不变**：token 验证逻辑不变，user_id 不需要验证
4. **TUI relay_ops.rs**：`get_or_register_user_id()` 函数；连接 URL 拼接 user_id
5. **TUI relay_panel.rs**：显示 Web 接入 URL（含 user_id hash）
6. **前端 connection.js**：从 `window.location.hash` 解析 user_id；连接参数增加 user_id
7. **relay-server 重编译触发**：`touch rust-relay-server/src/static_files.rs` 使前端修改生效

## 约束一致性

- **Workspace 分层约束**：所有改动限于 `rust-relay-server`（server 端）和 `rust-agent-tui`（client 端），不触及 `rust-create-agent` 和 `rust-agent-middlewares`
- **axum 0.8 + dashmap**：沿用现有 web 框架和并发数据结构，新增 `DashMap<String, Arc<UserNamespace>>` 嵌套符合现有模式
- **无构建工具前端**：仅修改 connection.js 和 state.js，不引入新依赖
- **Signal 订阅规则**：新增的 `userIdSignal` 使用 `useSignalValue` 订阅，不直接读 `.value`
- **异步优先**：新 handler 签名保持 async，UserNamespace 内部 RwLock 用 tokio 版本

## 验收标准

- [ ] `POST /register?token=RELAY_TOKEN` 返回合法 UUID v4
- [ ] Agent A 连接 user_id=X，Web 客户端连接 user_id=Y，Y 看不到 X 的任何 AgentOnline/消息
- [ ] Agent A 和 Agent B 同属 user_id=X，Web 客户端连接 user_id=X 可看到两个 agent
- [ ] Web 会话端连接时 user_id 与 session 的所有者不匹配，返回 session_not_found
- [ ] TUI 首次连接自动注册并保存 user_id 到 settings.json，后续重启复用同一 user_id
- [ ] `/relay` 面板显示含 user_id hash 的完整 Web 接入 URL
- [ ] 缺少 user_id 参数时服务器返回 400
- [ ] namespace 下所有 session 过期后，cleanup 任务自动删除空 namespace
