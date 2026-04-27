# Feature: 20260329_F003 - ui-display-fixes

## 需求背景

TUI 存在 3 个 UI 显示问题：

1. **SubAgentGroup 内部消息序号多余**：`render_view_model` 对 SubAgentGroup 内嵌消息传入 `index=0`，导致标题行显示 `0`、工具调用显示 `01 bash` 等无意义序号。
2. **AI 消息中 ToolUse 显示多余**：`AssistantBubble` 的 `ContentBlockView::ToolUse` 被渲染为 `🔧 name` 行（`message_render.rs:87-106`），但工具调用结果会由后续 `ToolBlock` 单独显示，AI 消息中的 ToolUse 行多余。
3. **HITL/AskUser 弹窗高度不足**：`active_panel_height` 限制弹窗最多占 40% 屏高（`screen_height * 2 / 5`），当问题内容较多时显示不全，且 AskUser 弹窗虽有 `scroll_offset` 但可见区域太小。

## 目标

- SubAgentGroup 内部消息不显示序号前缀
- AI 消息不再渲染 ToolUse 行（仅保留文本和 Reasoning）
- 弹窗高度上限从 40% 提高到 60%，内容多时可滚动查看

## 方案设计

### 修改 1：SubAgentGroup 内部消息去序号

**文件：** `rust-agent-tui/src/ui/message_render.rs`

**当前行为：** SubAgentGroup 嵌套消息调用 `render_view_model(inner_vm, 0, _width)`，`index=0` 导致各分支渲染时带有 `0` 前缀。

**修改方案：** 将 `render_view_model` 的 `index: usize` 改为 `index: Option<usize>`。

| 分支 | `Some(n)` 行为 | `None` 行为（SubAgent 内部） |
|------|---------------|---------------------------|
| `UserBubble` | `n ` + 内容 | 直接显示内容 |
| `AssistantBubble` | `n …` 标题 + 内容 | 标题只显示文本 |
| `ToolBlock` | `n name ▸` | `▸ name` |
| `SystemNote` | `ℹ ` + 内容 | `ℹ ` + 内容（无变化） |
| `SubAgentGroup` | `n 🤖 agent_id` | `🤖 agent_id` |

SubAgentGroup 内部调用改为 `render_view_model(inner_vm, None, _width)`。
外层调用保持 `render_view_model(vm, Some(i), width)`。

### 修改 2：AI 消息去除 ToolUse 渲染

**文件：** `rust-agent-tui/src/ui/message_render.rs`

**当前行为：** `AssistantBubble` 渲染时，`ContentBlockView::ToolUse` 显示为 `🔧 name` 行（带 `tool_idx` 编号）。

**修改方案：** 在 `AssistantBubble` 渲染循环中跳过 `ContentBlockView::ToolUse`，不输出任何行。工具调用信息由后续 `ToolBlock` 独立展示。

```rust
ContentBlockView::ToolUse { .. } => {
    // 跳过：工具调用由 ToolBlock 独立显示
    continue;
}
```

注意：仍需处理 `first_text_merged` 逻辑——如果第一个 block 是 ToolUse，需要确保标题行能正确创建。

### 修改 3：弹窗高度上限提升

**文件：** `rust-agent-tui/src/ui/main_ui.rs` 的 `active_panel_height` 函数

**当前值：**
```rust
let max_h = screen_height * 2 / 5; // 最多占 40% 屏高
```

**修改方案：** 将上限从 40% 提高到 60%：
```rust
let max_h = screen_height * 3 / 5; // 最多占 60% 屏高
```

这样当 HITL 审批项多或 AskUser 问题长时，弹窗有更多空间显示内容。AskUser 弹窗已有 `scroll_offset` 滚动支持（`ask_user.rs:131`），增大可见区域后滚动体验更好。

## 实现要点

- 修改 1 涉及 `message_render.rs` 函数签名变更，需同步更新所有调用点
- 修改 2 需处理 edge case：AssistantBubble 只含 ToolUse blocks 时，应创建空标题行而非无输出
- 修改 3 仅改一个常量，无风险
- 现有 headless 测试需更新匹配预期

## 约束一致性

本方案完全符合现有架构约束：
- TUI 仍使用 ratatui 渲染，仅调整渲染参数
- 不改变消息格式或数据流
- 不引入新依赖

## 验收标准

- [ ] SubAgentGroup 内部消息无序号前缀（如 `0`、`01`）
- [ ] AI 消息不再显示 `🔧 name` 的 ToolUse 行
- [ ] HITL 审批弹窗高度上限提升到 60%
- [ ] AskUser 问答弹窗可见区域增大，长问题可滚动查看
- [ ] 现有 headless 测试通过（含 subagent_group 相关测试更新）
