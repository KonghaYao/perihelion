# Feature: 20260326_F010 - relay-loading-state-sync

## 需求背景

Relay Server 前端（Web 页面）无法感知 Agent 当前是否正在执行，导致：
- 用户发送消息后看不到任何反馈，不知道 Agent 是否收到并处理中
- 断线重连后无法复原 loading 状态（Agent 可能仍在执行中）
- sync_response 重放历史时不携带执行状态信息

## 目标

- Agent 开始/结束执行时，通过 Relay 通知 Web 前端
- 前端显示「正在思考…」状态提示
- 重连/刷新页面后能从历史事件中正确还原 loading 状态

## 方案设计

### 协议层：两个新事件类型

后端通过现有的 `relay.send_value(json!(...))` 路径发送两个新事件。这两个事件会自动注入 seq 并缓存进 history（最多 1000 条），无需修改 `AgentEvent` enum 或 `RelayMessage` enum。

| 事件类型 | 触发时机 | 方向 |
|---------|---------|------|
| `{ "type": "agent_running", "seq": N }` | 用户消息开始处理前 | Agent → Relay → Web |
| `{ "type": "agent_done", "seq": N }` | 执行完毕（Done / Error / Interrupted） | Agent → Relay → Web |

### 后端改动（`rust-agent-tui/src/app/agent.rs`）

在 `run_universal_agent` 调用的外层包裹发送：

```rust
// 执行开始
relay.send_value(serde_json::json!({ "type": "agent_running" }));

let result = run_universal_agent(...).await;

// 执行结束（Done / Error / Interrupted 统一在此发送）
relay.send_value(serde_json::json!({ "type": "agent_done" }));
```

Done / Interrupted / Error 三条分支中，`agent_done` 在 `run_universal_agent` 返回后统一发送一次，无需每条分支单独处理。

### 前端改动

#### 1. `state.js` — agent 初始状态加字段

```js
agents.set(sessionId, {
  ...,
  isRunning: false,  // 新增
});
```

#### 2. `events.js` — handleLegacyEvent 新增两个 case

```js
case 'agent_running':
  agent.isRunning = true;
  break;

case 'agent_done':
  agent.isRunning = false;
  break;

case 'error':
  agent.isRunning = false;  // 错误也视为结束
  // ... 原有逻辑
  break;
```

#### 3. `render.js` — 输入栏状态文字

在 `renderPane` 的输入栏（`.pane-input`）区域左侧，当 `agent.isRunning === true` 时显示状态文字：

```html
<span class="agent-status thinking">正在思考…</span>
```

`isRunning === false` 时隐藏该元素。

![前端状态流转图](./images/01-state-flow.png)

### Sync 重放行为

新事件携带 seq，纳入历史缓存。当 Web 客户端重连并发送 `sync_request` 时，会收到完整历史（含 `agent_running`/`agent_done`）。前端按顺序重放：

- 历史末尾为 `agent_running` → 恢复 `isRunning = true`（Agent 执行中）
- 历史末尾为 `agent_done` → 恢复 `isRunning = false`（空闲）

## 实现要点

- **不修改核心枚举**：使用 `send_value` 发送原始 JSON，不需要修改 `AgentEvent`（rust-create-agent）或 `RelayMessage` 枚举，避免跨 crate 改动
- **三分支统一发 agent_done**：`run_universal_agent` 返回后统一发送，不在 Done/Error/Interrupted 各分支重复处理
- **前端输入不禁用**：仅显示状态文字，用户仍可继续输入（按需可后续增强为禁用）
- **renderPane 不重建**：`isRunning` 变化通过 `renderMessages`/状态文字更新反映，不触发全量 pane 重建

## 约束一致性

- 遵循事件驱动通信架构：新事件走现有 `send_value` → 带 seq → 缓存 → relay 转发路径
- 不引入共享可变状态：loading 状态从事件流派生，保持单向数据流
- 符合 WebSocket JSON 消息帧规范（axum 0.8）

## 验收标准

- [ ] 用户发送消息后，前端显示「正在思考…」
- [ ] Agent 回复完毕后，「正在思考…」消失
- [ ] Agent 执行中刷新页面，重连后仍显示 loading 状态
- [ ] Agent 出错时，loading 状态正确清除
- [ ] `agent_running`/`agent_done` 事件在 history 中有 seq，可被 sync_response 重放
