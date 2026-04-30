# LLM 重试 领域

## 领域综述

LLM 重试领域负责处理 LLM API 调用中的暂时性错误，通过装饰器模式实现透明重试，对 ReAct 执行器零改动。

核心职责：
- 精确区分可重试错误（429/5xx/网络超时）和不可重试错误（4xx 客户端）
- 指数退避 + 25% 随机抖动策略
- 通过事件通知 TUI 显示重试状态

## 核心流程

### 重试流程

```
LLM 调用失败
  → 错误分类:
      429/5xx/网络错误 → 可重试
      4xx 客户端错误 → 不可重试
  → 可重试:
      emit(LlmRetrying{attempt, max, delay, error})
      → sleep(exponential_delay)
      → 重试（最多 5 次）
  → 不可重试:
      直接返回错误
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 装饰器模式 | RetryableLLM<L> 包装任意 ReactLLM，对 executor 零改动 |
| 退避策略 | base_delay * 2^(attempt+1) + 25% jitter，最大 32s |
| 错误分类 | LlmHttpError(status_code) + LlmError(network)，is_retryable() 判断 |
| 事件通知 | AgentEvent::LlmRetrying 携带 attempt/max/delay/error |
| 配置 | max_retries=5, base_delay_ms=500, max_delay_ms=32000 |

## Feature 附录

### feature_20260428_F001_llm-retry
**摘要:** LLM 暂时性错误自动重试（指数退避+抖动）
**关键决策:**
- 装饰器模式 RetryableLLM<L> 包装任意 ReactLLM，对 executor 零改动
- 精确区分可重试错误（429/5xx/网络超时）和不可重试错误（4xx 客户端）
- 新增 LlmHttpError 变体携带 HTTP status code，保留 LlmError 用于网络错误
- 指数退避 + 25% 随机抖动，最大延迟 32s 封顶
- 通过 AgentEvent::LlmRetrying 事件通知 TUI 显示重试状态
**归档:** [链接](../../archive/feature_20260428_F001_llm-retry/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [agent.md](./agent.md) — ReAct 执行器，重试装饰器包装 ReactLLM
