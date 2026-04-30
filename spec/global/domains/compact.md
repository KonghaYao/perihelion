# 上下文压缩增强 领域

## 领域综述

上下文压缩增强领域负责 Micro-compact 和 Full Compact 策略的全面增强，包括可压缩工具白名单、9 段结构化摘要模板和压缩后重新注入。

核心职责：
- Micro-compact 可压缩工具白名单 + 时间衰减清除策略
- Full Compact 9 段结构化摘要模板对齐 Claude Code
- 压缩后重新注入最近读取文件和激活 Skills
- 工具对完整性保护确保 tool_use + tool_result 不被拆开
- CompactConfig 通过 settings.json 配置，环境变量可覆盖

## 核心流程

### Micro-compact 流程

```
触发条件: context_usage 70%-85%
  → 白名单工具结果可压缩（bash/read/glob/search/write/edit）
  → 时间衰减: 超过 micro_compact_stale_steps(5) 步的旧结果
  → 图片替换: [image] 或 [compacted: image ~{tokens} tokens]
  → 文档替换: [document] 或 [compacted: document ~{tokens} tokens]
  → 工具对保护: adjust_index_to_preserve_invariants() 确保 tool_use + tool_result 不拆开
```

### Full Compact 流程

```
触发条件: context_usage > 85%
  → 9 段结构化摘要模板:
      Primary Request → Technical Concepts → Files → Errors & Fixes →
      Problem Solving → User Messages → Pending Tasks → Current Work → Next Step
  → 调用 LLM 生成摘要
  → 移除 <analysis> 块，保留 <summary>
  → PTL 降级重试: 按消息步数组逐步删除最旧组，最多重试 3 次
  → re_inject: 提取最近文件路径 + Skills → System 消息重新注入
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Micro-compact | 可压缩白名单 + 时间衰减 + 图片/文档替换 + 工具对保护 |
| Full Compact | 9 段摘要模板 + LLM 调用 + PTL 降级重试 |
| 重新注入 | extract_recent_files() + extract_skills_paths() → System 消息 |
| 配置 | CompactConfig 支持环境变量覆盖 |
| 核心层分离 | 纯消息操作在核心层，TUI 层仅触发和展示 |

## Feature 附录

### feature_20260428_F001_compact-redesign
**摘要:** 全面增强 Micro/Full Compact 策略与压缩后重新注入
**关键决策:**
- Micro-compact 引入可压缩工具白名单 + 时间衰减清除策略
- Full Compact 采用 9 段结构化摘要模板对齐 Claude Code
- 压缩后重新注入最近读取文件和激活 Skills（System 消息形式）
- 工具对完整性保护：确保 tool_use 和 tool_result 不被拆开
- PTL 降级重试：按消息步数组逐步删除最旧组，最多重试 3 次
- CompactConfig 通过 settings.json 配置，环境变量可覆盖
- 核心层实现纯消息操作，TUI 层仅负责触发和 UI 展示
**归档:** [链接](../../archive/feature_20260428_F001_compact-redesign/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [token-tracking.md](./token-tracking.md) — Token 追踪触发压缩
- → [tui.md](./tui.md) — TUI /compact 命令
