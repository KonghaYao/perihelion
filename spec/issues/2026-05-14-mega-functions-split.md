# 超长函数拆分：event.rs 和 agent_ops.rs 各有 1000+ 行单函数

**状态**：Open
**优先级**：高
**创建日期**：2026-05-14

## 问题描述

`rust-agent-tui/src/event.rs` 的 `handle_event` 函数约 1120 行，`rust-agent-tui/src/app/agent_ops.rs` 的 `handle_agent_event` 函数约 890 行。两个函数是整个代码库中最长的单函数，认知复杂度极高，修改时难以理解和测试。

## 现状数据

| 函数 | 文件 | 约行数 | 主要职责 |
|------|------|--------|---------|
| `handle_event` | `rust-agent-tui/src/event.rs` | 1120 (L254-1335) | 键盘/粘贴/鼠标全部事件分发 |
| `handle_agent_event` | `rust-agent-tui/src/app/agent_ops.rs` | 890 (L7-896) | 20+ 种 AgentEvent 变体的 match 分支 |
| `open_plugin_panel` | `rust-agent-tui/src/app/panel_ops.rs` | 280 (L334-615) | 插件列表加载 + manifest 解析 + Discover 构建 |
| `invoke` | `rust-create-agent/src/llm/anthropic.rs` | 300 (L517-816) | 请求构建 + HTTP + 响应解析 + 缓存 |
| `run_universal_agent` | `rust-agent-tui/src/app/agent.rs` | 500 (L62-617) | 15 个中间件实例化 + 事件注册 + 执行 |

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

- `rust-agent-tui/src/event.rs`（1379 行）—— handle_event 所在文件
- `rust-agent-tui/src/app/agent_ops.rs`（1500 行）—— handle_agent_event 所在文件
- `rust-agent-tui/src/app/panel_ops.rs`（1084 行）—— open_plugin_panel 所在文件
- `rust-create-agent/src/llm/anthropic.rs`（1983 行）—— invoke 所在文件
- `rust-agent-tui/src/app/agent.rs`（797 行）—— run_universal_agent 所在文件
