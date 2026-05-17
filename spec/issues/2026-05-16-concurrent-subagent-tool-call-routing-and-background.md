# 并发 SubAgent 工具调用路由错误 + 死锁修复

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-16
**最新更新**：2026-05-17（修复完成）

## 问题描述

当父 Agent 在同一轮中并发调用多个 SubAgent 时：
1. 内部工具调用全部路由到最后一个 SubAgentGroup，其余为空
2. 并发执行时出现死锁（Ctrl+C 无法终止）
3. 同名 SubAgent（如 3×hello-agent）完成后 `in_subagent()` 仍为 true，Done 被拦截，loading 永不停止

## 根因分析

### 根因 1：TCP 死锁 + LLM 流式不可取消
LLM 调用期间不检查取消令牌，导致 Ctrl+C 需等数十秒 LLM 完成才生效。多 SubAgent 时累积延迟极长。

### 根因 2：位置路由（`last_mut()` / 位置匹配）
Pipeline 的所有 SubAgent 路由均依赖 `subagent_stack.last_mut()`，并发场景下永远返回最后入栈的 SubAgent。

### 根因 3：事件通道溢出静默丢弃
`mpsc::channel(256)` 容量不足，复杂 SubAgent 产生 500+ 事件时被 `try_send` 丢弃。

### 根因 4：同名 SubAgent 匹配缺陷
3 个 `hello-agent` 时，`find(|s| s.agent_id == "hello-agent")` 总命中第一个，
后续 `is_running` 永不清零，`in_subagent()=true` 拦截父 Agent 的 `Done`。

## 修复方案（7 个 commit，14 个文件）

### Fix 1: Agent 工具顺序执行（消除并发争用）
`peri-agent/src/agent/executor/tool_dispatch.rs`：
- 分离 Agent 调用和非 Agent 调用
- 非 Agent 工具保持并发（`join_all`），Agent 工具改为 `for` 循环逐个 `await`

### Fix 2: `source_agent_id` 事件打标 + 精确路由
- `peri-agent/src/agent/events.rs`：`ToolStart/ToolEnd/TextChunk` 加 `source_agent_id: Option<String>`
- `peri-middlewares/src/subagent/tool.rs`：`SourceAgentIdHandler` 包装器为子 Agent 事件注入 `source_agent_id`
- `peri-tui/src/app/message_pipeline.rs`：`ToolStart/ToolEnd/AssistantChunk` 按 `source_agent_id` 路由到 `find_running_subagent_mut(agent_id)`
- `SubAgentEnd` 按 `agent_id` 匹配对应 `tc_id`

### Fix 3: LLM 流式支持取消
- `peri-agent/src/llm/types.rs`：`StreamingContext` 加 `cancel: CancellationToken`
- `peri-agent/src/llm/anthropic/stream.rs` + `openai/stream.rs`：
  `while let` → `loop { tokio::select! { biased; cancel → Interrupted; stream.next() } }`

### Fix 4: `merge_frozen_subagents` 位置匹配 → agent_id 匹配
`peri-tui/src/app/message_pipeline.rs`：位置索引 → `HashMap<&str, &VM>` 精确匹配

### Fix 5: `SubagentStopped` → `SubAgentEnd` 精确路由
- `peri-agent/src/agent/events.rs`：`SubagentStopped` 加 `is_error: bool`
- `peri-tui/src/app/agent.rs`：`SubagentStopped` → `SubAgentEnd { agent_id: Some(name) }`
  移除 `ToolEnd("Agent")` → `SubAgentEnd` 映射（该路径无精确 agent_id）
- `peri-tui/src/app/agent_ops.rs`：`SubAgentEnd` 中 `subagent_depth==0` 时恢复 spinner

### Fix 6: `tool_end_internal` 同名 SubAgent 精准匹配
`find(|s| s.agent_id == target)` → `find(|s| s.agent_id == target && s.is_running)`

### Fix 7: 通道容量 256 → 4096
`agent_submit.rs`：`mpsc::channel(256)` → `mpsc::channel(4096)`

## 涉及文件

| 文件 | 变更摘要 |
|------|---------|
| `peri-agent/src/agent/events.rs` | `source_agent_id` 字段；`SubagentStopped.is_error` |
| `peri-agent/src/agent/executor/tool_dispatch.rs` | Agent 工具顺序执行 |
| `peri-agent/src/agent/executor/llm_step.rs` | StreamingContext 传 cancel |
| `peri-agent/src/llm/types.rs` | StreamingContext 加 cancel |
| `peri-agent/src/llm/anthropic/stream.rs` | 流式循环查取消 |
| `peri-agent/src/llm/openai/stream.rs` | 同上 |
| `peri-middlewares/src/subagent/tool.rs` | SourceAgentIdHandler；SubagentStopped 带 is_error |
| `peri-tui/src/app/agent.rs` | map_executor_event 路由重构 |
| `peri-tui/src/app/agent_ops.rs` | SubAgentEnd spinner 恢复 |
| `peri-tui/src/app/agent_submit.rs` | 通道 256→4096 |
| `peri-tui/src/app/message_pipeline.rs` | merge 按 agent_id；find 加 is_running |
| `peri-tui/src/app/message_pipeline_test.rs` | 测试更新 |
| `peri-tui/src/ui/headless_test.rs` | 测试更新 |

## 验证

- 全部 858 个测试通过
- 3×hello-agent 并发调用：所有卡片正确冻结，loading 停止
- Ctrl+C 在 LLM 流式期间立即生效

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>
