# Relay Server 领域

## 领域综述

Relay Server 领域负责中心化 WebSocket 中继服务，使远程客户端（浏览器）能够访问和控制本地运行的 Agent 实例，支持多 Agent 会话管理和实时事件同步。

核心职责：
- Agent 注册：WebSocket 连接认证（Token），session 管理（DashMap）
- 消息路由：Agent ↔ Web 双向转发
- 会话同步：seq 序列号、history 缓存、`sync_request` 增量拉取
- Web 前端：Tab 多 Agent 切换，HITL/ask_user 弹窗，消息渲染
- 协议规范：扁平化 JSON 帧，seq 序列号

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

---

## 相关 Feature
