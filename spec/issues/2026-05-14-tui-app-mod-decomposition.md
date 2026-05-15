# TUI app/ 目录模块化：app/mod.rs 48 个子模块、plugin_panel.rs 2017 行

**状态**：Open
**优先级**：高
**创建日期**：2026-05-14

## 问题描述

`rust-agent-tui/src/app/mod.rs` 包含 48 个子模块声明，是整个代码库内聚度最低的文件。`app/` 目录下的文件普遍过大（7 个文件超 1000 行），混合了面板组件定义、App struct 扩展操作、状态管理等多种职责。

## 现状数据

### `app/mod.rs` 子模块分布（48 个）

| 类别 | 子模块 | 数量 |
|------|--------|------|
| 会话管理 | session_manager, chat_session, session_metadata | 3 |
| Agent 通信 | agent_comm, agent_ops, agent_render, agent_submit, agent_compact, agent_events_bg, agent_events_oauth, agent_events_plugin | 8 |
| 面板 | panel_manager, panel_component, panel_list, panel_ops, agent_panel, config_panel, hooks_panel, model_panel, plugin_panel, status_panel, memory_panel, login_panel, setup_wizard, mcp_panel | 14 |
| 交互处理 | hitl_ops, hitl_prompt, ask_user_ops, ask_user_prompt, oauth_prompt, hint_ops | 6 |
| 后台功能 | cron_ops, cron_state, history_ops, thread_ops, langfuse_state | 5 |
| 核心状态 | ui_state, global_ui_state, service_registry, message_state, command_system, text_selection, tool_display, interaction_broker, events | 9 |
| 其他 | mod, panel_component, command_system, ... | 3 |

### app/ 目录大文件

| 文件 | 行数 | 主要问题 |
|------|------|---------|
| `plugin_panel.rs` | 2017 | 面板组件 + App 扩展操作混合，65 个 pub 声明 |
| `agent_ops.rs` | 1500 | handle_agent_event 890 行巨型 match |
| `mcp_panel.rs` | 1218 | 面板组件 + App 操作混合 |
| `panel_ops.rs` | 1084 | 40+ 个面板操作函数，open_plugin_panel 280 行 |
| `login_panel.rs` | 1021 | 表单状态机 + App 操作混合 |
| `message_pipeline.rs` | 912 | 流式事件处理 + reconcile + tail 构建 |
| `setup_wizard.rs` | 804 | 向导步骤状态机（职责相对单一） |
| `agent.rs` | 797 | run_universal_agent 500 行 |

### pub 接口过多

| 文件 | pub 声明数 |
|------|-----------|
| `plugin_panel.rs` | 65 |
| `panel_manager.rs` | 35 |
| `setup_wizard.rs` | 30 |
| `mod.rs` | 26 |
| `message_view.rs` | 26 |
| `message_pipeline.rs` | 25 |

## 期望改进方向

将 `app/mod.rs` 按职责拆分为子目录：

- `app/core/` — App struct + session 管理 + 核心状态
- `app/panels/` — 已有部分面板独立，将 `agent_*`/`hitl_*`/`ask_user_*`/`cron_*` 等操作模块归入
- `app/interaction/` — 人机交互相关（hitl_ops, ask_user_ops, hint_ops 等）

对面板文件统一拆分模式：面板 struct/impl 保留原文件，App 上的 `*_panel_*` 操作函数拆到 `*_ops.rs`（与已有 `mcp_ops.rs`、`login_ops.rs` 模式一致）。

`panel_manager.rs` 的内部类型降为 `pub(crate)`，减少 API 面。

## 涉及文件

- `rust-agent-tui/src/app/mod.rs`（762 行，48 子模块）
- `rust-agent-tui/src/app/plugin_panel.rs`（2017 行）
- `rust-agent-tui/src/app/mcp_panel.rs`（1218 行）
- `rust-agent-tui/src/app/panel_ops.rs`（1084 行）
- `rust-agent-tui/src/app/login_panel.rs`（1021 行）
- `rust-agent-tui/src/app/panel_manager.rs`（35 pub）
