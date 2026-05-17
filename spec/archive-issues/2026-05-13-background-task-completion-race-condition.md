> 归档于 2026-05-17，原路径 spec/issues/2026-05-13-background-task-completion-race-condition.md
# Background task 完成后未触发 agent continuation（竞态条件）

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13

## 问题描述

当 agent 发起后台任务（`run_in_background: true`）后进入 Done 状态时，后台任务完成通知无法可靠触发 agent continuation。根本原因是 `BackgroundTaskCompleted` 和 `Done` 两个事件通过同一 channel 传递，存在竞态条件：如果后台任务在 Done 之前完成，`BackgroundTaskCompleted` 先被消费，此时 `agent_done_pending_bg` 尚未设置，导致 continuation 永远不会被触发。

## 症状详情

### 竞态路径分析

```
时间线 ──────────────────────────────────────────────────►

正常路径（后台任务慢于 Done）：
  Agent 调用 bg task → background_task_count = 1
  → LLM 决定 Done → Done 事件处理
  → background_task_count > 0 → agent_done_pending_bg = true
  → [等待 bg task]
  → BackgroundTaskCompleted → count = 0, pending_bg = true
  → ✅ 设置 pending_bg_continuation → 下一帧 submit_message

竞态路径（后台任务快于 Done）：
  Agent 调用 bg task → background_task_count = 1
  → bg task 极快完成 → BackgroundTaskCompleted 事件入队
  → poll_agent 消费 BackgroundTaskCompleted
  → background_task_count = 0
  → agent_done_pending_bg == false（Done 还没处理！）
  → ❌ 不设置 pending_bg_continuation
  → poll_agent 消费 Done
  → background_task_count == 0 → agent_rx = None
  → ❌ 后台任务结果丢失，agent 永远不会继续
```

### 实际影响

- 用户发起后台任务后，agent 显示 Done，但后台任务结果从未被处理
- 用户必须手动输入消息才能让 agent 看到后台任务结果
- 后台任务越快，越容易触发此 bug（如 code-reviewer、小规模搜索任务）

## 复现条件

- **复现频率**：必现（当后台任务在 Done 之前完成时）
- **触发步骤**：
  1. 向 agent 提交一个会发起后台任务的任务（如 code review、并行 sub-agent）
  2. 确保后台任务执行较快，在 agent 主循环结束之前完成
  3. 观察：agent 显示 Done，后台任务通知可能/可能不显示，但 agent 不会自动继续处理后台任务结果
- **环境**：任何使用 `run_in_background: true` 的场景

## 相关代码

- `peri-tui/src/app/agent_events_bg.rs:6-201` —— `handle_background_task_completed`，continuation 逻辑仅在 `agent_done_pending_bg == true` 时触发
- `peri-tui/src/app/agent_events_bg.rs:131-134` —— 竞态关键点：检查 `agent_done_pending_bg && background_task_count == 0`
- `peri-tui/src/app/agent_ops.rs:360-373` —— Done 事件处理：`background_task_count > 0` 时设 `agent_done_pending_bg = true`，否则 `agent_rx = None`
- `peri-tui/src/app/agent_ops.rs:835-848` —— `poll_agent` 中消费 `pending_bg_continuation`
- `peri-middlewares/src/subagent/tool.rs:505-557` —— 后台任务 spawn：`tokio::spawn` 中通过 `event_handler.on_event(BackgroundTaskCompleted)` 发送完成通知
- `peri-tui/src/app/agent_ops.rs:906-930` —— `Disconnected` 处理：静默清理后台任务场景

## 关联 Issue

- `spec/issues/2026-05-12-compact-auto-continue-scenarios.md`（状态：Open）—— 同为后台任务完成后的 continuation 问题，但聚焦于 auto-compact 的 resubmit 场景

## 实施计划

### 修复策略：消除 `BackgroundTaskCompleted` 对 `agent_done_pending_bg` 的时序依赖

核心思路：`BackgroundTaskCompleted` 处理器不再仅依赖 `agent_done_pending_bg`（一个由 `Done` 设置的时序耦合标志），而是**记住**已完成但 agent 尚未 Done 的后台任务。当 `Done` 到达时，检查是否有这些暂存结果。

### Step 1：新增字段 `pre_done_bg_completions`

**文件**: `peri-tui/src/app/agent_comm.rs`
**位置**: 第 63 行之后（`pending_bg_continuation` 之后）

```rust
/// Agent 尚未 Done 但后台任务已完成的通知缓存。
/// 修复 BackgroundTaskCompleted 与 Done 事件的竞态条件：
/// 当 BackgroundTaskCompleted 在 Done 之前被消费时，将显示通知暂存于此，
/// 待 Done 处理时检查此字段并设置 pending_bg_continuation。
pub pre_done_bg_completions: Vec<String>,
```

