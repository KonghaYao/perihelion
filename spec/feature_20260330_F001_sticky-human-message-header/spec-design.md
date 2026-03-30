# Feature: 20260330_F001 - sticky-human-message-header

## 需求背景

当前 TUI 聊天区（`Constraint::Min(3)`）的消息列表可自由滚动。但当对话变长时，用户向上滚动查看历史消息后，会丢失"我最初问的是什么"这个上下文。需要一个始终可见的 sticky header 来锚定最后一条 Human 消息。

## 目标

- 用户滚动查看历史回复时，最新发出的问题始终固定在聊天区顶部可见
- 不干扰正常聊天体验——无 Human 消息时不占空间
- 视觉轻量化，与整体 TUI 风格一致

## 方案设计

![TUI Sticky Header 布局示意图](./images/01-wireframe.png)

### 布局变更

在 `main_ui.rs` 的垂直 Layout 中，将原来单一的聊天区 `[0]` 拆分为两个子区域：

```
┌──────────────────────────────────────┐
│  [sticky header]  ← 新增：1-3 行     │
├──────────────────────────────────────┤
│  [scrollable messages]  ← Min(1)     │
│                                      │
│  （原 Constraint::Min(3) 的聊天区）   │
└──────────────────────────────────────┘
```

Layout constraints 变更：

```rust
// Before
Constraint::Min(3),  // 聊天区

// After
Constraint::Length(sticky_header_height),  // sticky header（动态 0-3 行）
Constraint::Min(1),  // 可滚动消息区
```

`sticky_header_height` 计算规则：

| 条件 | 高度 |
|------|------|
| `last_human_message.is_none()` | `0`（完全隐藏） |
| 消息 ≤ 40 字符 | `1` 行 |
| 消息 41–120 字符 | `2` 行 |
| 消息 > 120 字符 | `3` 行（截断 + `…`） |

### 渲染内容

Header widget 渲染最后一条 Human 消息的纯文本（无需 markdown 解析），格式：

```
> <消息文本>
───────────────
```

- `"> "` 标签：`theme::ACCENT` 色 + `Modifier::BOLD`
- 消息文本：`theme::TEXT` 色，正常字重
- 超过 3 行时截断，末尾追加 `…`
- 底部分隔线：`theme::MUTED` 色纯文本行（`─` repeated）

> 为什么不走 RenderCache？
> Header 内容是纯文本，不含 markdown，无需解析。直接主线程渲染即可，RenderCache 保持只管理下方消息区的行数据。

### 状态管理

在 `app/core.rs` 新增字段：

```rust
last_human_message: Option<String>,
```

**更新时机：**
1. `submit_message()` 发送用户消息时 → 更新为新消息文本
2. `/clear` 清空消息时 → `last_human_message = None`
3. 打开历史 thread 时 → 从 thread 最后一条 Human 消息恢复

### 分支渲染逻辑

```rust
fn render_messages(f: &mut Frame, app: &mut App, header_area: Rect, messages_area: Rect) {
    // Header（始终渲染，但 height=0 时直接跳过）
    render_sticky_header(f, app, header_area);

    // 消息区（Welcome 或消息列表）
    if app.core.view_messages.is_empty() {
        welcome::render_welcome(f, app, messages_area);
    } else {
        render_message_list(f, app, messages_area);
    }
}
```

> 为什么不把 header 放在 `main_ui.rs` 的顶层 Layout 约束里？
> `main_ui.rs` 的 Layout 是将整个屏幕分区。如果 header 放在顶层，滚动时 header 就不动——这正是 sticky 的效果。但如果放在聊天区内（子区域），header 就跟着消息一起滚。两种方式都能实现 sticky，区别在于 header 是否占用聊天区的可见空间。当前方案选的是"占用聊天区空间"（即 header 在聊天区顶部，消息区在 header 下方），这样 header 和消息互不遮挡，header 始终紧贴聊天区顶部。

## 实现要点

1. **header 高度估算**：使用 `chars / terminal_width + 1`，clamp 到 `[1, 3]`；行宽取 `messages_area.width`（消息区不含滚动条）
2. **消息截断**：超过估算行数时，用 `text[..trunc_len]` + `…`，截断位置尽量在空格处（避免截断单词中间）
3. **多模态消息**：仅取 Human 消息的 Text ContentBlock 纯文本，忽略 Image ContentBlock 的 base64 数据
4. **无消息时**：header `height = 0`，Layout 不分配空间，`render_sticky_header` 内部 `if area.height == 0 { return; }` guard
5. **RenderCache 独立性**：header 渲染不走 RenderCache，RenderCache 继续只管消息区行数据，两者互不干扰

## 约束一致性

- **无新增 crate 依赖**：仅修改 `rust-agent-tui` 内部模块
- **不破坏现有 Layout**：Layout 约束变更完全向后兼容（`Min(1)` 保证消息区永不消失）
- **渲染线程零侵入**：header 走主线程渲染，不影响现有 `render_thread.rs` 逻辑
- **持久化无关**：header 显示状态不写入 SQLite，仅运行时内存状态

## 验收标准

- [ ] 用户发送消息后，聊天区顶部立即显示该消息的 sticky header
- [ ] 用户向上滚动消息列表时，header 固定不动（sticky 效果）
- [ ] 连续发送多条消息，header 始终显示**最后一条** Human 消息
- [ ] 消息超过 3 行时截断显示 `…`
- [ ] `/clear` 清空后 header 消失（`height=0`）
- [ ] 打开历史 thread 时，如果 thread 有消息则显示最后一条 Human 消息作为 header
- [ ] 没有任何 Human 消息时（welcome 状态），header 不占空间
- [ ] 终端宽度变化时，header 行数估算重新计算
