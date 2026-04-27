# Feature: 20260408_F001 - askuser-dialog-height

## 需求背景

TUI 端 AskUser 弹窗在内容较多时显示不全：长问题文本、多问题堆叠、选项描述过长时，弹窗高度计算不准确，导致内容被截断。虽然已有 `scroll_offset` 滚动机制，但可见高度硬编码为 10，且高度估算不考虑文本换行，实际体验较差。

## 目标

- 弹窗高度准确反映内容实际行数（含文本换行）
- 滚动机制使用真实可见区域高度，光标始终可见
- 切换 Tab 时滚动位置正确重置

## 方案设计

### 1. 修复高度计算（`active_panel_height`）

**文件**：`rust-agent-tui/src/ui/main_ui.rs`，`active_panel_height` 函数，AskUser 分支（约第 130-139 行）

**当前问题**：
```rust
let opt_rows = cur.data.options.len() as u16;
let desc_rows = cur.data.options.iter()
    .filter(|o| o.description.is_some()).count() as u16;
(cur.data.question.lines().count() as u16 + opt_rows + desc_rows + 7).max(8)
```
- `question.lines().count()` 只算换行符分隔的行数，不考虑宽度换行
- 每个 option/description 只算 1 行，不考虑长文本换行

**修复方案**：

引入 `wrapped_line_count(text: &str, width: u16) -> u16` 辅助函数，根据弹窗内宽度计算文本实际占用的视觉行数。弹窗内宽度 = `panel_width - 2`（border）。

```rust
// 辅助函数：计算文本在指定宽度下实际占用的行数
fn wrapped_line_count(text: &str, width: u16) -> u16 {
    if width == 0 { return text.lines().count() as u16; }
    let w = width as usize;
    text.lines().map(|line| {
        let chars = line.chars().count();
        if chars == 0 { 1 } else { ((chars + w - 1) / w).max(1) as u16 }
    }).sum::<u16>().max(1)
}
```

> 注意：CJK 字符占 2 列宽，生产环境应使用 `unicode-width` crate 的 `UnicodeWidthStr::width()` 替代 `chars().count()`。

**新的高度计算**：
```rust
// AskUser 分支
let inner_w = /* panel_width - 2 border */;
let q_lines = wrapped_line_count(&cur.data.question, inner_w);
let opt_lines: u16 = cur.data.options.iter().map(|o| {
    let label_lines = wrapped_line_count(&o.label, inner_w.saturating_sub(6));
    let desc_lines = o.description.as_ref()
        .map(|d| wrapped_line_count(d, inner_w.saturating_sub(6)))
        .unwrap_or(0);
    label_lines + desc_lines
}).sum();
(q_lines + opt_lines + 7).max(8)  // +7: select_hint(1) + blank(1) + input(1) + input_label(1) + border(2) + separator(1)
```

### 2. 修复滚动可见高度硬编码

**文件**：`rust-agent-tui/src/app/ask_user_ops.rs`，`ask_user_move` 函数（第 20-21 行）

**当前问题**：
```rust
p.scroll_offset = ensure_cursor_visible(cursor_row, p.scroll_offset, 10);
```
可见高度硬编码为 10，不随实际 `content_area` 变化。

**修复方案**：

在 `AskUserBatchPrompt` 中增加 `visible_height: u16` 字段，由渲染函数在每帧更新：

1. `ask_user_prompt.rs`：`AskUserBatchPrompt` 新增 `pub visible_height: u16` 字段，初始值 0
2. `popups/ask_user.rs`：渲染时将 `content_area.height` 写入 `prompt.visible_height`
3. `ask_user_ops.rs`：使用 `prompt.visible_height` 替代硬编码 10

```rust
// ask_user_ops.rs
let visible = p.visible_height.max(1);
p.scroll_offset = ensure_cursor_visible(cursor_row, p.scroll_offset, visible);
```

### 3. Tab 切换时重置滚动

**文件**：`rust-agent-tui/src/app/ask_user_prompt.rs`，`next_tab()` 和 `prev_tab()` 方法

**修复**：
```rust
pub fn next_tab(&mut self) {
    if !self.questions.is_empty() {
        self.active_tab = (self.active_tab + 1) % self.questions.len();
        self.scroll_offset = 0;
    }
}

pub fn prev_tab(&mut self) {
    if !self.questions.is_empty() {
        self.active_tab = self.active_tab
            .checked_sub(1)
            .unwrap_or(self.questions.len() - 1);
        self.scroll_offset = 0;
    }
}
```

## 实现要点

- `wrapped_line_count` 应使用 `unicode-width` crate 处理 CJK 字符宽度（项目已有此依赖）
- `active_panel_height` 目前无法直接获取 panel_width（它只接收 `screen_height`），需要扩展函数签名或从 layout 中获取宽度信息
- 渲染函数回写 `visible_height` 是单帧延迟（先计算高度 → 布局 → 渲染 → 写回），首次显示时 `visible_height=0`，应 fallback 到合理默认值

## 约束一致性

本方案不涉及架构变更，仅修复 TUI 渲染逻辑，与 `spec/global/constraints.md` 和 `spec/global/architecture.md` 一致。

## 验收标准

- [ ] 问题文本超宽时正确换行计算，弹窗高度匹配实际内容
- [ ] 选项 description 超宽时正确换行计算
- [ ] 4 个问题 + 长描述堆叠时，弹窗不超过 60% 屏高但内容可滚动
- [ ] 上下移动光标时滚动自动跟随，光标始终可见
- [ ] Tab 切换后滚动位置从顶部开始
- [ ] 短问题（不超宽）行为不变，回归正常
