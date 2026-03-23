# Feature: 20260323_F004 - remote-control-access

## 需求背景

当前 `rust-agent-tui` 是一个本地 TUI 应用，只能在运行它的终端上操作。用户需要从另一台机器（手机、平板、异地电脑）远程查看和操控本地 Agent 的执行过程，包括：发送消息、响应 HITL 审批、回答 ask_user 问题、查看 TODO 面板等。

现有架构缺乏网络暴露层，且直接暴露本地端口存在防火墙和安全隐患。为此需要一套基于中心化 Relay Server 的远程控制方案，支持多台机器上的多个 Agent 同时接入，通过 Web 前端 Tab 切换协同查看与操控。

## 目标

- 本地 Agent 启动后自动连接公网 Relay Server，无需开放本地端口
- 支持多个 Agent 同时连接同一 Relay，每个 Agent 有全局唯一 session_id 和可选名称
- Web 浏览器前端通过 Tab 切换访问不同 Agent，全功能交互（等同于 TUI）
- 全局单 Token 认证，持有即可访问 Relay 上全部已连接 Agent
- RelayClient 库代码内置于 `rust-relay-server` crate，供 `rust-agent-tui` 等应用引用
- 支持断线自动重连，连接稳定可靠

## 方案设计

### 总体架构

系统由三个部分组成：

1. **Relay Server + Client 库**（新 Crate `rust-relay-server`）：运行在公网服务器，负责多 Agent session 注册与管理、WebSocket 双向路由转发、Web 前端静态文件服务；内置 `client` 模块供本地 Agent 引用
2. **本地 Agent 客户端**（`rust-agent-tui` 引用 `rust-relay-server::client`）：主动外连 Relay Server，将 AgentEvent 实时转发
3. **Web 前端**（纯 HTML + Vanilla JS，内嵌在 Relay Server 中）：浏览器访问，Tab 切换多 Agent，全功能操控

![总体架构图](./images/01-architecture.png)

```
┌───────────────────────────────────────────────────────────┐
│                    Relay Server (公网)                     │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  session map: { session_id → SessionEntry(name) }   │  │
│  └──────────────────────────────────────────────────────┘  │
│  ┌─────────────────────────┐   ┌──────────────────────────┐ │
│  │  Agent WS /agent/ws     │◄─►│  Web WS /web/ws          │ │
│  │  (N 个 Agent 并发连接)  │   │  (M 个 Web 客户端连接)   │ │
│  └─────────────────────────┘   └──────────────────────────┘ │
│  HTTP /agents → 在线 Agent 列表                              │
│  HTTP /web/*  → 内嵌 SPA 静态文件                           │
│  HTTP /health → 健康检查                                    │
└───────────────────────────────────────────────────────────┘
     ▲▲▲ (N 个连接)                   ▲▲ (M 个连接)
     │                                 │
┌──────────┐ ┌──────────┐ ┌──────┐   ┌──────────────────────┐
│ Agent A  │ │ Agent B  │ │ ...  │   │  浏览器 Web 前端      │
│ (机器1)  │ │ (机器2)  │ │      │   │  Tab 切换查看多 Agent │
└──────────┘ └──────────┘ └──────┘   └──────────────────────┘
```

### Session 管理与通信流程

#### 多 Agent 连接建立流程

1. 本地 Agent 启动，读取配置 `relay_url` + `relay_token` + 可选 `relay_name`
2. Agent WebSocket 客户端连接 `wss://<relay>/agent/ws?token=<token>&name=<可选名称>`
3. Relay 验证 token → 生成全局唯一 `session_id`（UUID v4）→ 通过 WS 消息返回给 Agent
4. Agent 在终端/日志中打印：`🔗 Relay 已连接，session: <session_id>`
5. Relay 向**所有已连接的 Web 客户端**广播 `agent_online` 事件
6. 用户在浏览器访问 `https://<relay>/web/?token=<token>`（无需指定 session）
7. Web 前端建立管理 WS：`wss://<relay>/web/ws?token=<token>`，接收全量事件
8. Web 前端拉取 `GET /agents` 获取当前在线 Agent 列表，渲染 Tab 栏
9. 用户点击某个 Tab → Web 前端建立该 Agent 的专属通信 WS：`wss://<relay>/web/ws?token=<token>&session=<session_id>`

![Session 建立与多 Agent 连接流程](./images/02-flow.png)

#### 消息协议

所有消息均为 JSON，外层包装类型标签：

**Agent → Relay → Web（事件推送）**

```json
{ "type": "agent_event",  "event": { /* AgentEvent 序列化 */ } }
{ "type": "session_id",   "session_id": "550e8400-..." }
{ "type": "ping" }
```

**Web → Relay → Agent（用户操作，通过 session 专属 WS 发送）**

```json
{ "type": "user_input",       "text": "请读取 Cargo.toml" }
{ "type": "hitl_decision",    "decisions": [{ "tool_call_id": "...", "decision": "Approve" }] }
{ "type": "ask_user_response","answers": { "问题1": "回答1" } }
{ "type": "clear_thread" }
{ "type": "pong" }
```

