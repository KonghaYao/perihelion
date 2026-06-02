# PostToolBatch 钩子未实现

**状态**：Open
**优先级**：低
**创建日期**：2026-06-01

## 问题描述

Claude Code 有 `PostToolBatch` 钩子事件，在一批并行工具调用全部完成后、下次模型请求前触发（每个 batch 一次）。Peri 未实现此事件。

## 预期行为

- 当 LLM 在一次响应中返回多个工具调用时，这些工具并行执行
- 所有工具的 PostToolUse/PostToolUseFailure 都触发完毕后，触发一次 PostToolBatch
- PostToolBatch 可 block（停止 agentic 循环）

## 当前行为

- 每个工具单独触发 PostToolUse/PostToolUseFailure
- 无 batch 级别的事件

## 影响范围

用户无法在整批工具完成后执行聚合操作（如批量日志、状态同步等）。

## 修复方向

1. `peri-middlewares/src/hooks/types.rs` — `HookEvent` 已有 `Unknown(String)` 兜底，但需显式添加 `PostToolBatch` 变体
2. `peri-agent/src/agent/tool_dispatch.rs` — `dispatch_tools` 中，所有 tool_result 写入 state 后、返回前触发
3. `peri-middlewares/src/hooks/middleware.rs` — 新增 `after_tools_batch` 方法或在现有流程中插入触发点

## 涉及文件

- `peri-middlewares/src/hooks/types.rs` — 新增 `PostToolBatch` 变体
- `peri-agent/src/agent/tool_dispatch.rs` — 批量工具完成后的触发点
- `peri-middlewares/src/hooks/middleware.rs` — 事件触发逻辑
