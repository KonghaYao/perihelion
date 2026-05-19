# 超长函数拆分：event.rs 和 agent_ops.rs 各有 1000+ 行单函数

**状态**：Closed
**优先级**：高
**创建日期**：2026-05-14
**关闭日期**：2026-05-19

## 问题描述

`peri-tui/src/event.rs` 的 `handle_event` 函数约 1120 行，`peri-tui/src/app/agent_ops.rs` 的 `handle_agent_event` 函数约 890 行。两个函数是整个代码库中最长的单函数，认知复杂度极高，修改时难以理解和测试。

## 现状数据

| 函数 | 文件 | 约行数 | 主要职责 |
|------|------|--------|---------|
| `handle_event` | `peri-tui/src/event.rs` | 1120 (L254-1335) | 键盘/粘贴/鼠标全部事件分发 |
| `handle_agent_event` | `peri-tui/src/app/agent_ops.rs` | 890 (L7-896) | 20+ 种 AgentEvent 变体的 match 分支 |
| `open_plugin_panel` | `peri-tui/src/app/panel_ops.rs` | 280 (L334-615) | 插件列表加载 + manifest 解析 + Discover 构建 |
| `invoke` | `peri-agent/src/llm/anthropic.rs` | 300 (L517-816) | 请求构建 + HTTP + 响应解析 + 缓存 |
| `run_universal_agent` | `peri-tui/src/app/agent.rs` | 500 (L62-617) | 15 个中间件实例化 + 事件注册 + 执行 |

### `handle_event` 的问题

单个 async fn 处理所有键盘/粘贴/鼠标事件，包含：Setup 向导拦截、PanelManager 分发、HITL/AskUser 弹窗、Ctrl+C 退出、历史浏览、Tab 补全、Enter 提交、PageUp/Down、Ctrl+N/P session 切换、粘贴图片、鼠标滚轮/点击/拖拽/释放。所有逻辑平铺在一个函数体内，无法独立测试任何事件分支。

### `handle_agent_event` 的问题

巨型 match 分支处理 20+ 种 AgentEvent 变体，每个分支都涉及 pipeline 操作 + Langfuse 追踪 + UI 状态更新。事件之间有部分共享逻辑（如 `request_rebuild()` 调用），但被分散在各分支中，难以提取公共模式。

## 期望改进方向

按事件类型将超长函数拆分为独立方法或子模块：

- `handle_event` → `handle_key_event()` / `handle_paste()` / `handle_mouse()`
- `handle_agent_event` → 每个事件变体提取为独立方法（如 `handle_subagent_start()` / `handle_context_warning()` 等）
- `open_plugin_panel` → 数据构建逻辑提取为 `build_discover_data()`，视图构建提取为 `build_marketplace_entries()`
- `invoke` → `build_request_body()` + `parse_response()`
- `run_universal_agent` → `build_middleware_chain()` 提取中间件组装逻辑

## 涉及文件

- `peri-tui/src/event.rs`（1379 行）—— handle_event 所在文件
- `peri-tui/src/app/agent_ops.rs`（1500 行）—— handle_agent_event 所在文件
- `peri-tui/src/app/panel_ops.rs`（1084 行）—— open_plugin_panel 所在文件
- `peri-agent/src/llm/anthropic.rs`（1983 行）—— invoke 所在文件
- `peri-tui/src/app/agent.rs`（797 行）—— run_universal_agent 所在文件

## 实施（2026-05-19）

基于 slop scan 大文件分析，对 5 个最大文件进行了系统性拆分（分支 `refactor/split-large-files`）：

| # | 原文件 | 前大小 | 后 | 方法 |
|---|--------|-------|-----|------|
| 1 | `event.rs` | 1447 | `event/` (mouse.rs 175 + keyboard.rs 746 + mod.rs 576) | `handle_event` 拆分为 Key→keyboard::handle_key_event / Mouse→内联 / Paste→内联 |
| 2 | `plugin_panel.rs` | 1817 | `plugin_panel/` (types.rs 205 + handlers.rs 841 + mod.rs 812) | 类型/处理器/协调三向拆分 |
| 3 | `agent_ops.rs` | 1385 | 1053 + `agent_ops_interaction.rs` 247 | `handle_agent_event` 的 7 个 arm 提取为独立函数，3 处清理代码统一为 `cleanup_agent_state()` |
| 4 | `panel_ops.rs` | 1190 | ~140 + 8 个 `panel_*.rs` 文件 | 按面板类型垂直拆分 |
| 5 | `main.rs` | 952 | 497 + `acp_stdio.rs` 462 | `run_acp_stdio()` 及相关类型提取 |

**已解决**：`handle_event`（完成拆分）、`handle_agent_event`（减至 ~360 行分发器）、`open_plugin_panel`（面板文件拆分后函数自然定位）。

**遗留**：

- ~~`peri-agent/src/llm/anthropic.rs` 的 `invoke`（~300 行）~~ → 已拆分：`anthropic.rs` 拆为 `anthropic/` 目录（`mod.rs` + `invoke.rs` + `stream.rs` + `cache.rs`），`build_request_body()` 已提取，`invoke` 方法减至 ~205 行，再提取 `handle_anthropic_response()` → ~45 行
- ~~`peri-tui/src/app/agent.rs` 的 `run_universal_agent`（~500 行）~~ → 已移除：函数整体被 ACP 架构替代（`agent_submit.rs:188` 注释确认），`agent.rs` 现仅 198 行，只含 `map_executor_event()`

总计减少量：6791 行→19 个文件。Build/525 tests/clippy/fmt 全部通过。