**Relay → 所有 Web 广播（管理 WS 上接收）**

```json
{ "type": "agent_online",  "session_id": "...", "name": "机器A", "connected_at": "..." }
{ "type": "agent_offline", "session_id": "..." }
{ "type": "agents_list",   "agents": [{ "session_id": "...", "name": "...", "connected_at": "..." }] }
```

**Relay 错误消息**

```json
{ "type": "error", "code": "session_not_found", "message": "..." }
{ "type": "error", "code": "auth_failed",       "message": "..." }
```

#### 心跳与重连

- Relay Server 每 30s 向 Agent 和 Web 分别发送 `ping`
- 客户端回复 `pong`，超时 60s 未回复则断开连接
- Agent 客户端采用指数退避自动重连（初始 2s，最大 60s）
- Web 前端自动重连，重连后拉取最新 Agent 列表恢复 Tab 状态
- Session 在 Agent 断开后保留 30 分钟，便于 Agent 重连后 Web 端无感恢复

### Relay Server 设计

**技术栈：** Axum + tokio-tungstenite（与项目现有 tokio 生态一致）

**核心数据结构：**

```rust
struct SessionEntry {
    agent_tx: mpsc::Sender<Message>,         // 向 Agent 发消息
    web_txs: Vec<mpsc::Sender<Message>>,     // 向多个 Web 连接发消息
    name: Option<String>,                    // Agent 自报名称（用于 Tab 显示）
    created_at: Instant,
    last_active: Instant,                    // 用于空闲超时清理
}

struct RelayState {
    sessions: DashMap<SessionId, SessionEntry>,
    web_broadcast_txs: Vec<mpsc::Sender<Message>>, // 管理 WS 广播通道
    token: String,                           // 全局 token
}
```

**HTTP 路由：**

| 路径 | 方法 | 说明 |
|------|------|------|
| `GET /agent/ws` | WS | Agent 连接入口（`?token` + `?name` 可选） |
| `GET /web/ws` | WS | Web 连接入口（`?token`，管理 WS；`?token&session` 操作指定 Agent） |
| `GET /agents` | HTTP | 返回在线 Agent 列表 JSON |
| `GET /web/` | HTTP | 返回 index.html |
| `GET /web/app.js` | HTTP | 前端 JS |
| `GET /web/style.css` | HTTP | 前端 CSS |
| `GET /health` | HTTP | 健康检查 |

**Session 生命周期：** Agent 断开后保留 30 分钟，超时清理；空闲超时独立计时。

### Web 前端设计

**技术栈：** 纯 HTML + Vanilla JS（无框架），内嵌在 `rust-relay-server/web/` 目录，编译时通过 `rust-embed` 打包进二进制。

**页面布局（含 Tab 栏）：**

```
┌────────────────────────────────────────────────────────────┐
│  [机器A ●] [机器B ●] [机器C ○]                  [断线 ⚠️]  │  ← Tab 栏
├────────────────────────────────────────────────────────────┤
│  📋 TODO 面板（若有任务则显示）                              │
├────────────────────────────────────────────────────────────┤
│                                                            │
│              当前活跃 Agent 消息区                          │
│         （流式输出、工具调用块、HITL/ask_user 弹窗）         │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  > 输入框（发送到当前活跃 Agent）               [发送]      │
└────────────────────────────────────────────────────────────┘
```

![Web 前端页面布局](./images/03-wireframe.png)

**Tab 状态规范：**
- `●` 绿点：Agent 在线
- `○` 灰点：Agent 断线（消息历史本地缓存保留）
- `🔔` 角标：该 Agent 有待处理的 HITL 审批或 ask_user 请求（切换后显示弹窗）

**页面功能（等同 TUI，按活跃 Agent 隔离）：**

- **消息区域**：流式显示 AssistantChunk，工具调用块（含工具名颜色 + 参数暗灰色，对应 F003 规范）
- **TODO 面板**：动态显示/隐藏，颜色分类（进行中黄色、完成暗灰、待办白色）
- **输入区域**：支持 Enter 发送、`/clear`、`/help` 命令
- **HITL 弹窗**：批量审批操作（Approve/Reject/Edit/Respond）
- **ask_user 弹窗**：多选/单选问题回答
- **连接状态栏**：显示当前活跃 Agent 的连接状态

### RelayClient 模块（`rust-relay-server::client`）

`rust-relay-server` crate 通过 feature flag `client` 暴露 RelayClient 库，供 `rust-agent-tui` 等引用：

```rust
// rust-relay-server/src/client/mod.rs
pub struct RelayClient { ... }

impl RelayClient {
    pub async fn connect(url: &str, token: &str, name: Option<&str>)
        -> Result<(Self, RelayEventRx)>;
    pub fn send_agent_event(&self, event: &AgentEvent);
    // RelayEventRx 接收来自 Web 的操作（UserInput/HitlDecision/AskUserResponse）
}
```

`rust-agent-tui/Cargo.toml` 引用方式：

```toml
[dependencies]
rust-relay-server = { path = "../rust-relay-server", default-features = false, features = ["client"] }
```

### 项目结构变更

