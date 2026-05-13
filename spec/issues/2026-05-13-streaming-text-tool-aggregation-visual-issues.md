# 流式渲染视觉问题：多轮 AI 文本合并在一个气泡

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-13

## 问题描述

流式渲染过程中，多轮 AI 回复文本被合并在同一个 AssistantBubble 中，缺乏轮次分隔。所有 ReAct 迭代的 AI 文本和工具调用堆在一个 streaming bubble 里，直到 Done 后 reconcile 才正确分离。

## 根因分析

**`map_executor_event` 丢弃了 ReAct 循环中间的 StateSnapshot。**

`rust-agent-tui/src/app/agent.rs:551` 中 `map_executor_event` 将 `ExecutorEvent::StateSnapshot(_)` 映射为 `None`，导致 ReAct 循环中间 `emit_snapshot_and_drain_notifications()` 发射的所有 StateSnapshot 被静默丢弃。

**因果链**：

1. `emit_snapshot_and_drain_notifications()` 每次工具调用后发射 `ExecutorEvent::StateSnapshot`
2. 事件经过 `FnEventHandler` → `map_executor_event()` 转换
3. `map_executor_event` 对 `StateSnapshot` 返回 `None`，事件被丢弃
4. TUI 侧 `handle_event(StateSnapshot)` 从未被调用
5. `set_completed()` 从未被调用 → `has_snapshot_this_round` 始终为 `false`
6. `build_tail_vms()` 始终走 "仅 streaming" 路径，所有轮次堆在一个 bubble 里
7. 只有 `run_universal_agent` 末尾直接 `tx.send(AgentEvent::StateSnapshot)` 绕过 `map_executor_event` 的最终 StateSnapshot 才到达 TUI

**诊断日志证据**（07:14 时间段，11 个 Bash 调用的对话）：

```
07:14:01 has_snapshot=false, completed_len=0, current_ai_tool_calls=8
07:14:04 has_snapshot=false, completed_len=0, current_ai_tool_calls=9
07:14:12 has_snapshot=false, completed_len=0, current_ai_tool_calls=11
07:14:15 has_snapshot=false, completed_len=0, current_ai_tool_calls=11
07:14:17 has_snapshot=false, completed_len=0, current_ai_tool_calls=11
07:14:20 has_snapshot=true,  completed_len=20  ← 最终 StateSnapshot 到达
```

## 修复

在 `map_executor_event` 中将 `ExecutorEvent::StateSnapshot(msgs)` 映射为 `AgentEvent::StateSnapshot(msgs)`，使 ReAct 循环中间的增量 StateSnapshot 能到达 TUI pipeline。

## 相关代码

- `rust-agent-tui/src/app/agent.rs:550` — **修复点**：`ExecutorEvent::StateSnapshot(msgs) => AgentEvent::StateSnapshot(msgs)`
- `rust-create-agent/src/agent/executor/final_answer.rs:38-53` — `emit_snapshot_and_drain_notifications()` 每次工具调用后发射 StateSnapshot
- `rust-agent-tui/src/app/agent.rs:442` — `run_universal_agent` 末尾直接发送最终 StateSnapshot（绕过 map_executor_event）
- `rust-agent-tui/src/app/message_pipeline.rs:301-303` — `handle_event(StateSnapshot)` 调用 `set_completed()`
- `rust-agent-tui/src/app/message_pipeline.rs:744-753` — `set_completed()` 清空 `current_ai_text` 并设 `has_snapshot_this_round = true`

## 复现条件

- **复现频率**：必现（任何多轮工具调用的对话）
- **触发步骤**：
  1. 发起一个需要多轮工具调用的请求
  2. 观察 agent 执行过程中所有 AI 文本和工具调用堆在一个气泡里
  3. Done 后 reconcile 正确分离各轮次内容
