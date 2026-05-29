# 修复：所有错误路径的 Agent 失忆问题

## 问题摘要

`prompt.rs` 的 history 保留逻辑只保护 `Cancelled` 且有进展的场景。所有其他 `result.ok == false` 的错误路径（LLM 错误、工具错误、中间件错误、MaxIterationsExceeded）都走入 `else` 分支无条件 truncate，导致 agent 丢失当前轮次的所有工作成果。

## 根因分析

### 错误传播路径（以 LLM 流式错误为例）

```
1. LLM 流式错误（stream.rs:108）
   → AgentError::LlmError("流式读取失败: ...")

2. RetryableLLM 不重试（retry.rs:109-126）
   → "流式读取失败" 不在 is_retryable() 模式中，直接传播

3. call_llm 返回 Err（llm_step.rs:60）
   → emit LlmCallEnd(ERROR) + run_on_error + return Err(e)

4. ReAct 循环 ? 传播（executor/mod.rs:287）
   → 跳过 cleanup_prepended，executor 返回 Err

5. executor 包装 PromptResult（executor.rs:501）
   → ok = false, stop_reason = EndTurn, messages = agent_state.into_messages()

6. ACP server 判断（prompt.rs:206-211）← BUG 所在
   → else 分支：state.history.truncate(history_len)
```

### 关键代码（prompt.rs:166-212）

```rust
if result.ok {
    // 成功：保留完整 history
    state.history = result.messages;
} else if result.stop_reason == PromptStopReason::Cancelled
    && result.messages.len() > history_len + 1
{
    // Cancel + 有进展：保留 history（Ctrl+C amnesia 修复）
    state.history = cleaned;
} else {
    // ← 所有非 Cancelled 错误走到这里，无条件 truncate
    state.history.truncate(history_len);
}
```

### 所有受影响的错误场景

基于 ReAct 循环结构（`executor/mod.rs:279-323`）和 `stop_reason` 判定（`executor.rs:511-518`）：

| # | 错误场景 | stop_reason | 有进展？ | messages 增量 | 现状 | 应做 |
|---|---------|-------------|---------|--------------|------|------|
| 1 | **LLM 流式错误**（`error decoding response body`） | EndTurn | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 2 | **LLM HTTP 错误**（4xx/5xx 非可重试） | EndTurn | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 3 | **LLM 重试耗尽** | EndTurn | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 4 | **工具执行 deferred_error**（after_tool 错误，state 已写入） | EndTurn | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 5 | **中间件 before_model 错误**（step > 0 时） | EndTurn | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 6 | **中间件 after_model 错误**（step > 0 时） | EndTurn | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 7 | **MaxIterationsExceeded**（循环耗尽） | MaxTurnRequests | 有 | Human + N×(Ai+Tool) | truncate 丢失 | 保留 |
| 8 | **LLM 第一步就失败**（step 0，无工具调用） | EndTurn | 无 | 仅 Human | truncate 正确 | 回滚 |
| 9 | **before_agent 失败**（循环前） | EndTurn | 无 | 仅 Human | truncate 正确 | 回滚 |
| 10 | **Ctrl+C 无进展**（立即中断） | Cancelled | 无 | 仅 Human | truncate 正确 | 回滚 |
| 11 | **Ctrl+C 有进展**（已在工具执行后中断） | Cancelled | 有 | Human + N×(Ai+Tool) | 已修复保留 | ✅ |

**关键发现**：场景 7（MaxIterationsExceeded）是一个严重的额外 bug——agent 执行了 N 轮工具调用后达到最大迭代，所有工作成果被 truncate 丢弃。这与用户报告的 LLM 错误失忆是同一根因。

### 不受影响的路径

- **stdio 路径**：Grep 确认 `history.truncate(history_len)` 仅在 `prompt.rs` 出现一次，stdio 无此逻辑
- **`-p` 模式**（`cli_print.rs`）：不维持 session，不涉及 history truncate

## 修复方案

### 核心改动

将 `else if` 条件从 `Cancelled + has_progress` 改为通用的 `has_progress`：

```rust
// 之前：
} else if result.stop_reason == executor::PromptStopReason::Cancelled
    && result.messages.len() > history_len + 1
{

// 之后：
} else if result.messages.len() > history_len + 1 {
```

### 为什么 `> history_len + 1` 是正确的分界线

- `history_len + 1` = 原始历史 + Human 消息（execute() 入口总是先 add_message Human）
- `> history_len + 1` 意味着还有额外的 Ai/Tool 消息 = agent 产出了有价值的成果
- `== history_len + 1` 意味着只有 Human 消息 = 无进展，安全回滚
- `< history_len + 1` 不可能发生（Human 总是先加入）

### strip_leaked_prepends 安全性

所有 `result.ok == false` 的路径都跳过了 `cleanup_prepended`（? 传播），`result.messages` 头部可能有 leaked system prepends。`strip_leaked_prepends` 通过原始 history 首条消息 ID 定位来剥离，对 MaxTurnRequests/EndTurn 同样有效。

### 改动范围

| 文件 | 改动 |
|------|------|
| `peri-tui/src/acp_server/prompt.rs` | 移除 `result.stop_reason == Cancelled` 条件，仅保留 `messages.len() > history_len + 1` |
| `spec/issues/2026-05-29-llm-stream-error-causes-amnesia.md` | 更新状态为 Fixed，补充 MaxIterationsExceeded 场景 |

### 风险评估

| 维度 | 评估 |
|------|------|
| 改动量 | 1 行（移除条件子句） |
| 回归风险 | 极低——只是扩展了已有保护逻辑的触发条件，不改变保留/回滚的语义 |
| 无进展场景 | 不受影响——`== history_len + 1` 仍走 else 分支正确回滚 |
| 上下文长度 | 保留更多消息可能导致上下文略长，但这比失忆好得多 |
| strip_leaked_prepends | 已有清理机制，对所有 stop_reason 均有效 |

### 测试计划

1. 模拟 LLM 流式错误：mock LLM adapter 在流中途返回错误，验证 history 保留
2. 模拟 MaxIterationsExceeded：设置 max_iterations=2，执行 2 轮工具调用后验证 history 保留
3. 模拟第一步 LLM 错误（无进展）：验证 history 正确回滚
4. 回归测试：Ctrl+C 中断仍正确保留/回滚
5. 回归测试：正常完成路径不受影响
6. 验证下一轮上下文：新 prompt 后 agent 能引用之前执行的操作
