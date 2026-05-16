# active_provider 越界无保护可导致 render panic

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 问题描述

`render_form_edit` 中 `&wizard.providers[wizard.active_provider]` 是裸数组索引，无越界检查。正常流程中 active_provider 始终在 bounds 内，但异常状态（反序列化损坏、手动错误操作导致 providers 收缩但 active_provider 未更新）会直接 crash。

## 症状详情

| 现象 | 详情 |
|------|------|
| 潜在 panic | `providers` 为空或 `active_provider >= providers.len()` 时直接 panic |
| 渲染函数无容错 | 没有 `providers.get()` 回退逻辑 |

## 根因

`peri-tui/src/ui/main_ui/popups/setup_wizard.rs:264`

```rust
let mp = &wizard.providers[wizard.active_provider];  // 裸索引
```

## 期望

使用 `wizard.providers.get(wizard.active_provider)` 并处理 `None` 情况——回退到 Browse 模式、显示错误提示或跳过渲染。

## 涉及文件

- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— `render_form_edit()` (line 264)
- `peri-tui/src/app/setup_wizard.rs` —— 所有修改 `active_provider` 的地方
