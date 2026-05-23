# 思考内容只显示最后一行导致自动换行布局抖动

**状态**：Verify
**优先级**：中
**创建日期**：2026-05-23
**修复日期**：2026-05-23

## 问题描述

流式推理（thinking/reasoning）在 TUI 中只显示思考文本的最后一行（tail_lines=1），当该行内容逐渐增长超出终端宽度时，自动换行导致渲染高度在 1 行和 2 行之间来回变化，产生明显的上下抖动。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发场景 | 使用支持 thinking 的模型（如 Anthropic extended thinking）进行推理时 |
| 可见现象 | 思考文本流式显示，但只展示最后 1 行；该行随内容增长反复超出终端宽度自动换行，布局高度不断在 1↔2 行之间跳变 |
| 抖动频率 | 与流式 chunk 到达频率一致（100ms 节流） |
| 期望行为 | 思考内容应显示多行（如最后 3-5 行），或采用固定高度区域，避免单行内容增长导致的换行抖动 |

## 根因路径

`extract_tail_lines(text, 1)` 只取最后 1 行 → 渲染为单行 `⎿` 文本 → 内容增长超出终端宽度 → ratatui 自动换行 → 高度从 1 变为 2 → 下一个 chunk 可能又变回 1 → 循环抖动。

## 涉及文件

- `peri-tui/src/app/message_pipeline/reconcile.rs` —— `extract_tail_lines(text, 1)` 控制显示行数
- `peri-tui/src/ui/message_render.rs:216-235` —— Reasoning block 渲染：header + tail_lines
- `peri-tui/src/app/message_pipeline/transform.rs:18-23` —— 流式 bubble 构建，`tail_lines: None`
