# Skills 触发 领域

## 领域综述

Skills 触发领域负责 Skills 的触发机制设计，将触发键从 # 统一到 / 前缀，与命令系统共用命名空间。

核心职责：
- Skills 触发键从 # 改为 /，与命令共用命名空间
- 提示浮层合并展示命令组（前）和 Skills 组（后）
- 命令匹配优先，Skill 名与命令名冲突时命令优先执行
- 消息解析中 /xxx token 直接触发 Skill 预加载

## 核心流程

### Skills 触发流程

```
用户输入 / 前缀
  → 提示浮层显示: 命令组（最多 6 条）+ Skills 组（最多 4 条）
  → Tab 补全: 在合并候选列表中定位
  → Enter 触发:
      先尝试命令 dispatch → 命中则执行命令
      未命中 → 尝试 Skill 匹配 → 匹配则走 Submit 流程
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 触发键 | / 前缀（统一命令和 Skills 命名空间） |
| 提示浮层 | render_unified_hint() 合并两组，命令在前 Skills 在后 |
| 优先级 | 命令 > Skills（同名冲突时命令优先） |
| 消息解析 | /xxx token 直接触发 Skill 预加载，无需排除命令名 |

## Feature 附录

### feature_20260429_F001_skill-slash-trigger
**摘要:** Skills 触发键从 # 统一到 / 前缀
**关键决策:**
- Skills 触发键从 # 改为 /，与命令共用命名空间
- 提示浮层合并展示命令组（前）和 Skills 组（后）
- Enter 触发时命令匹配优先，未命中再尝试 Skill 匹配
- Skill 名与命令名冲突时命令优先执行
- 消息解析中 /xxx token 直接触发 Skill 预加载，无需排除命令名
**归档:** [链接](../../archive/feature_20260429_F001_skill-slash-trigger/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 提示浮层渲染
- → [agent.md](./agent.md) — SkillsMiddleware 和 SkillPreloadMiddleware
