# Feature: 20260429_F002 - mouse-text-selection

## 需求背景

当前 TUI 启用了 `EnableMouseCapture`（`main.rs:80`），终端将所有鼠标事件发送给应用而非终端自身的选择处理器。`event.rs:449-453` 仅处理 `ScrollUp`/`ScrollDown`，鼠标拖拽事件被丢弃，导致用户无法通过鼠标选中文本进行复制。

用户需要在所有区域（消息列表、输入框等）都能通过鼠标拖拽选中文本并复制到剪贴板，同时保留现有的鼠标滚轮滚动功能。

## 目标

- 鼠标拖拽选中文本，视觉高亮反馈
- Ctrl+C 复制选中内容到系统剪贴板
- 保留鼠标滚轮滚动消息列表的功能
- 兼容 CJK 等宽字符的选区计算

## 方案设计

### 数据结构

新增 `TextSelection` 模块（`rust-agent-tui/src/app/text_selection.rs`）：

```rust
/// 文本选区状态
pub struct TextSelection {
    /// 选区起始视觉坐标（相对于消息区域左上角）
    pub start: Option<(u16, u16)>,  // (visual_row, visual_col)
    /// 选区结束视觉坐标
    pub end: Option<(u16, u16)>,
    /// 是否正在拖拽中
    pub dragging: bool,
    /// 选区对应的纯文本内容（松开鼠标后计算）
    pub selected_text: Option<String>,
}
```

存储位置：在 `AppCore` 中新增 `pub text_selection: TextSelection` 字段。

坐标体系：采用视觉行坐标（visual_row, visual_col），相对于消息渲染区域左上角。

### 坐标映射与换行计算

**问题**：鼠标坐标是屏幕像素坐标（字符格），需要映射到 `all_lines` 中的具体字符位置。由于 `Paragraph` widget 的 `Wrap` 换行，一个逻辑行可能对应多个视觉行。

**换行映射表（WrappedLineInfo）**

在 `RenderCache` 中新增字段：

```rust
pub struct WrappedLineInfo {
    /// 该行在 all_lines 中的索引
    pub line_idx: usize,
    /// 该逻辑行渲染后的起始视觉行号
    pub visual_row_start: u16,
    /// 该逻辑行渲染后的结束视觉行号（不含）
    pub visual_row_end: u16,
    /// 该逻辑行的纯文本内容（去样式，用于复制）
    pub plain_text: String,
    /// 每个字符的显示宽度序列（用于列号→字符偏移映射）
    pub char_widths: Vec<u8>,
}

// RenderCache 新增
pub wrap_map: Vec<WrappedLineInfo>,
```

**计算时机**：渲染线程每次更新缓存时（`rebuild_all`、`AddMessage`、`AppendChunk` 等），同步计算 `wrap_map`。

**计算方法**：

1. 对每个 `Line`，遍历其 `Span` 提取纯文本内容和每个字符的显示宽度（使用 `unicode-width` crate，ASCII=1, CJK=2）
2. 根据 `text_area.width` 模拟 word-wrap，计算该逻辑行占几个视觉行
3. 累计视觉行号，记录 `visual_row_start` 和 `visual_row_end`

**映射流程**：

```
鼠标 (screen_x, screen_y)
  → visual_row = screen_y - text_area.y + scroll_offset
  → visual_col = screen_x - text_area.x
  → 二分查找 wrap_map，找到 visual_row 落在哪个 line_idx
  → 用 char_widths 数组，从行首累积宽度到 visual_col，得到字符偏移 char_offset
```

### 鼠标事件处理

在 `event.rs` 的 `Event::Mouse` 分支中扩展：

```rust
Event::Mouse(mouse) => {
    match mouse.kind {
        MouseEventKind::ScrollUp => app.scroll_up(),
        MouseEventKind::ScrollDown => app.scroll_down(),
        MouseEventKind::Down(MouseButton::Left) => {
            if in_message_area(app, mouse.column, mouse.row) {
                app.core.text_selection.start_drag(...);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.core.text_selection.dragging {
                app.core.text_selection.update_drag(...);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.core.text_selection.dragging {
                app.core.text_selection.end_drag(...);
            }
        }
        _ => {}
    }
}
```

