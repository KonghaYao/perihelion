# TUI 领域

## 领域综述

TUI 领域负责交互式终端界面的实现，包括渲染引擎、事件处理、命令系统、面板管理和与 Agent 核心的集成。

核心职责：
- 双线程渲染：独立渲染线程计算 Markdown 解析（pulldown-cmark）和行包装，UI 线程只从 `RenderCache` 读取可见行，按需重绘
- 事件处理：crossterm 输入拦截、命令解析（`/` 前缀）、弹窗状态管理
- 命令系统：`/model`、`/history`、`/clear`、`/help`、`/compact`、`/relay`（远程控制）
- 多会话管理：SQLite 持久化，`/history` 面板浏览
- 弹窗系统：HITL 审批弹窗、AskUser 问答弹窗（支持 header 短标签 + 选项 description）、Model/Agents/Thread/Relay 配置面板
- SubAgent 层级展示：SubAgentGroup 可折叠块，滑动窗口显示最近 4 步
- Skill 全文预加载：消息含 `#skill-name` 时通过 SkillPreloadMiddleware 将 skill 全文注入 agent state

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
| Markdown 渲染 | pulldown-cmark 0.12（CommonMark 规范，事件驱动，自制 ratatui 渲染器） |
| 渲染线程同步 | `parking_lot::RwLock<RenderCache>` + `tokio::sync::Notify` 零 sleep |
| Headless 测试 | `ratatui::backend::TestBackend`，`#[cfg(test)]` 隔离 |
| 剪贴板 | `arboard` crate，跨平台，macOS/Linux/Windows |
| 图片编码 | `png` crate（RGBA→PNG）+ `base64` crate |
| 命令解析 | 前缀唯一匹配（`/` 开头），`default_registry()` 注册 |
| 模型别名 | Opus/Sonnet/Haiku 三档，`ModelAliasMap` 独立绑定 Provider+Model |
| 输入缓冲 | `pending_messages: Vec<String>`，Done/Error 时合并发送 |
| 弹窗滚动 | `scroll_offset: u16`，`ensure_cursor_visible()`，80% 高度上限 |
| SubAgent 展示 | SubAgentGroup ViewModel；滑动窗口 4 条；RenderEvent::UpdateLastMessage 原地更新 |
| 远程控制配置 | RelayPanel View/Edit 模式；RemoteControlConfig 持久化到 ~/.zen-code/settings.json |
| 环境变量注入 | AppConfig.env HashMap，main() 最先调用 inject_env_from_settings()，进程环境变量优先 |
| 文件组织 | app/ 拆分 8 子文件；ui/ 拆分 popups/、panels/ 子目录；pub(super) 可见性 |

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

### feature_20260324_F001_ratatui-markdown-renderer
**摘要:** pulldown-cmark 替代 tui-markdown，自制 ratatui Markdown 渲染器
**关键决策:**
- pulldown-cmark 0.12（CommonMark 规范，事件驱动）替代 tui-markdown 0.3
- RenderState 累积行内 Span，事件驱动构建 Text<'static>
- dirty flag 全量重解析（10KB 约 30μs，帧预算 16.7ms 内可接受）
- parse_markdown / ensure_rendered 接口不变，message_render.rs 零改动
**归档:** [链接](../../archive/feature_20260324_F001_ratatui-markdown-renderer/)
**归档日期:** 2026-03-27

### feature_20260325_F002_large-file-refactor
**摘要:** app/mod.rs 和 main_ui.rs 大文件拆分为多子文件
**关键决策:**
- Rust 同模块多文件 impl 块，app/ 拆分为 8 个子文件（hitl_prompt/ask_user_prompt/agent_ops 等）
- ui/ 拆分为 popups/（hitl/ask_user/hints）和 panels/（model/thread_browser/agent）子目录
- 纯机械搬移，禁止顺手重构，pub use 重导出保持外部路径不变
- pub(super) 可见性约束，render() 为唯一对外入口
**归档:** [链接](../../archive/feature_20260325_F002_large-file-refactor/)
**归档日期:** 2026-03-27

