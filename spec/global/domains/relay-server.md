# Relay Server 领域

## 领域综述

Relay Server 领域负责中心化 WebSocket 中继服务，使远程客户端（浏览器）能够访问和控制本地运行的 Agent 实例，支持多 Agent 会话管理和实时事件同步。

核心职责：
- Agent 注册：WebSocket 连接认证（Token），session 管理（DashMap）
- 消息路由：Agent ↔ Web 双向转发
- 会话同步：seq 序列号、history 缓存、`sync_request` 增量拉取
- Web 前端：Preact + @preact/signals + htm（esm.sh CDN，无打包工具），1/2/3 分屏，Markdown 渲染+代码高亮，移动端响应式布局（抽屉侧边栏 + 面板 Tab）
- 协议规范：扁平化 JSON 帧，seq 序列号，message_id 字段支持 update-in-place；消息基于 UUIDv7 ID upsert 去重
- 可观测性：Web 连接/断开 info 日志，认证失败 warn 日志，消息转发 trace 日志
- 执行状态同步：agent_running/agent_done 事件驱动 Web 「正在思考…」状态
- Agent 中断：WebMessage::CancelAgent 触发 App::interrupt()，同步关闭弹窗
- Thread 双向同步：ThreadReset 消息同步 clear/history/compact 后的状态；不进历史缓存（send_raw）
- AskUser 完整协议：AskUserQuestion 含 tool_call_id/multi_select/options(含 description)/allow_custom_input/placeholder

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
| Web 前端 | Preact + @preact/signals + htm（esm.sh CDN，无构建工具）；marked.js + highlight.js（GitHub Dark）+ DOMPurify（动态 UMD 注入）|
| Signal 订阅 | 组件通过 `useSignalValue(signal)` 显式订阅；直接读 signal.value 仅用于写，不用于响应式依赖 |
| 分屏布局 | 1/2/3 分屏，state.layout.cols + panes 数组，各面板独立绑定 session |
| 移动端适配 | 抽屉侧边栏（transform+fixed，不占文档流）；多面板 Tab 栏（has-tabs class）；100dvh 虚拟键盘适配 |
| 消息去重 | upsertMessage(agent, msg)：基于 UUIDv7 id 的 merge 语义；tool slot 消息维持 tool_call_id 逻辑 |
| 双向同步 | ThreadReset：send_raw 路径，不进 history 缓存；CancelAgent → App::interrupt() |
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

### feature_20260327_F002_relay-command-sync
**摘要:** Web 端发 /compact 命令及 Agent 侧 thread 状态双向同步
**关键决策:**
- 新增 WebMessage::CompactThread 和 RelayMessage::ThreadReset
- ThreadReset 使用 send_raw（不进历史缓存，不被 SyncRequest 重放）
- TUI clear/history/compact 完成后均触发 send_thread_reset
- 前端 /clear 双保险：本地立即清空 + 等待 ThreadReset 确认
**归档:** [链接](../../archive/feature_20260327_F002_relay-command-sync/)
**归档日期:** 2026-03-28

### feature_20260327_F001_web-ask-user-interrupt
**摘要:** 补全 AskUser 协议字段并支持 Web 端中断 Agent 运行
**关键决策:**
- AskUserQuestion 字段补全：tool_call_id/description/multi_select/options/allow_custom_input/placeholder
- 新增 WebMessage::CancelAgent，relay_ops 调用 App::interrupt()，清空弹窗状态
- 停止按钮仅在 agent.isRunning 时渲染
- AskUserResponse 改用 tool_call_id 为 key（原 description）
**归档:** [链接](../../archive/feature_20260327_F001_web-ask-user-interrupt/)
**归档日期:** 2026-03-28

### feature_20260327_F001_relay-mobile-layout
**摘要:** Relay Web 前端移动端完整适配含汉堡侧边栏和面板 Tab 切换
**关键决策:**
- 侧边栏抽屉：transform translateX + position:fixed，动画过渡，不占文档流
- 多面板 Tab 栏：has-tabs class 控制显示，activeMobilePane 状态同步
- 100dvh 适配虚拟键盘，modal 85dvh
- 桌面端隔离：@media(min-width:769px) display:none !important
**归档:** [链接](../../archive/feature_20260327_F001_relay-mobile-layout/)
**归档日期:** 2026-03-28

### feature_20260327_F001_preact-no-bundle-migration
**摘要:** 前端从命令式 DOM 迁移到 Preact+Signals+htm 声明式组件体系
**关键决策:**
- Preact + @preact/signals + htm，全部 esm.sh CDN，无构建工具
- Signals 状态：agents/layout/activePane/markedReady；Map 更新需替换引用触发重渲染
- useSignalValue(signal) 显式订阅规则（esm.sh 多版本 auto-tracking 失效）
- UMD 脚本（marked/hljs/DOMPurify）动态加载，markedReady signal 触发重渲染
- 组件架构：App/Sidebar/PaneContainer/Pane/MessageList/TodoPanel/HitlDialog/AskUserDialog
**归档:** [链接](../../archive/feature_20260327_F001_preact-no-bundle-migration/)
**归档日期:** 2026-03-28

### feature_20260327_F001_frontend-message-id-dedup
**摘要:** 前端消息基于 UUIDv7 ID 实现 upsert 去重防重复显示
**关键决策:**
- 新增 upsertMessage(agent, msg)：按 id 实现 upsert 语义（有则 merge，无则追加）
- user/assistant 消息用 id 去重；tool slot 消息维持 tool_call_id 逻辑不变
- 改动集中在 state.js 和 events.js，render.js 零改动
**归档:** [链接](../../archive/feature_20260327_F001_frontend-message-id-dedup/)
**归档日期:** 2026-03-28

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
- → [agent.md#feature_20260328_F001_ask-user-question-align](./agent.md#feature_20260328_F001_ask-user-question-align) — AskUser 协议字段对齐（agent 为主域，relay-server 前端同步更新）
