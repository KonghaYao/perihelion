# TUI 组件 领域

## 领域综述

TUI 组件领域负责通用 UI 组件的抽取和独立 widget crate 的创建，以及对标 Claude Code 的信息展示组件。

核心职责：
- perihelion-widgets crate 提供 11 个通用组件，零内部依赖
- SpinnerWidget：动词从 TODO activeForm 获取，Token 计数平滑递增动画
- ToolCallWidget：工具调用状态指示器，智能折叠策略
- MessageBlockWidget：消息块渲染，代码高亮和 diff 着色
- 所有组件遵循 ratatui StatefulWidget trait 原生 API 风格

## 核心流程

### Spinner 动画流程

```
TUI tick 事件
  → SpinnerState.frame_idx 递增
  → SpinnerWidget::render():
      动词 = active_verb || 随机选取默认动词池
      帧字符 = SPINNER_FRAMES[frame_idx % len]
      Token 平滑递增 = smooth_increment(displayed, target)
      已用时间 = format_elapsed(start_time)
```

### 智能折叠策略

```
工具调用渲染
  → 只读工具（read/glob/search）: 默认折叠
  → 写操作（bash/write/edit/folder）: 默认展开
  → SubAgent 步数 > 4: 自动折叠内部消息
  → Enter 键切换折叠/展开
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 独立 crate | perihelion-widgets，零内部依赖，仅依赖 ratatui + pulldown-cmark |
| 组件数量 | 11 个：BorderedPanel/ScrollableArea/SelectableList/InputField/TabBar/RadioGroup/CheckboxGroup/FormState/MarkdownRenderer/Spinner/ToolCall |
| API 风格 | ratatui StatefulWidget trait 原生风格 |
| 泛型设计 | ListState<T> 不要求 T: Clone；FormState<F> 泛型管理字段导航 |
| 主题抽象 | Theme trait 查询颜色，组件不硬编码 |
| 代码高亮 | syntect default-fancy，feature-gated |
| diff 着色 | highlight_diff_line 实现添加/删除/hunk 行颜色区分 |

## Feature 附录

### feature_20260427_F001_claude-code-info-display
**摘要:** 对标 Claude Code 新增 Spinner、工具调用、消息块三个 widget
**关键决策:**
- Spinner 动词从 TODO activeForm 获取，无任务时随机选取默认动词池
- 动画帧通过 ratatui tick 事件驱动，不引入额外线程
- Token 计数使用平滑递增动画（维护 displayed_tokens 与 token_count 双值）
- 代码高亮采用 syntect 而非 tree-sitter
- 只读工具默认折叠、写操作默认展开的智能折叠策略
- SubAgent 步数超过 4 时自动折叠内部消息
**归档:** [链接](../../archive/feature_20260427_F001_claude-code-info-display/)
**归档日期:** 2026-04-30

### feature_20260427_F001_ratatui-widget-lib
**摘要:** 抽取 TUI 重复 UI 代码为独立可复用 ratatui widget crate
**关键决策:**
- 新增 perihelion-widgets crate，零内部依赖仅依赖 ratatui + pulldown-cmark
- 全量抽取 11 个通用组件（BorderedPanel、ScrollableArea、SelectableList 等）
- 所有组件遵循 ratatui StatefulWidget trait 原生 API 风格
- ListState<T> 泛型设计不要求 T: Clone
- MarkdownRenderer 从 TUI 层迁移，仅负责 &str → Text 转换
- Theme trait 抽象颜色查询接口，组件不硬编码颜色
- FormState<F> 泛型管理字段间导航
**归档:** [链接](../../archive/feature_20260427_F001_ratatui-widget-lib/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 集成使用 perihelion-widgets 组件
- → [code-highlight.md](./code-highlight.md) — syntect 代码高亮集成
