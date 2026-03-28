# Feature: 20260327_F001 - frontend-message-id-dedup

## 需求背景

前端 `agent.messages` 是一个追加数组，当前去重逻辑不完整：

- `handleSingleEvent` 通过 `seq` 字段去重，但写入 `agent.messages` 时 assistant 消息不存 `seq`，tool slot 消息完全无标识字段，导致去重失效
- 断线重连后收到 `sync_response`，历史消息被重新插入一遍（重复）
- 实时 `MessageBatch` 与 `sync_response` 可能下发同一条消息（重复）

服务端已为每条 `BaseMessage` 分配 UUIDv7 `id`，前端应利用此 ID 实现 upsert 语义，保证消息不重复。

## 目标

- 引入 `upsertMessage(agent, msg)` 辅助函数，基于消息 `id` 实现 upsert：有则更新、无则追加
- `user`/`assistant` 消息统一使用 `id` 去重，并在消息对象中保留 `id` 字段
- `tool` slot 消息（`type: 'tool'`，通过 `tool_call_id` 配对）**维持现有逻辑不变**
- 改动集中在 `state.js` 和 `events.js`，不触动 `render.js`

## 方案设计

### 架构概览

![消息去重数据流](./images/01-flow.png)

消息处理链路：

```
WebSocket onmessage
  └─ handleAgentEvent(sessionId, msg)
       ├─ sync_response: forEach → handleSingleEvent
       └─ 实时事件: handleSingleEvent
            └─ handleBaseMessage / handleLegacyEvent
                 ├─ user/assistant 角色 → upsertMessage(agent, msg)  ← 新增
                 └─ tool 角色      → 按 tool_call_id 匹配修改 output  ← 不变
```

### upsertMessage 函数（state.js）

在 `state.js` 中新增导出函数：

```javascript
/**
 * 按 id 去重地将消息写入 agent.messages。
 * - 有 id 且已存在：合并更新（保留已有 output/streaming 等状态）
 * - 其他情况：追加
 */
export function upsertMessage(agent, msg) {
  if (msg.id) {
    const idx = agent.messages.findIndex(m => m.id === msg.id);
    if (idx !== -1) {
      agent.messages[idx] = { ...agent.messages[idx], ...msg };
      return;
    }
  }
  agent.messages.push(msg);
}
```

### events.js 变更

| 位置 | 现状 | 变更 |
|------|------|------|
| `handleBaseMessage` user role | `agent.messages.push({ type:'user', text, seq })` | 改为 `upsertMessage(agent, { type:'user', text, id: event.id, seq })` |
| `handleBaseMessage` assistant role（无 tool_calls） | `agent.messages.push({ type:'assistant', text, streaming:false, id: event.id })` | 改为 `upsertMessage(agent, { type:'assistant', text, streaming:false, id: event.id })` |
| `handleBaseMessage` tool role | 按 `tool_call_id` 反向查找并写 output | **不变** |
| `handleLegacyEvent` text_chunk | 已有 `message_id` 检查（`messages.some(m => m.id === msgId)`） | **不变**，兼容旧格式 |

> **注意：** `handleBaseMessage` 中 assistant 有 `tool_calls` 时创建的 tool slot（`type:'tool'`）不通过 upsertMessage，继续 push；它们由 tool role 事件通过 `tool_call_id` 匹配更新，不存在重复问题。

### 不变的内容

- `render.js` 全文不变，仍遍历 `agent.messages` 数组
- `handleSingleEvent` 中基于 `seq` 的事件级去重**保留**（与消息级 ID 去重互补）
- Legacy event 路径（`handleLegacyEvent`）整体不变，仅 text_chunk 的已有 message_id 检查继续生效

## 实现要点

- `upsertMessage` 使用 spread merge（`{ ...old, ...new }`），确保新字段覆盖旧字段，同时保留 old 中新对象未携带的字段（如 tool slot 的 `output`）
- 由于 `agent.messages` 数组长度通常较短（单会话消息数），`findIndex` 的 O(n) 代价可接受；无需引入 Map
- `user` 消息现在也携带 `id`，有助于将来按 ID 定位或编辑用户消息

## 约束一致性

- 纯前端 JS（`rust-relay-server/web/js/`），不涉及 Rust 技术栈，无架构约束冲突
- 修改范围：`state.js`（新增 `upsertMessage`）、`events.js`（调用变更），符合现有模块职责划分

## 验收标准

- [ ] 断线重连后触发 `sync_response`，历史消息不出现重复条目
- [ ] 实时推送与 `sync_response` 重叠时，重叠消息不重复显示
- [ ] `upsertMessage` 对同一 `id` 的消息执行 merge 而非追加
- [ ] 无 `id` 的消息（legacy 格式）仍按原逻辑追加，不受影响
- [ ] `render.js` 无任何改动，渲染逻辑不受影响
