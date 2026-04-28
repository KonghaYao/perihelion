# Feature: 20260428_F002 - 消息显示管线统一

## 需求背景

当前 TUI 的消息显示存在两条独立路径：

1. **流式对话路径**：`agent_ops.rs::handle_agent_event()` 手动操作 `view_messages` 和 `RenderEvent`，包含工具聚合、SubAgent 路由、参数格式化等逻辑
2. **历史恢复路径**：`thread_ops.rs::open_thread()` 调用 `MessagePipeline::messages_to_view_models()` 批量转换

`message_pipeline.rs` 定义了完整的 `MessagePipeline` 结构体（含 `push_chunk`/`tool_start`/`tool_end`/`done`/`reconcile`），但 **`agent_ops.rs` 完全没有使用它**。导致：

- 工具聚合行为不一致：流式期间手动聚合（line 256-310），恢复路径由 `aggregate_tool_groups()` 处理，逻辑不完全相同
- 两套并行逻辑维护成本高，改一处容易遗漏另一处
- SubAgent 路由逻辑在 `agent_ops.rs` 中通过 `subagent_group_idx` 手动管理，与 Pipeline 的 `subagent_stack` 重复

## 目标

- `MessagePipeline` 成为消息状态管理的**唯一入口**
- `agent_ops.rs` 不再手动操作 `view_messages`，只负责将 `PipelineAction` 映射到 `RenderEvent`
- 流式和恢复路径共享同一个转换函数 `messages_to_view_models()`
- 保留 `AppendChunk` 流式优化，在 finalize 边界 reconcile 确保一致性
- SubAgent 内部消息保持现状（不持久化，滑动窗口 max 4）

## 方案设计

### 整体架构

```
                    ┌─────────────────────────────────┐
 AgentEvent ──────→ │  MessagePipeline                │
                    │  ├─ push_chunk()                │
                    │  ├─ push_reasoning()            │
                    │  ├─ tool_start()                │
                    │  ├─ tool_end()                  │
                    │  ├─ done()                      │
                    │  └─ reconcile()                 │
                    └──────────┬──────────────────────┘
                               │ PipelineAction
                               ▼
                    ┌─────────────────────────────────┐
                    │  agent_ops (瘦代理)              │
                    │  PipelineAction → RenderEvent   │
                    │  + 跨切面逻辑 (spinner/langfuse) │
                    └──────────┬──────────────────────┘
                               │ RenderEvent
                               ▼
                    ┌─────────────────────────────────┐
                    │  渲染线程                         │
                    └─────────────────────────────────┘
```

### 1. AppCore 持有 MessagePipeline

```rust
// app/core.rs
pub struct AppCore {
    pub pipeline: MessagePipeline,  // 新增：统一消息管线
    pub view_messages: Vec<MessageViewModel>,  // 保留：渲染层直接读取
    // subagent_group_idx 移除，由 pipeline.in_subagent() 替代
    // ...
}
```

### 2. handle_agent_event 改造

改造前（当前）：

```rust
fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
    match event {
        AgentEvent::ToolCall { name, display, args, is_error } => {
            // 手动判断 subagent_group_idx
            // 手动聚合 ToolCallGroup
            // 手动 push view_messages
            // 手动 send RenderEvent
        }
        AgentEvent::AssistantChunk(chunk) => {
            // 手动判断 subagent_group_idx
            // 手动 append_chunk
            // 手动 send RenderEvent
        }
        // ...
    }
}
```

改造后：

```rust
fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
    // 1. 跨切面逻辑（spinner、langfuse 等）
    self.handle_cross_concerns(&event);

    // 2. 通过 Pipeline 处理事件
    let actions = self.core.pipeline.handle_event(event);

    // 3. 将 PipelineAction 映射到 RenderEvent + view_messages 更新
    for action in actions {
        self.apply_pipeline_action(action);
    }
}
```

### 3. PipelineAction → RenderEvent 映射

