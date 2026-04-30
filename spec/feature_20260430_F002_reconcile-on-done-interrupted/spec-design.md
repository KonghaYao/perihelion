# Feature: 20260430_F002 - Done/Interrupted 事件 Reconcile 修复

## 需求背景

`MessagePipeline`（设计文档 `F002_message-pipeline-unify`）的核心承诺是：

> 流式和恢复路径共享同一个转换函数 `messages_to_view_models()`

当前实现中 `Done` 和 `Interrupted` 事件处理违反了这一承诺：

```rust
// message_pipeline.rs:223-226 (当前 buggy 代码)
AgentEvent::Done => {
    self.done();
    vec![PipelineAction::StreamingDone]  // 只标记流结束，没有 reconcile
}
AgentEvent::Interrupted => {
    self.interrupt();
    vec![PipelineAction::None]  // 完全没通知 UI 重建
}
```

导致的问题：
1. `StreamingDone` 只将最后一条 `AssistantBubble.is_streaming` 设为 `false`，不重建内容
2. `Interrupted` 返回 `None`，UI 完全不知道需要更新
3. 流式增量路径（`AppendChunk`）和恢复路径（`messages_to_view_models` → `aggregate_tool_groups`）的结果可能不一致

## 目标

- `Done` 和 `Interrupted` 事件触发 `reconcile()`，确保流式最终状态与恢复路径一致
- 只重建当前轮次（最后一条 Human 消息之后）的 view_models，避免全量重建导致的性能浪费和闪烁
- 移除 `StreamingDone` 变体（职责合并到 `RebuildAll`）

## 方案设计

### 1. PipelineAction::RebuildAll 扩展

```rust
pub enum PipelineAction {
    // ...
    /// 全量重建（工具聚合变更等）
    RebuildAll(Vec<MessageViewModel>),
}
```

改为：

```rust
pub enum PipelineAction {
    // ...
    /// 重建 view_messages 尾部
    /// - prefix_len: 保留 view_messages[..prefix_len] 不变
    /// - tail_vms: 新尾部，替换 view_messages[prefix_len..]
    RebuildAll {
        prefix_len: usize,
        tail_vms: Vec<MessageViewModel>,
    },
}
```

### 2. reconcile_tail() 方法

在 `MessagePipeline` 中新增方法，找到 `completed` 中最后一条 `Human` 消息的 index，只重建该 index 之后的部分：

```rust
impl MessagePipeline {
    /// 只重建当前轮次的 view_models（最后一条 Human 之后）
    pub fn reconcile_tail(&self) -> (usize, Vec<MessageViewModel>) {
        // 找到 completed 中最后一条 BaseMessage::Human 的 index
        let human_idx = self.completed.iter().rposition(|m| matches!(m, BaseMessage::Human { .. }))
            .map(|i| i + 1)  // Human 消息本身也包含在重建范围
            .unwrap_or(0);

        let tail_msgs = &self.completed[human_idx..];
        let tail_vms = Self::messages_to_view_models(tail_msgs, &self.cwd);
        (human_idx, tail_vms)
    }
}
```

### 3. Done / Interrupted 事件处理

```rust
AgentEvent::Done => {
    self.done();
    let (prefix_len, tail_vms) = self.reconcile_tail();
    vec![PipelineAction::RebuildAll { prefix_len, tail_vms }]
}
AgentEvent::Interrupted => {
    self.interrupt();
    let (prefix_len, tail_vms) = self.reconcile_tail();
    vec![PipelineAction::RebuildAll { prefix_len, tail_vms }]
}
```

### 4. apply_pipeline_action 适配

```rust
PipelineAction::RebuildAll { prefix_len, tail_vms } => {
    // 截断保留前段，替换尾部
    self.core.view_messages.truncate(prefix_len);
    self.core.view_messages.extend(tail_vms.clone());
    let _ = self.core.render_tx.send(RenderEvent::LoadHistory(self.core.view_messages.clone()));
}
```

### 5. 移除 StreamingDone

- 从 `PipelineAction` 枚举中移除 `StreamingDone` 变体
- 从 `apply_pipeline_action` 中移除对应的 match 分支
- 从 `RenderEvent` 中检查是否有 `StreamingDone`，如有也移除（或保留为内部使用但不从 PipelineAction 产生）

### 6. 前段长度的语义

`prefix_len` 对应的是 `completed` 数组中最后一条 Human 消息的 index（含）。但 `view_messages` 和 `completed` 不一定 1:1 对应（`messages_to_view_models` 会跳过无可见内容的 AssistantBubble）。

解决方案：**`prefix_len` 基于 `view_messages` 而非 `completed`**。在 `submit_message` 时记录当前 `view_messages.len()` 作为本轮起始位置（此时 Human VM 已 push），reconcile 时直接使用该值。

```rust
// AppCore
pub struct AppCore {
    // ...
    pub round_start_vm_idx: usize,  // 本轮 Human 消息在 view_messages 中的位置
}

// submit_message() 中
self.core.round_start_vm_idx = self.core.view_messages.len();  // push Human VM 之前
```

```rust
// reconcile_tail() 改为接受 round_start_idx
pub fn reconcile_tail(&self, round_start_idx: usize) -> Vec<MessageViewModel> {
    // 找到 completed 中最后一条 Human 的 index，从该位置开始重建
    let human_idx = self.completed.iter().rposition(|m| matches!(m, BaseMessage::Human { .. }))
        .unwrap_or(0);
    let tail_msgs = &self.completed[human_idx..];
    Self::messages_to_view_models(tail_msgs, &self.cwd)
}
```

```rust
// Done 事件
AgentEvent::Done => {
    self.done();
    let tail_vms = self.reconcile_tail(self.round_start_vm_idx);
    vec![PipelineAction::RebuildAll { prefix_len: self.round_start_vm_idx, tail_vms }]
}
```

## 改动文件清单

| 文件 | 改动内容 |
|------|----------|
| `app/message_pipeline.rs` | 移除 `StreamingDone` 变体；`RebuildAll` 改为 `{ prefix_len, tail_vms }` 结构体形式；`Done`/`Interrupted` 调用 `reconcile_tail()`；新增 `reconcile_tail()` 方法 |
| `app/agent_ops.rs` | `apply_pipeline_action` 中 `RebuildAll` 改为截断 + extend；移除 `StreamingDone` match 分支 |
| `app/mod.rs`（AppCore） | 新增 `round_start_vm_idx: usize` 字段 |
| `app/agent_ops.rs`（submit_message） | `push Human VM` 后记录 `round_start_vm_idx = view_messages.len()` |

## 不在范围内

- `reconcile()` 全量方法保留（其他场景仍需使用，如 `CompactDone`）
- `Error` 事件处理（`Error` 不走 Pipeline，由 agent_ops 直接处理，当前行为正确）
- 渲染线程内部逻辑改动

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| `round_start_vm_idx` 与 `completed` 中 Human 位置不一致 | `round_start_vm_idx` 在 `submit_message` 中记录，`reconcile_tail` 用 `completed.rposition(Human)` 找重建起点，两者独立但语义对齐（都是"本轮 Human"） |
| 尾部重建遗漏 Human VM 本身 | `completed.rposition(Human)` 返回的 index 包含 Human 消息，重建时 Human VM 会包含在 tail_vms 中 |