在 `Default::default()` 初始化：`pre_done_bg_completions: Vec::new()`

### Step 2：修改 `handle_background_task_completed` — 暂存逻辑

**文件**: `peri-tui/src/app/agent_events_bg.rs`
**位置**: 第 130-198 行

**当前逻辑**：

```
if agent_done_pending_bg && background_task_count == 0 {
    → 设 pending_bg_continuation
}
```

**修改为**：

```rust
if agent_done_pending_bg && background_task_count == 0 {
    // 原有逻辑不变：agent 已 Done 且所有后台任务完成
    ...
} else if !agent_done_pending_bg && background_task_count == 0 {
    // 新增：agent 尚未 Done，但所有后台任务已完成
    // 暂存通知，待 Done 处理时触发 continuation
    pre_done_bg_completions.push(display_notification);
}
```

**提取公共函数**：将 display_notification 构建逻辑（第 144-172 行）提取为内部辅助函数 `build_bg_display_notification()`，避免代码重复。

### Step 3：修改 Done 处理 — 检查暂存结果

**文件**: `peri-tui/src/app/agent_ops.rs`
**位置**: 第 360-373 行

**当前逻辑**：

```rust
if background_task_count > 0 {
    agent_done_pending_bg = true;
} else {
    agent_rx = None;
}
```

**修改为**：

```rust
if background_task_count > 0 {
    agent_done_pending_bg = true;
    // 原有 tracing::info 不变
} else {
    // 检查是否有暂存的后台任务完成通知（竞态修复）
    if !pre_done_bg_completions.is_empty() {
        let combined = pre_done_bg_completions.drain(..).collect::<Vec<_>>().join("\n");
        pending_bg_continuation = Some(combined);
    }
    agent_rx = None;
}
```

### Step 4：修改 Error 处理 — 对称修复

**文件**: `peri-tui/src/app/agent_ops.rs`
**位置**: Error 事件中 `background_task_count` 检查块

Error 路径同样存在此竞态。在 `background_task_count == 0` 的 else 分支中，增加 `pre_done_bg_completions` 非空检查，如有暂存通知则设置 `pending_bg_continuation`。

### Step 5：清理暂存字段

- **`agent_submit.rs`**（`submit_message` 状态重置）：追加 `pre_done_bg_completions.clear()`
- **`agent_compact.rs`**（`handle_compact_done` 状态重置）：追加 `pre_done_bg_completions.clear()`
- **`agent_ops.rs`**（`Disconnected` 后台任务静默清理，第 909-926 行）：追加 `pre_done_bg_completions.clear()`

### Step 6：测试

**文件**: `peri-tui/src/ui/headless_test.rs`

1. **`test_bg_completed_before_done_triggers_continuation`** — 核心竞态：BackgroundTaskCompleted → Done，验证 continuation 被设置
2. **`test_multiple_bg_completed_before_done`** — 多个后台任务在 Done 前完成，验证合并通知
3. **`test_bg_completed_after_done_unchanged`** — 正常路径不受影响：Done → BackgroundTaskCompleted
4. **`test_submit_message_clears_pre_done_completions`** — 用户主动发消息清理暂存

### 边界情况

| 场景 | 行为 |
|------|------|
| 多个后台任务部分完成 | `pre_done_bg_completions` 逐个追加；Done 时 `background_task_count > 0` 走原 `agent_done_pending_bg` 路径 |
| Disconnected | 清理 `pre_done_bg_completions`，与现有清理一致 |
| auto-compact | Done 中 `has_bg_tasks` 为 false 时正常触发；compact 完成时暂存已被消费 |
| Interrupted | `agent_done_pending_bg` 不设置，暂存在下次 `submit_message` 时清理 |

### 文件变更清单

| 文件 | 变更 |
|------|------|
| `peri-tui/src/app/agent_comm.rs` | 新增 `pre_done_bg_completions` 字段 |
| `peri-tui/src/app/agent_events_bg.rs` | 修改暂存逻辑 + 提取 `build_bg_display_notification()` |
| `peri-tui/src/app/agent_ops.rs` | Done/Error/Disconnected 处理检查暂存 |
| `peri-tui/src/app/agent_submit.rs` | `submit_message` 清理暂存 |
| `peri-tui/src/app/agent_compact.rs` | `handle_compact_done` 清理暂存 |
| `peri-tui/src/ui/headless_test.rs` | 新增 4 个测试 |