```rust
fn apply_pipeline_action(&mut self, action: PipelineAction) {
    match action {
        PipelineAction::AddMessage(vm) => {
            self.core.view_messages.push(vm.clone());
            let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
        }
        PipelineAction::AppendChunk(chunk) => {
            // 流式优化：直接操作渲染线程
            let _ = self.core.render_tx.send(RenderEvent::AppendChunk(chunk));
        }
        PipelineAction::UpdateLast(vm) => {
            if let Some(last) = self.core.view_messages.last_mut() {
                *last = vm.clone();
            }
            let _ = self.core.render_tx.send(RenderEvent::UpdateLastMessage(vm));
        }
        PipelineAction::RemoveLast => {
            self.core.view_messages.pop();
            let _ = self.core.render_tx.send(RenderEvent::RemoveLastMessage);
        }
        PipelineAction::RemoveLastN(n) => {
            for _ in 0..n {
                self.core.view_messages.pop();
            }
            let _ = self.core.render_tx.send(RenderEvent::RemoveLastN(n));
        }
        PipelineAction::RebuildAll(vms) => {
            self.core.view_messages = vms.clone();
            let _ = self.core.render_tx.send(RenderEvent::LoadHistory(vms));
        }
        PipelineAction::StreamingDone => {
            if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                self.core.view_messages.last_mut()
            {
                *is_streaming = false;
            }
            let _ = self.core.render_tx.send(RenderEvent::StreamingDone);
        }
        PipelineAction::None => {}
    }
}
```

### 4. MessagePipeline 新增方法

当前 `MessagePipeline` 的方法签名是分散的（`push_chunk`、`tool_start` 等），需要新增统一的 `handle_event` 入口：

```rust
impl MessagePipeline {
    /// 统一事件处理入口：将 AgentEvent 转换为 PipelineAction 列表
    pub fn handle_event(&mut self, event: AgentEvent) -> Vec<PipelineAction> {
        match event {
            AgentEvent::AssistantChunk(chunk) => {
                self.push_chunk(&chunk);
                if self.in_subagent() {
                    // SubAgent 内部：更新 SubAgentGroup 的 recent_messages
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    // 父 Agent：流式追加
                    vec![PipelineAction::AppendChunk(chunk)]
                }
            }
            AgentEvent::ToolCall { tool_call_id, name, display, args, is_error } => {
                // 区分 ToolStart（is_error=false, 有 args）和 ToolEnd（is_error 或结果）
                self.handle_tool_call(tool_call_id, name, display, args, is_error)
            }
            AgentEvent::SubAgentStart { agent_id, task_preview } => {
                let vm = self.subagent_start(agent_id, task_preview);
                vec![PipelineAction::AddMessage(vm)]
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                let action = self.subagent_end(result, is_error);
                vec![action]
            }
            AgentEvent::Done => {
                self.done();
                // Done 时 reconcile 确保最终状态一致
                let vms = self.reconcile();
                vec![PipelineAction::RebuildAll(vms)]
            }
            AgentEvent::Error(e) => {
                // Error 不走 Pipeline，由 agent_ops 直接处理
                vec![PipelineAction::None]
            }
            // ... 其他事件类型
        }
    }
}
```

### 5. 工具聚合统一

**当前问题**：`agent_ops.rs` 手动实现工具聚合（line 256-310），与 `message_view.rs` 的 `aggregate_tool_groups()` 逻辑重复且不一致。

**改造方案**：移除 `agent_ops.rs` 中的手动聚合，在 `MessagePipeline::tool_end()` 中不返回聚合相关的 `PipelineAction`，而是在 `reconcile()` 时由 `aggregate_tool_groups()` 统一处理。

流式期间的 UX：工具调用到达时显示独立的 ToolBlock（通过 `PipelineAction::AddMessage`），在 reconcile 时由 `aggregate_tool_groups()` 聚合。视觉上用户会先看到独立的 ToolBlock，然后在下一个 finalize 边界（ToolStart 或 Done）看到聚合后的 ToolCallGroup。

### 6. AgentEvent 调整

当前 `AgentEvent::ToolCall` 合并了 ToolStart 和 ToolEnd 的语义（通过 `is_error` 区分）。需要拆分或增加字段以让 Pipeline 能正确处理：

