# 消息列表面板缺少「滚动到底」快捷按钮

**状态**：Open
**优先级**：中
**创建日期**：2026-05-15

## 问题描述

当用户在消息列表中向上滚动查看历史消息后（`scroll_follow = false`），没有任何可视化的 UI 元素提示用户可以快速回到底部。用户必须手动向下滚动或提交新消息才能回到最新内容。应在消息列表右侧添加一个下箭头按钮，当存在未显示的最底部内容时显示，点击后直接滚动到底。

## 症状详情

| 场景 | 当前行为 | 期望行为 |
|------|----------|----------|
| 用户向上滚动查看历史 | 无提示，需手动滚回 | 右侧显示 ↓ 按钮 |
| 用户已在底部（`scroll_follow = true`） | N/A | 不显示按钮 |
| 用户点击 ↓ 按钮 | N/A | 立即滚动到底部 |
| 用户已在底部（`offset >= max_scroll`） | N/A | 不显示按钮 |

## 期望行为

在消息列表渲染区域的右侧（与现有滚动条相邻），当 `offset < max_scroll` 时渲染一个倒三角箭头符号（如 `▼`），鼠标点击或键盘快捷键触发后滚动到底部（设置 `scroll_follow = true` 或直接 `scroll_offset = max_scroll`）。

## 相关代码

- `rust-agent-tui/src/ui/main_ui.rs:476-533` —— 消息列表滚动状态计算（`scroll_offset`、`scroll_follow`、`max_scroll`）
- `rust-agent-tui/src/ui/main_ui.rs:655-678` —— 消息列表渲染（sticky header、Paragraph scroll、Scrollbar）
- `rust-agent-tui/src/app/ui_state.rs:9-10` —— `scroll_offset` 和 `scroll_follow` 字段定义
- `rust-agent-tui/src/event.rs:1048-1078` —— 鼠标滚轮事件处理
- `rust-agent-tui/src/app/mod.rs:597-608` —— `ensure_cursor_visible()` 函数
- `perihelion-widgets/src/scrollable.rs` —— `ScrollState` + `ScrollableArea` Widget

## 设计要点

1. **显示条件**：`max_scroll > 0 && offset < max_scroll`（存在可滚动内容且未在底部）
2. **位置**：消息区域右下角（可与现有滚动条共用右侧空间，或放在滚动条下方/上方）
3. **样式**：使用 `▼` 或其他箭头符号，与现有主题色系统一
4. **交互**：支持鼠标左键点击触发滚动到底
5. **状态更新**：点击后设 `scroll_follow = true`，触发 `request_rebuild()` 刷新视图
