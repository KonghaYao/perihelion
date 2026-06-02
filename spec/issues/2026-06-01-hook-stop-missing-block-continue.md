# Stop 钩子缺少 block 继续工作语义

**状态**：Open
**优先级**：中
**创建日期**：2026-06-01

## 问题描述

Claude Code 中 Stop 钩子的 block 动作有特殊语义：将 reason 反馈给 Claude 让它继续工作（最多连续 8 次 block）。Peri 当前 Stop 钩子的 block 动作只是拒绝 agent 输出，不会让 agent 继续工作。

## 当前行为

```rust
// middleware.rs:522
let _action = self.fire_event(HookEvent::Stop, &input, None, None).await;
// action 被忽略，不影响后续流程
```

Stop 钩子的返回值被丢弃，无论 hook 返回 Allow/Block/PreventContinuation，都不会改变 agent 行为。

## 预期行为

| Hook 返回 | Claude Code 行为 | Peri 当前行为 |
|----------|-----------------|-------------|
| Allow | 正常结束 | 正常结束 ✓ |
| Block + reason | 将 reason 作为反馈注入，Claude 继续工作（最多连续 8 次） | 无效，正常结束 ✗ |
| PreventContinuation | 停止 | 无效，正常结束 ✗ |

## 修复方向

1. `after_agent` 中检查 Stop hook 返回的 action
2. 若为 `Block { reason }` 且连续 block 次数 < 8，将 reason 注入消息并让 agent 继续
3. 在 HookMiddleware 中维护 `stop_block_count: Arc<Mutex<u32>>` 计数器
4. 若连续 block 次数 >= 8，忽略 block 并正常结束

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `after_agent` 中 Stop 触发和返回值处理
