# 死代码与未完成功能清理：CaptureLLM 未接入测试、多个未使用字段

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-14

## 问题描述

编译器零警告（`cargo build` + `cargo clippy`），但通过 `#[allow(dead_code/unused)]` 注解抑制了 24 处潜在问题。其中 1 处为真正的死代码（完整实现但未接入），多处为未完成的功能预留字段。

## 现状数据

### 真正死代码（1 处）

| 位置 | 类型 | 说明 |
|------|------|------|
| `rust-agent-middlewares/src/subagent/tool_test.rs:686-702` | struct+impl | `CaptureLLM` 完整实现了 `ReactLLM` trait（含 `new()` + trait impl），但在其之后定义的 9 个测试函数无一使用它。属于"写了但忘接入"的死代码。 |

### 未完成功能 / 未使用字段（5 处）

| 位置 | 类型 | 说明 |
|------|------|------|
| `rust-agent-tui/src/app/message_pipeline.rs:74-79` | struct fields | `PendingTool` 的 `tool_call_id`/`name`/`input` 三个字段被赋值但从未读取，存入 `pending_tools` HashMap 后未被消费。可能是未完成的 gap 展示逻辑。 |
| `rust-agent-middlewares/src/tool_search/tool_index.rs:23` | struct field | `TfIdfIndex::doc_freqs` 被填充但从未读取。TF-IDF 算法中 IDF 部分未实现，要么补全算法要么移除字段。 |
| `rust-agent-tui/src/app/login_panel.rs:198` | method | `request_delete()` 进入删除确认模式的方法，功能完整但无 UI 入口调用。 |
| `rust-agent-tui/src/app/panel_ops.rs:919` | method | `agent_panel_clear()` 关闭 Agent 面板但不改变 agent_id，与 `agent_panel_select()` 成对但无调用方。 |
| `rust-agent-tui/src/app/plugin_panel.rs:1535` | method | `discover_toggle_selected()` 切换插件发现列表选中状态，功能完整但无 UI 入口。 |

### 多余注解（2 处）

| 位置 | 说明 |
|------|------|
| `rust-agent-tui/src/app/agent_comm.rs:1-11` | 5 个 `#[allow(unused)]` 导入（`AgentCancellationToken`, `BaseMessage`, `mpsc`, `AgentEvent`, `InteractionPrompt`）实际全部在使用，注解多余。 |
| `perihelion-lsp/src/client.rs:43` | `OpenFileInfo::language_id` 从未读取。 |

### TODO 注释（3 处）

| 位置 | 内容 |
|------|------|
| `rust-agent-tui/src/prompt.rs:16` | `subagent_enabled: true, // TODO: 从中间件注册状态推断` |
| `rust-agent-tui/src/prompt.rs:17` | `cron_enabled: true, // TODO: 从中间件注册状态推断` |
| `rust-agent-tui/src/prompt.rs:18` | `skills_enabled: true, // TODO: 从中间件注册状态推断` |

### 注释掉的代码块

0 处。代码库在这方面非常整洁。

## 期望改进方向

1. `CaptureLLM`：接入 Fork path 测试或删除
2. `PendingTool` 字段：明确意图，补全 gap 展示逻辑或清理
3. `TfIdfIndex::doc_freqs`：补全 IDF 算法或移除字段及填充逻辑
4. 无调用入口的方法：接入 UI 快捷键或标记 `#[cfg(test)]`
5. 多余 `#[allow(unused)]`：直接移除
6. PromptFeatures TODO：实现从中间件注册状态动态推断

## 处理决策

| # | 决策 | 原因 |
|---|------|------|
| 1 | CaptureLLM 删除 | 完全未接入，9 个测试无一使用 |
| 2 | PendingTool.name/input 移除 #[allow] | 实际在 `build_tail_vms()` 中被读取（:722-723），tool_call_id 保留 allow（仅 HashMap key 使用） |
| 3 | TfIdfIndex::doc_freqs 移除 | 仅存储不读取；局部变量保留（IDF 计算中仍使用） |
| 4a | request_delete 移除 #[allow] | 有 Ctrl+D 快捷键调用（:474）和测试（:1014） |
| 4b | agent_panel_clear 删除 | 零调用方 |
| 4c | discover_toggle_selected 删除 | 零调用方 |
| 5 | agent_comm.rs 移除所有 #[allow(unused)] | 5 个导入全部在使用 |
| 5 | OpenFileInfo::language_id 移除 | 存储但不读取 |
| 6 | PromptFeatures TODO 移除 | 从中间件注册状态推断需架构变更（detect() 无中间件上下文），属于独立 feature

## 涉及文件

- `rust-agent-middlewares/src/subagent/tool_test.rs`（CaptureLLM 死代码）
- `rust-agent-tui/src/app/message_pipeline.rs`（PendingTool 未使用字段）
- `rust-agent-middlewares/src/tool_search/tool_index.rs`（doc_freqs 未使用）
- `rust-agent-tui/src/app/agent_comm.rs`（多余 allow 注解）
- `rust-agent-tui/src/prompt.rs`（3 个 TODO）
