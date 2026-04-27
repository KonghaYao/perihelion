# Feature: 20260329_F003 - compact-thread-migration

## 需求背景

当前 `/compact` 命令在原地压缩上下文：调用 LLM 生成摘要，然后直接替换当前 Thread 的 `agent_state_messages`。这导致：

1. 旧对话历史被覆盖，无法回溯压缩前的完整上下文
2. 用户在 `/history` 中看不到压缩前的原始对话
3. TUI 和 Web 端没有明确的"新 Thread"切换感知

用户希望 compact 操作**创建新 Thread 并迁移数据**，旧 Thread 保留完整历史（归档），新 Thread 从摘要开始继续对话。

## 目标

- `/compact` 执行后，创建新 Thread，旧 Thread 完整保留
- 新 Thread 的 `agent_state_messages` 以 LLM 生成的摘要 System 消息开头
- TUI 和 Relay Web 前端均感知到 Thread 切换
- 用户可在 `/history` 中回溯旧 Thread 的完整对话

## 方案设计

### 整体流程

![compact 执行流程](./images/01-flow.png)

1. 用户输入 `/compact [instructions]`
2. 校验：`agent_state_messages` 非空且 LLM Provider 可用
3. 异步调用 `compact_task()` 生成摘要（复用现有逻辑，**不变**）
4. 摘要完成后（`CompactDone` 事件）：
   - **旧 Thread 不动**（数据已在之前持久化）
   - 调用 `ThreadStore::create_thread(ThreadMeta::new(&cwd))` 创建新 Thread
   - 新 Thread 的 `agent_state_messages = [System(摘要)]`
   - 调用 `ThreadStore::save_messages()` 持久化新 Thread 消息
   - TUI 切换：`current_thread_id = 新 ThreadId`，清空 `view_messages`，插入压缩提示 + 摘要
   - Relay 通知：发送新增的 `CompactDone` 事件（含新/旧 ThreadId + 摘要）
   - Web 端收到后切换到新 Thread 并显示归档提示
5. 清理 loading 状态，处理 `pending_messages`

### 数据模型与事件变更

#### AgentEvent 变更

```rust
// 旧：
CompactDone(String)

// 新：
CompactDone {
    summary: String,           // LLM 生成的摘要
    new_thread_id: ThreadId,   // 新创建的 Thread ID
}
```

`CompactError(String)` 保持不变。

#### RelayMessage 新增

```rust
// Agent → Web 新增事件
RelayMessage::CompactDone {
    summary: String,
    new_thread_id: String,
    old_thread_id: String,     // 用于 Web 端显示"从旧对话压缩而来"
}
```

Web 端收到后：
- 清空当前面板消息
- 显示系统提示："📦 上下文已从旧对话压缩，摘要如下..."
- 显示摘要内容
- 后续用户输入发送到新 Thread

#### WebMessage

现有的 `WebMessage::CompactThread`（Web → Agent 触发压缩）**保持不变**，行为与 `/compact` 命令一致。

#### ThreadStore

**无需变更**。`create_thread(ThreadMeta)` + `save_messages(thread_id, messages)` 已满足所有需求。

### view_messages 处理

新 Thread 的 `view_messages`：
1. 插入 System 提示："📦 上下文已压缩（从旧对话迁移到新 Thread）"
2. 插入摘要内容（用户可见）
3. 不保留旧 Thread 的消息视图（用户可在 `/history` 中查看旧 Thread）

## 实现要点

### 改动文件清单

| 文件 | 改动内容 |
|------|----------|
| `rust-agent-tui/src/app/events.rs` | `CompactDone` 从 `CompactDone(String)` 改为结构体变体 |
| `rust-agent-tui/src/app/agent_ops.rs` | `CompactDone` 分支：创建新 Thread、切换 `current_thread_id`、持久化、Relay 通知 |
| `rust-agent-tui/src/app/agent.rs` | `compact_task()` 签名不变，返回值通过 channel 传递 summary（Thread 创建在 poll_agent 侧） |
| `rust-relay-server/src/protocol.rs` | `RelayMessage` 新增 `CompactDone` 变体 |
| `rust-relay-server/web/components/events.js` | 新增 `compact_done` 事件处理 |
| `rust-relay-server/web/components/Pane.js` | compact_done 后 UI 更新（清空消息、显示摘要） |

### 关键实现步骤

1. **修改 `CompactDone` 事件结构**：改为携带 `{ summary, new_thread_id }`
2. **在 `poll_agent` 的 `CompactDone` 分支中**：
   - `block_in_place` + `block_on` 调用 `thread_store.create_thread()` 创建新 Thread（与 `ensure_thread_id` 模式一致）
   - 构造 `agent_state_messages = vec![BaseMessage::system(summary)]`
   - `block_on` 调用 `thread_store.save_messages()` 持久化
   - `self.current_thread_id = Some(new_thread_id)`
   - 清空 `view_messages`，插入压缩提示 + 摘要
   - 通过 Relay 发送 `CompactDone` 事件
3. **Relay Web 前端**：
   - `events.js` 新增 `compact_done` 类型处理
   - 清空当前面板消息列表
   - 显示压缩提示和摘要
   - 更新全局 state

### 异步处理注意事项

- `create_thread` 和 `save_messages` 是异步操作，但 `poll_agent` 在同步上下文中
- 使用 `tokio::task::block_in_place` + `Handle::current().block_on()` 模式（与 `ensure_thread_id`、`open_thread` 一致）
- 旧 Thread 无需任何操作（数据已在之前持久化）

## 验收标准

- [ ] `/compact` 执行后，旧 Thread 完整保留在 `/history` 列表中
- [ ] 新 Thread 的 `agent_state_messages` 以 System(摘要) 开头
- [ ] TUI 中显示压缩提示和摘要内容
- [ ] `current_thread_id` 已切换到新 Thread
- [ ] 新 Thread 消息已持久化到 SQLite
- [ ] compact 后用户输入的消息属于新 Thread
- [ ] Relay Web 前端收到 `CompactDone` 事件并正确切换
- [ ] 现有 headless 测试通过（`CompactDone` 事件构造方式更新）
- [ ] `pending_messages` 在 compact 后正确刷新到新 Thread
