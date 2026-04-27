# TUI 领域

## 领域综述

TUI 领域负责交互式终端界面的实现，包括渲染引擎、事件处理、命令系统、面板管理和与 Agent 核心的集成。

核心职责：
- 双线程渲染：独立渲染线程计算 Markdown 解析（pulldown-cmark）和行包装，UI 线程只从 `RenderCache` 读取可见行，按需重绘
- 事件处理：crossterm 输入拦截、命令解析（`/` 前缀）、弹窗状态管理
- 命令系统：`/model`、`/history`、`/clear`、`/help`、`/compact`、`/relay`（远程控制）、`/loop`（定时任务注册）、`/cron`（定时任务管理面板）
- 多会话管理：SQLite 持久化，`/history` 面板按 cwd 过滤当前工作区对话
- 弹窗系统：HITL 审批弹窗、AskUser 问答弹窗（支持 header 短标签 + 选项 description + 动态高度计算）、Model/Agents/Thread/Relay 配置面板
- SubAgent 层级展示：SubAgentGroup 可折叠块，滑动窗口显示最近 4 步
- Skill 全文预加载：消息含 `#skill-name` 时通过 SkillPreloadMiddleware 将 skill 全文注入 agent state
- Setup Wizard：首次启动自动检测配置完整性，三步引导（Provider → API Key → Model Alias），原子写回 settings.json
- 配色系统 v1.1：橙色仅保留最高优先级交互（命令输入框），工具名三级分层（bash=ACCENT / 写操作=WARNING / 只读=MUTED），配置面板边框 MUTED 降噪
- App 结构体拆分：AppCore/AgentComm/RelayState/LangfuseState 四子结构体，对外 API 通过转发方法保持不变
- Welcome Card：空消息时显示品牌 ASCII Art Logo + 功能亮点 + 命令提示，发送消息后自动消失
- Sticky Human Message Header：聊天区顶部固定显示最后一条 Human 消息（1-3 行截断），滚动时不随之移动

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
| App 结构体 | AppCore/AgentComm/RelayState/LangfuseState 四子结构体（共 37 字段），Deref 代理访问 |
| 配色方案 | v1.1 降噪：橙色仅用于输入框，工具名 bash=ACCENT/写操作=WARNING/只读=MUTED，面板边框 MUTED |
| Setup Wizard | 三步引导（Provider → API Key → Model Alias），save_setup() 原子写回 settings.json |
| Welcome Card | 空消息时 ASCII Art Logo + 功能亮点，窄屏降级为文字标题 |
| Sticky Header | 最后一条 Human 消息 1-3 行截断固定在聊天区顶部 |
| 历史过滤 | /history 面板按 cwd 过滤 ThreadMeta，标题含工作区路径 |
| 定时任务面板 | /loop 注册（cron 表达式 + prompt），/cron 面板管理（导航/删除/切换启用） |
| /compact 迁移 | 执行后创建新 Thread 保留旧历史，新 Thread 以摘要 System 消息开头 |
| AskUser 高度 | wrapped_line_count 动态换行计算，visible_height 替代硬编码滚动区域 |
| 弹窗滚动 | `scroll_offset: u16`，`ensure_cursor_visible(visible_height)`，Tab 切换重置 scroll_offset |

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

### feature_20260408_F001_askuser-dialog-height
**摘要:** AskUser 弹窗高度计算修复，滚动可见高度动态化
**关键决策:**
- wrapped_line_count 辅助函数：根据弹窗内宽度和 unicode-width 计算文本实际换行行数
- active_panel_height 重写：question/options/description 均使用 wrapped_line_count 估算
- visible_height 字段：AskUserBatchPrompt 新增字段，渲染函数每帧回写 content_area.height
- Tab 切换重置：next_tab()/prev_tab() 中 scroll_offset 归零
**归档:** [链接](../../archive/feature_20260408_F001_askuser-dialog-height/)
**归档日期:** 2026-04-27

### feature_20260331_F001_history-workspace-tag
**摘要:** /history 面板按 cwd 过滤只显示当前工作区对话
**关键决策:**
- open_thread_browser() 中 threads.into_iter().filter(|t| t.cwd == cwd).collect() 过滤
- ThreadBrowser 面板标题包含 app.cwd 路径
- ThreadMeta 已有 cwd 字段，无需数据库变更
**归档:** [链接](../../archive/feature_20260331_F001_history-workspace-tag/)
**归档日期:** 2026-04-27

