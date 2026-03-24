# TUI 领域

## 领域综述

TUI 领域负责交互式终端界面的实现，包括渲染引擎、事件处理、命令系统、面板管理和与 Agent 核心的集成。

核心职责：
- 双线程渲染：独立渲染线程计算 Markdown 解析和行包装，UI 线程只从 `RenderCache` 读取可见行，按需重绘
- 事件处理：crossterm 输入拦截、命令解析（`/` 前缀）、弹窗状态管理
- 命令系统：`/model`、`/history`、`/clear`、`/help`、`/compact`
- 多会话管理：SQLite 持久化，`/history` 面板浏览
- 弹窗系统：HITL 审批弹窗、AskUser 问答弹窗、Model/Agents/Thread 配置面板

## 核心流程

### 渲染管道（双线程）

```
App (UI 线程)
  ↓ AgentEvent
render_tx.try_send(RenderEvent)
  ↓
RenderTask（渲染线程）
  ↓ markdown 解析 / 行包装
Arc<RwLock<RenderCache>>
  ↓ version 变化时
terminal.draw(main_ui::render)
```

### 事件处理循环

```
Event::Key → 命令前缀匹配（/）
           → 普通字符输入（loading 时缓冲 pending_messages）
           → Ctrl+V（剪贴板图片）
           → Del（删除最后一张附件）
           → Enter（loading 缓冲，非 loading 提交）
           → Tab/Shift+Tab/方向键（面板导航）

poll_agent() → AgentEvent → handle_agent_event → view_messages + render_tx
```

### 多模态消息提交流程

```
Ctrl+V（剪贴板有图片）
  → arboard 读取 RGBA → png 编码 → base64
  → pending_attachments.push()
  → 渲染附件栏

submit_message(text)
  → mem::take(pending_attachments)
  → AgentInput::blocks([Text, Image, ...])
  → run_universal_agent(provider, agent_input)
  → pending_attachments 清空
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 渲染框架 | ratatui ≥0.30，主 UI 线程 + 独立渲染线程 |
| 渲染线程同步 | `parking_lot::RwLock<RenderCache>` + `tokio::sync::Notify` 零 sleep |
| Headless 测试 | `ratatui::backend::TestBackend`，`#[cfg(test)]` 隔离 |
| 剪贴板 | `arboard` crate，跨平台，macOS/Linux/Windows |
| 图片编码 | `png` crate（RGBA→PNG）+ `base64` crate |
| 命令解析 | 前缀唯一匹配（`/` 开头），`default_registry()` 注册 |
| 模型别名 | Opus/Sonnet/Haiku 三档，`ModelAliasMap` 独立绑定 Provider+Model |
| 输入缓冲 | `pending_messages: Vec<String>`，Done/Error 时合并发送 |
| 弹窗滚动 | `scroll_offset: u16`，`ensure_cursor_visible()`，80% 高度上限 |

## Feature 附录

### 20260324_F001_tui-clipboard-image-paste
**摘要:** Ctrl+V 粘贴剪贴板图片作为多模态消息发送
**关键决策:**
- 依赖: arboard 3 + png 0.17 + base64 0.22
- 数据结构: `PendingAttachment { label, media_type, base64_data, size_bytes }`
- run_universal_agent 签名变更: `input: String` → `input: AgentInput`
- 附件栏 Layout: 6-slot，新增 `Constraint::Length(attachment_height)`
**归档:** [链接](../../archive/feature_20260324_F001_tui-clipboard-image-paste/)
**归档日期:** 2026-03-24

### 20260324_F001_compact-context-command
**摘要:** /compact 指令调用 LLM 将对话历史压缩为结构化摘要
**关键决策:**
- 独立压缩任务: `tokio::spawn compact_task`，不经过 ReAct 循环
- 消息格式化: [用户]/[助手]/[工具结果] 标签，跳过 System
- 摘要存储: `BaseMessage::system(summary)` 替换 agent_state_messages
- view_messages 保留最近 10 条，头部插入压缩提示
- 空历史保护: is_empty() → 直接返回，不进入 loading
- 失败保护: CompactError 不修改历史
**归档:** [链接](../../archive/feature_20260324_F001_compact-context-command/)
**归档日期:** 2026-03-24

