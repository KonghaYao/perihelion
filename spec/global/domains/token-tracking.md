# Token 追踪与压缩 领域

## 领域综述

Token 追踪与压缩领域负责 Agent 执行过程中 Token 用量的累积追踪、上下文窗口使用率监控，以及在接近上下文限制时自动触发压缩。

核心职责：
- TokenTracker：累积追踪 input/output/cache_read/cache_creation tokens
- ContextBudget：上下文窗口预算管理，默认 200K，85% 自动压缩阈值
- Micro-compact：零 API 调用的轻量压缩，清除可压缩工具结果和图片/文档
- Full Compact：调用 LLM 生成 9 段结构化摘要替换历史消息
- 压缩后重新注入最近读取文件和激活 Skills

## 核心流程

### Token 累积追踪

```
LlmCallEnd 事件携带 usage
  → TokenTracker.accumulate(usage)
  → estimated_context_tokens() 估算当前上下文大小
  → context_usage_percent() 计算使用百分比
  → 超过 70% 警告 → 超过 85% 触发自动压缩
```

### 自动压缩流程

```
context_usage > 85%
  → Micro-compact:
      清除可压缩工具白名单中的工具结果（bash/read/glob/search/write/edit）
      时间衰减清除（超过 5 步的旧结果）
      图片替换为 [image]，文档替换为 [document]
      工具对完整性保护（tool_use + tool_result 不拆开）
  → 仍超限 → Full Compact:
      9 段结构化摘要模板调用 LLM
      移除 <analysis> 块保留 <summary>
      替换历史为摘要 System 消息
  → re_inject:
      提取最近读取文件路径 → 重新注入文件内容
      提取 Skills 路径 → 重新注入 Skill 全文
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Token 追踪 | TokenTracker 放在 AgentState，SubAgent 继承 |
| 上下文估算 | 优先 API usage，不可用时 fallback 粗估 |
| 压缩配置 | CompactConfig 支持环境变量覆盖（DISABLE_COMPACT 等） |
| Micro-compact | 可压缩白名单 + 时间衰减 + 图片/文档替换 |
| Full Compact | 9 段摘要模板 + 工具对保护 + PTL 降级重试 |
| 重新注入 | 提取最近文件 + Skills 路径，System 消息形式注入 |
| 核心层与 TUI 层分离 | 核心层实现纯消息操作，TUI 层仅负责触发和 UI 展示 |

## Feature 附录

### feature_20260427_F004_token-tracking-auto-compact
**摘要:** Token 累积追踪与上下文窗口感知的自动压缩机制
**关键决策:**
- TokenTracker 放在 AgentState 中，便于 SubAgent 继承和持久化
- 上下文估算优先使用 API 返回的 usage，不可用时 fallback 粗估
- Auto-compact 在 TUI 层触发（两阶段：LlmCallEnd 标记 + Done 后执行）
- Micro-compact 作为零 API 调用的轻量前置防线（70%-85% 触发）
- Full compact 在 85% 时调用 LLM 生成摘要并创建新 Thread
**归档:** [链接](../../archive/feature_20260427_F004_token-tracking-auto-compact/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [compact.md](./compact.md) — Micro/Full Compact 策略增强设计
- → [tui.md](./tui.md) — TUI 状态栏 token 用量展示