```
perihelion/
├── rust-create-agent/          # 不变
├── rust-agent-middlewares/     # 不变
├── rust-agent-tui/
│   ├── Cargo.toml              # 新增依赖 rust-relay-server（features=["client"]）
│   └── src/
│       └── app/mod.rs          # 修改：集成 RelayClient，处理 relay 事件
└── rust-relay-server/          # 新增 Crate（server + client 双用途）
    ├── Cargo.toml              # features: ["server"(default), "client"]
    ├── src/
    │   ├── main.rs             # Server 启动入口（需 feature = "server"）
    │   ├── relay.rs            # Session 管理，WebSocket 路由
    │   ├── auth.rs             # Token 验证
    │   ├── static_files.rs     # 内嵌 Web 静态文件（rust-embed）
    │   └── client/
    │       └── mod.rs          # RelayClient（pub，feature = "client"）
    └── web/
        ├── index.html
        ├── app.js              # Tab 切换、多 WS 连接管理、HITL 角标
        └── style.css
```

### rust-agent-tui 改动

**`app/mod.rs` 改动：**
- `App` 增加 `relay_client: Option<RelayClient>` 字段
- 启动时若配置了 `relay_url`，异步初始化 RelayClient（传入可选 `relay_name`）
- `submit_message`/`hitl_confirm`/`ask_user_confirm` 路径同时触发 relay 事件转发
- 新增 relay 事件接收循环（接收 Web 端输入，注入 App 消息队列）

**配置扩展（`~/.zen-code/settings.json`）：**

```json
{
  "relay_url":   "wss://your-relay.example.com",
  "relay_token": "your-secret-token",
  "relay_name":  "机器A"
}
```

## 实现要点

- **并发安全**：`DashMap` 管理 session 映射，多连接并发无锁竞争；`web_broadcast_txs` 用 `RwLock<Vec<...>>` 管理
- **消息队列**：Agent WS 和 Web WS 各自有独立 mpsc channel，不相互阻塞；`web_txs` 为 Vec，广播时串行发送
- **AgentEvent 序列化**：需为 `AgentEvent` 及相关类型派生 `serde::Serialize`；部分事件（如 `ApprovalNeeded`）需补充 Web 友好格式
- **rust-embed**：Web 前端静态文件在编译时嵌入二进制，`release` build 内嵌，`dev` build 读取本地文件（便于调试）
- **Feature flag 隔离**：`features = ["server"]`（默认，含 Axum 等服务端依赖）和 `features = ["client"]`（仅含 tokio-tungstenite 客户端依赖），避免 tui 引入不必要的服务端依赖
- **多 Web 客户端写权限**：初期全共享写入权限（任意 Web 客户端均可发送操作），后续可按需增加权限控制
- **HITL 双端同步**：HITL 审批弹窗在 Web 和 TUI 同时弹出，任意一端确认即生效；Relay 广播关闭事件后，其余端自动关闭弹窗
- **Web 多 WS 连接**：Web 前端同时维护 1 条管理 WS（接收广播）+ N 条 session 专属 WS（与具体 Agent 通信），Tab 切换时懒加载建立连接

## 约束一致性

- **Tokio 异步运行时**：Relay Server 完全基于 tokio，与现有所有 Crate 一致
- **Workspace Crate**：新增 `rust-relay-server` 作为独立 Crate 加入 Workspace，不修改现有 Crate 的公共 API
- **消息类型复用**：`AgentEvent` 扩展 `serde::Serialize`，不改变内部结构；新增 `RelayMessage` 枚举描述协议消息
- **无破坏性变更**：`rust-agent-tui` 的 relay 功能为可选（未配置 relay_url 时行为与现在完全一致）

## 验收标准

- [ ] 两个本地 `rust-agent-tui` 实例同时启动，均自动连接 Relay 并分别打印不同的 session_id
- [ ] Relay Server 独立启动，`/health` 返回 200，`/agents` 返回在线 Agent 列表 JSON
- [ ] 浏览器访问 `https://<relay>/web/?token=<token>`，页面正常加载，顶部显示两个 Agent Tab
- [ ] 切换 Tab 后，消息区域切换到对应 Agent 的消息历史
- [ ] 在 Web 前端对 Agent A 发送消息，仅 Agent A 收到并执行，响应实时流式出现在对应 Tab
- [ ] Agent A 触发 HITL 审批时，Tab A 显示 `🔔` 角标，切换到 Tab A 后弹窗正确显示
- [ ] ask_user 弹窗在对应 Tab 内正确显示并能回答
- [ ] TODO 面板内容按 Agent 隔离，切换 Tab 后展示对应 Agent 的 TODO
- [ ] 某个 Agent 断线后，对应 Tab 显示灰点 `○`，消息历史保留；重连后自动恢复绿点
- [ ] 新 Agent 上线时，所有已打开的 Web 页面动态增加新 Tab（无需刷新）
- [ ] `rust-agent-tui` 在未配置 relay_url 时，行为与现有版本完全一致（无回归）
- [ ] feature flag 隔离验证：仅启用 `features=["client"]` 时，编译不引入 Axum 等服务端依赖
