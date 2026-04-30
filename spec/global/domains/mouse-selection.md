# 鼠标选区 领域

## 领域综述

鼠标选区领域负责 TUI 中的鼠标拖拽文字选择和系统剪贴板复制功能，实现字符级精度选区，正确处理 CJK 宽字符。

核心职责：
- TextSelection 模块管理拖拽状态和选区坐标
- WrappedLineInfo 换行映射将屏幕坐标映射到逻辑字符位置
- unicode-width 正确处理 CJK 宽字符选区计算
- Ctrl+C 优先级链：选区复制 > 中断 > 退出

## 核心流程

### 鼠标选区流程

```
鼠标左键按下（消息区）
  → TextSelection.start_drag(row, col)
  → 鼠标拖动
  → TextSelection.update_drag(row, col)
  → 鼠标释放
  → extract_selected_text(wrap_map, selection)
  → arboard::Clipboard 写入系统剪贴板
  → TextSelection.clear()
```

### Ctrl+C 优先级链

```
Ctrl+C 按下
  → 消息区有选区？→ 复制到剪贴板 → clear()
  → 面板有选区？→ 复制到剪贴板 → clear()
  → textarea 有选区？→ textarea.copy()
  → 无选区 → Loading? 中断 : 退出
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 选区数据结构 | TextSelection { start, end, dragging, selected_text } |
| 面板选区 | PanelTextSelection，Vec<String> 纯文本行直接索引 |
| 换行映射 | WrappedLineInfo { line_idx, visual_row_start/end, plain_text, char_widths } |
| 坐标映射 | visual_to_logical()：视觉坐标 → 逻辑行+字符偏移 |
| 宽字符 | unicode-width crate，char_widths 累积宽度定位字符 |
| 高亮渲染 | highlight_line_spans()：Span 在字符边界拆分 + Modifier::REVERSED |
| 剪贴板 | arboard::Clipboard 跨平台写入 |

## Feature 附录

### feature_20260429_F002_mouse-text-selection
**摘要:** TUI 鼠标拖拽选中文本并复制到系统剪贴板
**关键决策:**
- 新增 TextSelection 模块管理拖拽状态，存储在 AppCore 中
- 通过 WrappedLineInfo 换行映射表将屏幕坐标映射到逻辑字符位置
- 使用 unicode-width crate 正确处理 CJK 宽字符的选区计算
- Ctrl+C 优先级链：有选区时复制 > Loading 时中断 > 非Loading时退出
- 选区高亮使用 Modifier::REVERSED 反色，只深拷贝涉及的少量行
- 渲染时通过 AppCore.messages_area 存储消息区域 Rect 供事件处理使用
**归档:** [链接](../../archive/feature_20260429_F002_mouse-text-selection/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 事件处理和渲染