### 20260323_F001_tui-render-perf
**摘要:** 双线程渲染架构：独立渲染线程 + 按需重绘，消除消息多时卡顿
**关键决策:**
- 渲染线程: `tokio::spawn RenderTask::run`，持有私有消息副本
- RenderCache: `lines: Vec<Line<'static>>` + `version: u64`
- AppendChunk 增量: 仅重新渲染最后一条 assistant 消息
- 按需重绘: `last_render_version` 比较，`needs_redraw` 标志
**归档:** [链接](../../archive/feature_20260323_F001_tui-render-perf/)
**归档日期:** 2026-03-24

### 20260323_F002_tui-headless-mode
**摘要:** Headless 测试模式：TestBackend + 渲染线程零 sleep 同步
**关键决策:**
- App::new_headless(): `TestBackend::new(w, h)` + `spawn_render_thread`
- push_agent_event() + process_pending_events(): 测试注入事件，复用 handle_agent_event
- wait_for_render(): `notify.notified().await`，零轮询
- snapshot() / contains(): 遍历 buffer cell 拼接纯文本
- 条件编译: `#[cfg(any(test, feature = "headless"))]`
**归档:** [链接](../../archive/feature_20260323_F002_tui-headless-mode/)
**归档日期:** 2026-03-24

### 20260323_F003_tui-status-panel
**摘要:** TODO 状态固定面板、工具调用颜色分层、路径参数缩短
**关键决策:**
- TODO 面板: 独立 Layout slot，`todo_height` 动态计算，颜色分类（黄/灰/白）
- 工具颜色分层: 工具名（颜色+BOLD）+ 参数（DarkGray）
- 路径缩短: `strip_cwd(prefix)`，bash 和 search_files_rg 除外
- App 状态变更: `todo_items: Vec<TodoItem>`，删除 `todo_message_index`
**归档:** [链接](../../archive/feature_20260323_F003_tui-status-panel/)
**归档日期:** 2026-03-24

### 20260323_F001_model-alias-provider-mapping
**摘要:** Opus/Sonnet/Haiku 三级别名映射，支持 /model <alias> 快捷切换
**关键决策:**
- 数据结构: `ModelAliasConfig { provider_id, model_id }` + `ModelAliasMap { opus, sonnet, haiku }`
- 向后兼容迁移: 检测旧 provider_id 字段，自动填充 opus 别名
- 空 model_id fallback: anthropic→claude-sonnet-4-6, 其他→gpt-4o
- /model <alias> 命令: 直接切换 active_alias，无需打开面板
**归档:** [链接](../../archive/feature_20260323_F001_model-alias-provider-mapping/)
**归档日期:** 2026-03-24

### 20260323_F005_tui-bug-fixes
**摘要:** 修复弹窗滚动/粘贴换行/loading 输入锁死三个 TUI bug
**关键决策:**
- 弹窗滚动: 所有面板 popup_height ≤ area.height * 4/5，`scroll_offset` + `ensure_cursor_visible`
- Bracketed Paste: EnableBracketedPaste + Event::Paste → textarea.insert_str
- Loading 缓冲: pending_messages + "已缓存 N 条" 标题，Done/Error 时合并发送
**归档:** [链接](../../archive/feature_20260323_F005_tui-bug-fixes/)
**归档日期:** 2026-03-24

### 20260322_F002_data-pipeline-unification
**摘要:** 实时流式与历史恢复统一工具调用参数显示，含 tool_call_id 匹配
**关键决策:**
- ToolStart 扩展: 增加 `tool_call_id: String` 字段
- prev_ai_tool_calls: 存储 `(id, name, input)` 三元组
- 统一格式化: `format_tool_call_display()` 被实时和历史共用
- 降级处理: 无匹配时使用 tool_call_id 作为工具名
**归档:** [链接](../../archive/feature_20260322_F002_data-pipeline-unification/)
**归档日期:** 2026-03-24

### 20260322_F001_message-render-refactor
**摘要:** MessageViewModel 中间层重构，tui-markdown 渲染，工具折叠
**关键决策:**
- ViewModel 变体: UserBubble / AssistantBubble / ToolBlock / SystemNote / TodoStatus
- Markdown 渲染: `tui-markdown` crate，`ensure_rendered()` dirty flag 降频
- 工具折叠: collapsed 状态，Tab 键切换，默认折叠
- ChatMessage 完全移除: 替换为 view_messages
**归档:** [链接](../../archive/feature_20260322_F001_message-render-refactor/)
**归档日期:** 2026-03-24

---

## 相关 Feature
- → [agent.md#20260322_F001_agent-storage-refactor](./agent.md#20260322_F001_agent-storage-refactor) — SQLite 持久化，TUI 消息渲染依赖此存储