**区域判定** `in_message_area()`：需要在事件处理时知道消息区域的 `Rect`。

方案：在 `AppCore` 中新增 `pub messages_area: Option<Rect>`，每次 `render()` 时更新。事件处理时直接读取。

### Ctrl+C 冲突处理

当前 Ctrl+C 的行为：
- Loading 时：中断 Agent
- 非 Loading 时：退出应用

新增优先级：

```
Ctrl+C:
  1. 有文本选区 → 复制到剪贴板，清除选区，返回 Redraw
  2. Loading → 中断 Agent
  3. 非 Loading → 退出
```

剪贴板复制使用已有的 `arboard` crate。

复制成功后，通过 `mode_highlight_until` 类似机制在状态栏短暂显示 "已复制 N 个字符" 提示。

### 选区渲染与高亮

在 `render_messages()` 中，将 `all_lines` 传给 `Paragraph` 之前，对选中范围内的行应用反色高亮。

**算法**：

1. 检测 `text_selection.dragging || text_selection.selected_text.is_some()`
2. 通过 `wrap_map` 确定涉及的逻辑行范围和每行的选中列区间
3. 对涉及的 `Line` 进行深拷贝，修改对应 span 的 `Style`，添加 `Modifier::REVERSED`
4. 使用修改后的 `all_lines` 进行渲染

**高亮样式**：`Style::default().add_modifier(Modifier::REVERSED)`，终端通用的"选中"视觉。

**性能**：只深拷贝选区涉及的少量行，其余行零拷贝传入。消息量通常 < 1000 行，开销可忽略。

### 文本提取

鼠标松开时，根据选区坐标从 `all_lines` 提取纯文本：

```
1. 通过 wrap_map 将 start/end 视觉坐标映射为 (line_idx, char_offset)
2. 如果同一行：selected_text = line.plain_text[char_start..char_end]
3. 如果跨行：拼接 line_start[char_start..] + "\n" + 中间行全文 + "\n" + line_end[..char_end]
```

### 选区清除时机

- Ctrl+C 复制后立即清除
- 鼠标点击消息区域（非拖拽）时清除旧选区并开始新拖拽
- 滚轮滚动时不影响当前选区（选区随内容滚动）

## 实现要点

1. **WrapMap 计算**：需要在渲染线程中与 `all_lines` 同步计算，确保映射与渲染一致。计算逻辑需使用 `unicode-width` crate（项目已有依赖）正确处理 CJK 宽字符。
2. **消息区域 Rect**：Layout split 在 `render()` 中执行，需将结果存入 `AppCore.messages_area`。由于 `render()` 接收 `&mut App`，可直接修改。
3. **Line 深拷贝**：`Line<'static>` 的 `Clone` 实现是深拷贝 spans，这对少量行是安全的。需确保修改的是拷贝而非原始缓存。
4. **Scroll offset**：映射时需加上 `scroll_offset`，且选区坐标在滚动后仍然正确（因为选区基于视觉行号，滚动后视觉行号对应的内容不变）。
5. **第一版范围**：不做拖拽中自动滚动（鼠标拖到边缘时自动 scroll），作为后续增强。

## 约束一致性

- 符合 CLAUDE.md 中"字符串显示宽度"开发注意事项（使用 `unicode-width` crate 计算列宽）。
- Ctrl+C 行为扩展不与现有快捷键冲突，优先级明确。
- 新增的 `TextSelection` 状态管理符合现有 AppCore 的组织模式。

## 验收标准

- [ ] 鼠标在消息区域拖拽时，选中文字有反色高亮
- [ ] 松开鼠标后高亮保持
- [ ] Ctrl+C 将选中文字复制到系统剪贴板，复制后高亮消失
- [ ] 状态栏短暂显示复制成功提示
- [ ] 鼠标滚轮滚动不受影响
- [ ] 无选区时 Ctrl+C 保持原有中断/退出行为
- [ ] CJK 字符选区计算正确（宽字符占 2 列）
- [ ] 跨行选区文本提取正确（包含换行符）
- [ ] 窗口 resize 后选区正确清除
