# 代码高亮 领域

## 领域综述

代码高亮领域负责 TUI 中 Markdown 多行代码块的语法高亮渲染，使用 syntect 实现基于语法分析的高精度着色。

核心职责：
- 通过 markdown-highlight feature flag 控制启用
- syntect default-fancy 纯 Rust 实现，避免 C 库编译问题
- 延迟初始化 SyntaxSet/ThemeSet，仅加载一次
- 未识别语言回退到统一颜色

## 核心流程

### 代码高亮流程

```
Markdown 解析遇到代码块（```lang）
  → feature markdown-highlight 启用?
      是 → highlight_code_block(code, lang)
           → SyntaxSet::find_syntax_by_token(lang)
           → 找到 → syntect HighlightLines → ratatui Span 着色
           → 未找到 → 回退 theme.code() 统一颜色
      否 → 直接使用 theme.code() 统一颜色
  → 单行代码块不做语法高亮
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 高亮库 | syntect 5（default-fancy，纯 Rust，避免 oniguruma C 库） |
| Feature flag | markdown-highlight 控制，不影响默认构建 |
| 初始化 | once_cell::sync::Lazy 延迟加载 SyntaxSet/ThemeSet |
| 主题 | base16-ocean.dark（与 TUI 暗色背景协调） |
| 回退 | 未识别语言或无标签 → theme.code() 统一颜色 |
| 单行代码 | 不做语法高亮，保持 theme.code() |

## Feature 附录

### feature_20260429_F001_syntect-codeblock-highlight
**摘要:** 使用 syntect 为 Markdown 多行代码块添加语法高亮
**关键决策:**
- 通过 feature flag markdown-highlight 控制启用，不影响默认构建
- 使用 syntect default-fancy（纯 Rust）避免 C 库 oniguruma 编译问题
- SyntaxSet/ThemeSet 通过 once_cell::sync::Lazy 延迟初始化
- 未识别语言或无语言标签时回退到统一颜色行为
- 单行代码块不做语法高亮，保持 theme.code() 统一颜色
- 默认使用 base16-ocean.dark 主题，与 TUI 暗色背景协调
**归档:** [链接](../../archive/feature_20260429_F001_syntect-codeblock-highlight/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — Markdown 渲染集成点
- → [tui-widgets.md](./tui-widgets.md) — perihelion-widgets MarkdownRenderer 组件
