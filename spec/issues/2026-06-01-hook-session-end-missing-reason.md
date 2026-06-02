# SessionEnd 钩子缺少 reason 字段和部分触发场景

**状态**：Open
**优先级**：高
**创建日期**：2026-06-01

## 问题描述

SessionEnd 钩子缺少 Claude Code 官方的 `reason` 字段，且未覆盖所有应触发场景。

## 当前行为 vs 预期行为

| 场景 | Claude Code (reason) | Peri | 差异 |
|------|---------------------|------|------|
| `/clear` 新建 thread | `clear` | 触发（无 reason） | ✗ 缺 reason |
| 恢复其他会话（导致当前会话结束） | `resume` | 不触发 | ✗ 缺场景 |
| TUI 退出（Ctrl+C 双击） | `prompt_input_exit` | 触发（无 reason） | ✗ 缺 reason |
| `/quit` 退出 | `other` | 触发（无 reason） | ✗ 缺 reason |

## 影响范围

钩子脚本无法根据退出原因执行不同的清理逻辑。例如用户在 `herdr-agent-state.sh` 中需要区分 `clear`（状态变为 blocked）和 `prompt_input_exit`（状态变为 idle）。

## 根因分析

- `fire_standalone_lifecycle_hooks` 中 SessionEnd 的 `HookInput` 不包含 `source` 字段（设为 `None`）
- `thread_ops.rs:new_thread()` 和 `main.rs` 退出路径调用时未传递 reason
- 缺少 resume 场景的触发点

## 修复方向

1. `HookInput` 中增加 `source` 字段用于 SessionEnd reason
2. `new_thread()` 调用时传 `reason = "clear"`
3. TUI 退出时传 `reason = "prompt_input_exit"` 或 `"other"`
4. 检查 resume 会话时是否需要触发当前会话的 SessionEnd（`reason = "resume"`）
5. Claude Code 默认超时 1.5s，peri 当前无超时控制

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `fire_standalone_lifecycle_hooks` SessionEnd 分支
- `peri-tui/src/app/thread_ops.rs` — `new_thread()` 中 SessionEnd 触发
- `peri-tui/src/main.rs` — TUI 退出时 SessionEnd 触发
