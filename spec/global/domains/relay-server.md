# Relay Server 领域

## 领域综述

Relay Server 领域负责中心化 WebSocket 中继服务，使远程客户端（浏览器）能够访问和控制本地运行的 Agent 实例，支持多 Agent 会话管理和实时事件同步。

核心职责：
- Agent 注册：WebSocket 连接认证（Token），session 管理（DashMap）
- 消息路由：Agent ↔ Web 双向转发
- 会话同步：seq 序列号、history 缓存、`sync_request` 增量拉取
- Web 前端：Claude 风格深色主题，1/2/3 分屏，ES Modules 模块化，Markdown 渲染+代码高亮
- 协议规范：扁平化 JSON 帧，seq 序列号，message_id 字段支持 update-in-place
- 可观测性：Web 连接/断开 info 日志，认证失败 warn 日志，消息转发 trace 日志
- 执行状态同步：agent_running/agent_done 事件驱动 Web 「正在思考…」状态

## 核心流程

### Agent 连接建立

```
Agent TUI 启动（配置 relay_url/relay_token/relay_name）
  → RelayClient::connect(ws://host/agent/ws?token=&name=)
  → Relay 验证 token → 生成 UUID session_id
  → 返回 { type: "session_id", session_id: "..." }
  → 广播 agent_online 给所有 Web 客户端
```

### Web 同步流程

```
Web 连接 session WS
  → onopen: send { type: "sync_request", since_seq: 0/maxSeq }
  → TUI poll_relay 收到 → get_history_since(since_seq)
  → sync_response { events: [...] } → 批量回放

实时事件: send_with_seq(event) → seq 递增 → 发往所有 session 订阅者
```

### 消息格式规范化

```
旧格式（废弃）: { "type": "agent_event", "event": { "type": "text_chunk", ... } }
新格式（扁平）: { "type": "text_chunk", "seq": 42, "0": "hello" }
BaseMessage 格式: { "role": "user"|"assistant"|"tool"|"system", "content": "...", "seq": N }
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Web 框架 | axum 0.8（WS feature），与 tokio 生态一致 |
| WS 库 | tokio-tungstenite，与 tokio runtime 集成 |
| 会话管理 | `DashMap<SessionId, SessionEntry>`，无锁并发 |
| 广播 | `RwLock<Vec<UnboundedSender>>`，向所有 Web 写 |
| Feature flag | `server`（默认，含 axum）+ `client`（仅 tungstenite），避免 TUI 引入服务端依赖 |
| 静态文件 | `rust-embed`，编译时嵌入 web/ 目录 |
| 序列号 | `AtomicU64`，`fetch_add(1, Relaxed)`，历史缓存 VecDeque 上限 1000 条 |
| 消息序列化 | serde internally-tagged enum，`#[serde(tag = "type")]` |
| Web 前端 | Claude 风格深色主题；ES Modules 无构建工具；Tailwind CSS CDN；marked.js + highlight.js（GitHub Dark）+ DOMPurify |
| 分屏布局 | 1/2/3 分屏，state.layout.cols + panes 数组，各面板独立绑定 session |
| 日志规范 | 认证失败 warn；连接/断开 info；消息转发 trace（不记录内容，只记字节数）|
| 执行状态 | agent_running/agent_done JSON 事件，send_value 路径，纳入 history 缓存，可被 sync_response 重放 |

## Feature 附录

### 20260323_F004_remote-control-access
**摘要:** Relay Server + Web 前端实现远程访问控制本地 Agent
**关键决策:**
- 架构: 新 crate `rust-relay-server`，server + client 双 feature
- Feature 隔离: `features = ["server"]`（默认）含 axum；`features = ["client"]` 仅含 tungstenite
- Web 前端: 纯 HTML + Vanilla JS，内嵌在 rust-embed，无前端框架
- Tab 管理: 动态增删，绿点（在线）/灰点（断线）/🔔（待审批）
- HITL 同步: Web 和 TUI 同时弹出，任意一端确认即生效
- 重连: 指数退避（2s-60s），Session 保留 30 分钟
**归档:** [链接](../../archive/feature_20260323_F004_remote-control-access/)
**归档日期:** 2026-03-24

### 20260323_F006_ws-event-sync
**摘要:** WebSocket 事件扁平化+seq序列号+会话 Sync 同步
**关键决策:**
- 扁平化: RelayClient::send_with_seq 直接发送事件 JSON，不包裹 RelayMessage
- seq 注入: `fetch_add(1) → val["seq"] = seq → 缓存 + 发送
- history 缓存: VecDeque 上限 1000 条，超时 pop_front
- get_history_since: 过滤 `seq > since_seq`，支持增量 sync
- Phase 2 BaseMessage: 新增 MessageAdded(BaseMessage) 事件，前端双格式兼容
- 前端双格式: `handleBaseMessage`（role 字段）+ `handleLegacyEvent`（type 字段）
**归档:** [链接](../../archive/feature_20260323_F006_ws-event-sync/)
**归档日期:** 2026-03-24

### feature_20260324_F002_relay-server-ui-redesign
**摘要:** Relay Web 前端重设计为 Claude 风格多分屏界面
**关键决策:**
- Tailwind CSS CDN + 自定义 CSS 变量（--bg-base/#0d0d0d 暖橙强调色 --accent/#e8975e）
- 7 个 ES Module：main/state/connection/events/render/layout/dialog
- marked.js + highlight.js（GitHub Dark）+ DOMPurify（XSS 防护）
- 分屏模式：state.layout.cols(1/2/3) + panes 数组
- 消息渲染：工具调用卡片可折叠；代码块复制按钮；streaming 闪烁光标
**归档:** [链接](../../archive/feature_20260324_F002_relay-server-ui-redesign/)
**归档日期:** 2026-03-27

### feature_20260326_F001_relay-frontend-mobile-redesign
**摘要:** Relay 前端移动端重设计（无设计文档）
**关键决策:** — （无设计文档）
**归档:** [链接](../../archive/feature_20260326_F001_relay-frontend-mobile-redesign/)
**归档日期:** 2026-03-27

### feature_20260326_F007_relay-server-logging
**摘要:** 补充 Relay Server Web 连接、认证失败、消息转发日志
**关键决策:**
- 认证失败：tracing::warn!(endpoint=..., "认证失败，返回 {code}")
- Web 管理端/会话端连接/断开：tracing::info!(active_web=..., session=...)
- 消息转发：tracing::trace!(bytes=text.len())，不记录内容（避免泄漏）
- 全部使用 tracing 宏，不使用 println!
**归档:** [链接](../../archive/feature_20260326_F007_relay-server-logging/)
**归档日期:** 2026-03-27

### feature_20260326_F010_relay-loading-state-sync
**摘要:** Agent 执行状态同步到 Web 前端显示「正在思考…」
**关键决策:**
- 不修改 AgentEvent/RelayMessage 枚举，用 send_value(json!({type: "agent_running"})) 发送
- agent_running/agent_done 纳入 history 缓存（含 seq），可被 sync_response 重放还原状态
- 前端 isRunning 状态从事件流派生；输入不禁用，仅显示状态文字
**归档:** [链接](../../archive/feature_20260326_F010_relay-loading-state-sync/)
**归档日期:** 2026-03-27

---

## 相关 Feature
