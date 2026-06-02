# 移除 /split 多 session 分屏功能

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-01

## 问题描述

`/split` 命令创建多个 ChatSession 并在 TUI 中以水平多列布局同时显示。tmux 等终端复用工具已经提供了同等的窗口分割能力，项目内部维护多 session 分屏增加了大量代码复杂度但用户实际使用价值低。应完全移除多 session 并发分屏功能，简化 TUI 架构。

## 症状详情

当前多 session 分屏功能的代码痕迹遍布 TUI 层（80+ 个文件、~900 处 `session_mgr` 引用）：

- **SessionManager**：`Vec<ChatSession>` + `active: usize` + `session_areas: Vec<Rect>` 多 session 管理
- **多列布局**：`main_ui/mod.rs` 中 48 处引用 `session_areas`/`sessions[`/`session_mgr.`
- **快捷键**：Ctrl+N（新建）/ Ctrl+P（上一个）/ Ctrl+W（关闭）会话循环
- **/split 命令**：`SplitCommand` 创建新 session 并切换
- **状态栏**：显示 Ctrl+N/P/W 快捷键提示（仅多 session 时）
- **事件处理**：键盘/鼠标事件中大量 `session_mgr.sessions[session_mgr.active]` 间接访问
- **headless_test**：多处调用 `app.new_session()` 测试多 session 场景

## 期望改进方向

1. **SessionManager 保留但限制 len=1**：`new_session()` 改为先销毁旧 session 再创建新 session（不并发），`close_session()`/`switch_next/prev` 不再生效
2. **UI 布局改为纯单列**：删除 `session_areas` 多列计算逻辑，`main_ui` 只渲染单个 session 的垂直布局
3. **删除 /split 命令**：移除 `SplitCommand` 及其注册
4. **删除多 session 快捷键**：移除 Ctrl+N/P/W 的会话循环处理
5. **/clear 仍支持**：通过销毁旧 session + 创建新 ACP session 实现新对话，不并发
6. **简化事件处理**：`session_mgr.sessions[active]` 简化为 `session_mgr.current()`（语义不变但 mental model 更简单）

## 涉及文件

核心文件（需删除或大幅修改）：

- `peri-tui/src/command/session/split.rs`（17 行）—— 删除
- `peri-tui/src/command/session/mod.rs` —— 移除 split 模块
- `peri-tui/src/app/session_manager.rs`（46 行）—— 限制 len=1，移除 session_areas
- `peri-tui/src/app/mod.rs` —— `new_session()`/`close_session()`/`switch_*_session()` 重写
- `peri-tui/src/ui/main_ui/mod.rs`（48 处引用）—— 多列布局改单列

高频引用文件（需 `sessions[active]` → `current()` 简化）：

- `peri-tui/src/event/keyboard/normal_keys.rs`（74 处）
- `peri-tui/src/event/mod.rs`（67 处）
- `peri-tui/src/ui/headless_test.rs`（198 处）
- `peri-tui/src/app/agent_submit.rs`（55 处）
- `peri-tui/src/app/thread_ops.rs`（65 处）
- `peri-tui/src/app/agent_ops/lifecycle.rs`（48 处）
- `peri-tui/src/ui/main_ui/message_area.rs`（28 处）
- `peri-tui/src/ui/main_ui/status_bar.rs`（13 处）—— 移除 Ctrl+N/P/W 提示
- 以及约 70 个其他文件中的零散引用

## 备注

- 此为纯重构，不影响 ACP 层（`peri-acp` 的 session 管理保持不变）
- `peri-acp` 的 `SessionStore` 仍支持多 session 存储（用于 `/history` 恢复、`/fork` 分叉等），只是 TUI 不再同时显示多个
- 移除后，用户如需多窗口可通过 `tmux split-window` 运行多个 peri 实例
