# 同步 SubAgent Ctrl+C 中断——排查与修复记录

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-26
**修复日期**：2026-05-26
**关联 Issue**：[#2026-05-25-ctrl-c-cannot-interrupt-sync-subagent](./2026-05-25-ctrl-c-cannot-interrupt-sync-subagent.md)
**修复 Commit**：`2d6c88a`

## 根因

**`peri-tui/src/app/agent_ops/lifecycle.rs:145-156`，`handle_interrupted()` 中的 `in_subagent()` 守卫静默吞掉了父 Agent 的 Interrupted 事件。**

当用户在同步 SubAgent 执行期间按 Ctrl+C 时：
1. cancel token 正确传播 ✅
2. 父 agent 的 `tool_dispatch.rs:234` select! 正确触发 ✅
3. `AgentExecutionFailed { "Interrupted by user" }` 事件正确发送 ✅
4. 事件经过 event pump → client pump → notification channel → `poll_agent()` 全部到达 ✅
5. `map_executor_event()` 正确映射为 `AgentEvent::Interrupted` ✅
6. **`handle_interrupted()` 被 `in_subagent()=true` 拦截，return `(false, false, false)`** ❌
7. Pipeline cleanup 从未执行，UI 停留在 loading 状态
8. 仅靠 5s `cancel_sent_at` 超时兜底（极差的用户体验）

## 排查过程

### 阶段 1：错误假设——cancel token 没传播

最初假设 cancel token 没有传递到子 agent，在两个位置添加了 `tokio::select!` 包装：
- `peri-middlewares/src/subagent/tool/define.rs:1157`（子 agent 的 execute 调用）
- `peri-acp/src/session/executor.rs:468`（父 agent 的 execute 调用）

**结果：完全无效。** 所有改动已回退。

**教训：先验证假设再动手写代码。** cancel token 链路本身是正确的——`session.cancel_token → cancel.clone() → build_agent → SubAgentMiddleware → SubAgentTool → child_token()`，每一环都是 `CancellationToken` 的 `clone()` 或 `child_token()`，共享同一个 `Arc<Inner>`。

### 阶段 2：逐步追踪——逐步排除

每一步只加一个 WARN 级别的 tracing 日志，要求用户复现后查看。这种方式效率极低（需要 4-5 轮复现），但每轮都能排除一个假设：

| 诊断点 | 文件 | 结果 |
|--------|------|------|
| `handle_notification` 收到 `$/cancel_request` | `notify.rs` | ✅ 到达 |
| 父 agent `tool_dispatch.rs:234` select! 触发 | `tool_dispatch.rs` | ✅ 触发 |
| event pump 发出 `AgentExecutionFailed` | `executor.rs` | ✅ 发出 |
| client pump 收到并发送到 notification channel | `client.rs` | ✅ 发送 |
| `poll_agent()` 从 channel 取到通知 | `polling.rs` | ✅ 取到 |
| `map_executor_event()` 映射结果 | `agent.rs` | ✅ 映射为 Interrupted |
| `handle_interrupted()` 被调用 | `lifecycle.rs` | ✅ 调用了 |
| **`in_subagent()` 检查** | `lifecycle.rs` | **❌ 返回 true → 事件被丢弃** |

### 阶段 3：根因确认

最终诊断链路（一次到位的完整日志）：

```
[CANCEL-FIRE] parent tool_dispatch select! cancel branch fired     ✅
[CANCEL-TRACE] executor: sending AgentExecutionFailed              ✅
[CANCEL-TRACE] event pump: push_event sent to transport            ✅
[CANCEL-TRACE] client pump: received → sending to TUI             ✅
[CANCEL-TRACE] poll_agent() acp_result_empty=false                 ✅
[CANCEL-TRACE] map_executor_event: "Interrupted by user"          ✅
[CANCEL-TRACE] ACP→TUI: dispatching Interrupted/Error event        ✅
[CANCEL-TRACE] handle_interrupted() called                         ✅
[CANCEL-TRACE] handle_interrupted: in_subagent=true                ❌ ← 根因
```

## 修复

移除 `handle_interrupted()` 中的 `in_subagent()` 早返回守卫。改为 fall-through 到正常 cleanup 流程。

`in_subagent()` 守卫的设计意图是忽略**子 agent 自身**的中断（如 background agent 被取消），但它也错误地捕获了**父 agent 在 sync SubAgent 执行期间的 Ctrl+C 中断**——这恰恰是用户想要取消的操作。

## 排查经验总结

1. **不要预设根因位置。** 初始假设是 "cancel token 没传播到子 agent"，但实际根因在最末端的 TUI 事件处理层。cancel token 链路、事件传递链路全部正确。
2. **二分法追踪比深度假设更高效。** 从信号链的中点开始追踪（父 agent 的 select! 是否触发），而不是从起点（cancel token 创建）或终点（UI 状态）开始。
3. **一次到位的完整诊断 > 多轮逐步追踪。** 第一次尝试应该就在链路的每个关键节点都加日志，而不是每轮只加一个。
4. **`tool_dispatch.rs:234` 的已有 `select!` 就是足够的。** 不需要在 `define.rs` 或 `executor.rs` 再加额外的 `select!` 包装——cancel token 的 `child_token()` 机制工作正常，父 agent 层面的 select! 已经能中断子 agent。
5. **"UI 卡住"不等于 "信号没到"。** TUI 收到了 Interrupted 事件，只是处理逻辑把它丢弃了。症状和根因可能在不同层级。
