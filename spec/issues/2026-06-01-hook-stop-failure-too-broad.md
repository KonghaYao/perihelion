# StopFailure 钩子触发范围过宽

**状态**：Open
**优先级**：低
**创建日期**：2026-06-01

## 问题描述

Claude Code 中 StopFailure 仅在 API 错误导致轮次结束时替代 Stop 触发。Peri 当前在 `on_error` 中对所有错误类型都触发 StopFailure，包括用户中断（Interrupted）和最大迭代次数超出（MaxIterationsExceeded）。

## 当前行为

```rust
// middleware.rs:531-564
async fn on_error(&self, _state: &mut S, error: &AgentError) -> AgentResult<()> {
    // 所有错误路径都触发 StopFailure
    // 包括：Interrupted、MaxIterationsExceeded、LLM 调用失败、ToolRejected 等
    self.fire_event(HookEvent::StopFailure, &input, None, None).await;
}
```

## 预期行为

| 错误类型 | Claude Code | Peri |
|---------|------------|------|
| API/LLM 调用失败 | StopFailure | StopFailure ✓ |
| 用户中断（Ctrl+C） | 不触发任何 Stop 事件 | StopFailure ✗ |
| MaxIterationsExceeded | 不触发任何 Stop 事件 | StopFailure ✗ |
| ToolRejected（钩子拒绝） | 不触发任何 Stop 事件 | StopFailure ✗ |

## 修复方向

1. 在 `on_error` 中根据 `AgentError` 变体决定是否触发 StopFailure
2. 仅对 `AgentError::LlmError`/API 相关错误触发 StopFailure
3. 对 `Interrupted`、`MaxIterationsExceeded` 等非 API 错误跳过 StopFailure
4. 参考 `after_agent` 和 `on_error` 的调用路径判断哪些错误类型应触发

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `on_error` 方法（line 531-564）
- `peri-agent/src/error.rs` — `AgentError` 枚举定义
