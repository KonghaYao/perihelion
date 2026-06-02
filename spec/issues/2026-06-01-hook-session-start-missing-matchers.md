# SessionStart 钩子缺少 resume/clear/compact 触发场景

**状态**：Open
**优先级**：高
**创建日期**：2026-06-01

## 问题描述

SessionStart 钩子仅在 session 首次 prompt 时触发（`is_session_start == true`），但 Claude Code 官方行为中 SessionStart 支持 4 种 matcher：`startup`、`resume`、`clear`、`compact`。

## 当前行为 vs 预期行为

| 场景 | Claude Code | Peri | 差异 |
|------|------------|------|------|
| 新会话首次 prompt | 触发（matcher=`startup`） | 触发 | ✓ 一致 |
| 恢复历史会话（`-c`/`-r`） | 触发（matcher=`resume`） | 不触发 | ✗ 缺失 |
| `/clear` 后首次 prompt | 触发（matcher=`clear`） | 不触发 | ✗ 缺失 |
| compact 后 | 触发（matcher=`compact`） | 不触发 | ✗ 缺失 |

## 影响范围

用户在 `~/.claude/settings.json` 中配置的 SessionStart 钩子（如设置 agent 状态为 `idle`）在 resume/clear/compact 场景下不会被触发，导致状态指示器与实际不同步。

## 根因分析

- `peri-acp/src/session/executor.rs` 中 `hook_session_start` 仅依赖 `is_empty_history` 判断
- `HookMiddleware::before_agent` 中 `is_session_start` 为 bool 标记，不携带 matcher 信息
- `HookInput` 中 `source` 字段硬编码为 `"startup"`，无 resume/clear/compact 值

## 修复方向

1. `HookEvent::SessionStart` 增加 matcher 语义（通过 `HookInput.source` 传递）
2. executor 中识别 resume/clear/compact 场景并设置对应 matcher
3. 钩子脚本的 stdin JSON 中 `source` 字段应为 `startup`/`resume`/`clear`/`compact`

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `before_agent` 中 SessionStart 触发逻辑
- `peri-acp/src/session/executor.rs` — `hook_session_start` 参数传递
- `peri-middlewares/src/hooks/types.rs` — `HookInput` 的 `source` 字段