```rust
pub enum AgentEvent {
    /// 工具调用开始（参数已就绪）
    ToolStart {
        tool_call_id: String,
        name: String,
        display: String,
        args: String,  // 格式化后的参数
        input: serde_json::Value,  // 原始输入（用于 cwd 缩短）
    },
    /// 工具调用结果
    ToolEnd {
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    // ... 其他事件不变
}
```

这需要在 `agent.rs::map_executor_event()` 中同步调整。

### 7. 流式优化保留策略

`AssistantChunk` 的 `AppendChunk` 优化保留。具体做法：

- `view_messages` 在流式期间仍由 `apply_pipeline_action` 维护（与当前行为一致）
- 渲染线程通过 `AppendChunk` 增量更新，避免每字符重做 Markdown
- 在 `Done` 事件时，Pipeline 调用 `reconcile()` 重建完整 `view_messages`，确保最终状态与恢复路径一致
- 在 `ToolStart` 时也可选 reconcile（将已完成的 AssistantBubble 从增量状态转为规范状态）

### 8. 历史恢复路径

```rust
// thread_ops.rs::open_thread()
pub fn open_thread(&mut self, thread_id: ThreadId) {
    let base_msgs = store.load_messages(&tid).unwrap_or_default();

    // 通过 Pipeline 初始化
    self.core.pipeline.clear();
    self.core.pipeline.set_completed(base_msgs.clone());

    // 统一转换
    self.core.view_messages =
        MessagePipeline::messages_to_view_models(&base_msgs, &self.cwd);

    // ...
}
```

### 9. StateSnapshot 处理

当前 `StateSnapshot` 更新 `agent.agent_state_messages`。改造后同时更新 Pipeline：

```rust
AgentEvent::StateSnapshot(msgs) => {
    self.agent.agent_state_messages = msgs.clone();
    self.core.pipeline.set_completed(msgs);
}
```

## 改动文件清单

| 文件 | 改动内容 |
|------|----------|
| `app/core.rs` | 添加 `pipeline: MessagePipeline` 字段，移除 `subagent_group_idx` |
| `app/message_pipeline.rs` | 新增 `handle_event()` 统一入口；调整 `tool_start`/`tool_end` 签名（接收原始 input）；新增 `subagent_start`/`subagent_end` 方法 |
| `app/agent_ops.rs` | 重构 `handle_agent_event`：事件 → Pipeline → PipelineAction → RenderEvent；移除手动工具聚合（line 256-310）和 SubAgent 路由逻辑；移除对 `subagent_group_idx` 的直接操作 |
| `app/events.rs` | 拆分 `ToolCall` 为 `ToolStart` + `ToolEnd`；`ToolStart` 增加 `input: serde_json::Value` 字段 |
| `app/agent.rs` | 调整 `map_executor_event()`：`ExecutorEvent::ToolStart` → `AgentEvent::ToolStart`，`ExecutorEvent::ToolEnd` → `AgentEvent::ToolEnd` |
| `app/thread_ops.rs` | `open_thread` 通过 Pipeline 初始化（`set_completed`） |
| `ui/message_view.rs` | 无改动（`messages_to_view_models` 保持不变） |

## 不在范围内

- SubAgent 内部消息持久化（保持现状：不持久化，滑动窗口 max 4）
- 渲染线程内部逻辑改动
- BaseMessage 层改动

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| 流式显示闪烁（reconcile 重建 view_messages） | 只在 Done 时 reconcile；ToolStart 时可选择性 reconcile 或保持增量 |
| Pipeline 与 agent_ops 职责边界模糊 | Pipeline 只负责消息状态管理，跨切面逻辑（spinner/langfuse/token tracking）保留在 agent_ops |
| AgentEvent 拆分影响面广 | `ToolStart`/`ToolEnd` 拆分仅在 TUI 内部，不影响核心层 |
| 测试覆盖不足 | 改造后补充测试：流式 vs 恢复一致性、工具聚合行为、SubAgent 路由 |