### feature_20260330_F005_tui-setup-wizard
**摘要:** 首次启动三步引导（Provider → API Key → Model Alias）
**关键决策:**
- 配置完整性检测：启动时检查 provider/model/api_key，任一缺失触发向导
- 三步向导：Provider 选择（Anthropic/OpenAI Compatible）→ API Key 输入 → Model Alias 配置
- save_setup() 原子写回：先写临时文件再 rename 到 settings.json
- 向导完成后自动重启 Agent 连接
**归档:** [链接](../../archive/feature_20260330_F005_tui-setup-wizard/)
**归档日期:** 2026-04-27

### feature_20260330_F002_tui-color-refresh
**摘要:** 配色系统 v1.1 降噪，橙色聚焦交互，工具名三级分层
**关键决策:**
- 橙色收缩：仅保留命令输入框边框和高亮，其他橙色元素降级为 MUTED/WARNING
- 工具名颜色分层：bash=ACCENT（橙色），write/edit/folder=WARNING（黄色），read/glob/search=MUTED（暗灰）
- 配置面板边框统一为 MUTED，HITL/AskUser 弹窗使用 WARNING
- 工具参数统一 DarkGray 显示
**归档:** [链接](../../archive/feature_20260330_F002_tui-color-refresh/)
**归档日期:** 2026-04-27

### feature_20260330_F001_sticky-human-message-header
**摘要:** 聊天区顶部固定最后一条 Human 消息摘要
**关键决策:**
- 渲染位置：固定在聊天区最顶部，不随消息列表滚动
- 显示规则：1-3 行截断（Str::from(msg).lines().take(3)），空消息/无人类消息时不显示
- 生命周期：发送新消息后更新，/clear 后消失，打开历史 Thread 自动恢复
- 实现位置：main_ui.rs 渲染函数中作为独立 Layout 块
**归档:** [链接](../../archive/feature_20260330_F001_sticky-human-message-header/)
**归档日期:** 2026-04-27

### feature_20260329_F004_app-refactor
**摘要:** App 结构体拆分为 AppCore/AgentComm/RelayState/LangfuseState
**关键决策:**
- 四子结构体：AppCore（UI 状态）、AgentComm（agent channel）、RelayState（relay 客户端）、LangfuseState（追踪配置）
- 共 37 字段拆分，App 持有所有子结构体
- 对外 API 通过 impl App 上的转发方法保持不变，调用方零改动
- Deref 代理：部分内部模块直接 Deref 到子结构体简化访问
**归档:** [链接](../../archive/feature_20260329_F004_app-refactor/)
**归档日期:** 2026-04-27

### feature_20260329_F003_compact-thread-migration
**摘要:** /compact 执行后创建新 Thread 保留旧历史
**关键决策:**
- 新建 Thread：compact 完成后 open_new_thread()，旧 Thread 保留在 SQLite 中
- 新 Thread 开头：插入摘要 System 消息，标记"此对话从历史压缩而来"
- Relay 同步：CompactDone 事件通知 Web 前端切换到新 Thread
- view_messages 重建：新 Thread 从空消息开始
**归档:** [链接](../../archive/feature_20260329_F003_compact-thread-migration/)
**归档日期:** 2026-04-27

### feature_20260329_F003_ui-display-fixes
**摘要:** 修复空消息欢迎页、长文本截断、子 Agent 空状态显示
**关键决策:**
- 空消息时不再显示空白聊天区，改为显示欢迎提示或品牌内容
- 长文本消息截断策略优化，避免单条消息占据整个可见区域
- 子 Agent 执行中无输出时显示"正在执行..."占位，而非空白
**归档:** [链接](../../archive/feature_20260329_F003_ui-display-fixes/)
**归档日期:** 2026-04-27

### feature_20260329_F001_tui-welcome-card
**摘要:** 空消息时显示品牌 ASCII Art Logo + 功能亮点
**关键决策:**
- 渲染条件：view_messages.is_empty() 时显示，发送消息后自动消失
- 内容：ASCII Art Logo + 功能亮点列表 + 命令提示（/help /model /history 等）
- 窄屏降级：宽度不足时隐藏 ASCII Art，仅显示文字标题
- 实现方式：独立渲染函数，作为 main_ui.rs 中的一个渲染分支
**归档:** [链接](../../archive/feature_20260329_F001_tui-welcome-card/)
**归档日期:** 2026-04-27

---

## 相关 Feature
- → [agent.md#20260322_F001_agent-storage-refactor](./agent.md#20260322_F001_agent-storage-refactor) — SQLite 持久化，TUI 消息渲染依赖此存储
- → [langfuse.md#feature_20260324_F001_langfuse-tui-monitoring](./langfuse.md#feature_20260324_F001_langfuse-tui-monitoring) — Langfuse 追踪集成在 TUI 的 app/agent.rs
- → [agent.md#feature_20260328_F001_ask-user-question-align](./agent.md#feature_20260328_F001_ask-user-question-align) — AskUser 弹窗展示更新（header + description），TUI 弹窗同步更新