### feature_20260326_F001_subagent-message-hierarchy
**摘要:** SubAgent 执行消息分层为可折叠块
**关键决策:**
- 纯 TUI 层感知（方案 A）：利用 launch_agent ToolStart/End 事件作为边界
- SubAgentGroup ViewModel：滑动窗口最多 4 条，total_steps 单独累计
- RenderEvent::UpdateLastMessage 原地更新，不触发全量重建
- 完成后 Enter 键折叠/展开，折叠态只显示摘要行
**归档:** [链接](../../archive/feature_20260326_F001_subagent-message-hierarchy/)
**归档日期:** 2026-03-27

### feature_20260326_F004_remote-control-panel
**摘要:** /relay 命令面板：TUI 内配置并持久化远程控制参数
**关键决策:**
- RelayPanel View/Edit 两模式（参考 ModelPanel 设计）
- RemoteControlConfig 结构化替代 extra 字段（向后兼容 extra.relay_*）
- --remote-control 无参数时从配置读取；无 --remote-control 参数则不自动连接
- Token 脱敏显示（****last4****），存储在 ~/.zen-code/settings.json
**归档:** [链接](../../archive/feature_20260326_F004_remote-control-panel/)
**归档日期:** 2026-03-27

### feature_20260328_F001_skill-preload-on-send
**摘要:** TUI 发送含 #skill-name 消息时自动全文预加载对应 skill
**关键决策:**
- AgentRunConfig 新增 preload_skills: Vec<String>
- submit_message 用正则 `#([a-zA-Z0-9_-]+)` 解析 skill 名列表
- run_universal_agent 有 preload_skills 时插入 SkillPreloadMiddleware（紧随 SkillsMiddleware 之后）
- 空列表时 SkillPreloadMiddleware.before_agent early return，无额外开销
- 找不到的 skill 名静默跳过
**归档:** [链接](../../archive/feature_20260328_F001_skill-preload-on-send/)
**归档日期:** 2026-03-28

### feature_20260328_F003_test-coverage-improvement
**摘要:** 四高风险区域补充 55+ 单元测试提升覆盖率
**关键决策:**
- 文件系统工具测试: tempfile TempDir 隔离，6 个工具各 4-5 个测试（正常/边界/错误）
- Relay Server 测试: auth.rs 5 个 token 验证；client/mod.rs 7 个历史缓存（new_for_testing 绕过 WS）
- AskUserTool 测试: MockBroker mock broker，10 个测试覆盖参数解析和返回格式
- TUI 命令测试: StubCommand + headless App，8 个 dispatch/prefix 匹配测试
- 新增总数 ~56 个测试，工具实现层覆盖率 ~40%→~80%
**归档:** [链接](../../archive/feature_20260328_F003_test-coverage-improvement/)
**归档日期:** 2026-03-29

### feature_20260328_F004_settings-env-injection
**摘要:** settings.json env 字段替代 .env 注入环境变量
**关键决策:**
- AppConfig.env: Option<HashMap<String, String>>，serde default + skip_serializing_if
- inject_env_from_settings(): main() 最先调用，std::env::var(key).is_err() 判断不存在再 set_var
- 优先级: 进程环境变量 > settings.json env 字段
- 错误处理: 文件不存在/env 缺失/JSON 解析失败均静默跳过（不 panic）
- 移除 dotenvy 依赖
**归档:** [链接](../../archive/feature_20260328_F004_settings-env-injection/)
**归档日期:** 2026-03-29

### feature_20260326_F008_statusbar-msgcount-relay-flag
**摘要:** 状态栏显示消息计数，禁止 relay 隐式自动连接
**关键决策:**
- 消息数从 app.view_messages.len() 直接读取，无需新增事件或字段
- 无 --remote-control 参数时 try_connect_relay else 分支直接 return，不读配置
**归档:** [链接](../../archive/feature_20260326_F008_statusbar-msgcount-relay-flag/)
**归档日期:** 2026-03-27

---

## 相关 Feature
- → [agent.md#20260322_F001_agent-storage-refactor](./agent.md#20260322_F001_agent-storage-refactor) — SQLite 持久化，TUI 消息渲染依赖此存储
- → [langfuse.md#feature_20260324_F001_langfuse-tui-monitoring](./langfuse.md#feature_20260324_F001_langfuse-tui-monitoring) — Langfuse 追踪集成在 TUI 的 app/agent.rs
- → [agent.md#feature_20260328_F001_ask-user-question-align](./agent.md#feature_20260328_F001_ask-user-question-align) — AskUser 弹窗展示更新（header + description），TUI 弹窗同步更新
