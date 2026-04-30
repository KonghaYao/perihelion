# 项目全局 Spec 索引

![全局领域拓扑](./images/01-domain-topology.png)

## 项目概况
→ [overview.md](./overview.md) — 项目概述
→ [architecture.md](./architecture.md) — 架构全景
→ [features.md](./features.md) — 已有功能清单
→ [constraints.md](./constraints.md) — 架构约束

## 已归档 Feature

| Feature ID | 摘要 | 领域 | 归档日期 |
|-----------|------|------|----------|
| [feature_20260430_F003_replace-grep-with-ripgrep](../archive/feature_20260430_F003_replace-grep-with-ripgrep/) | 用 grep+grep-regex crate 替换外部 rg 进程调用 | file-search | 2026-04-30 |
| [feature_20260430_F002_reconcile-on-done-interrupted](../archive/feature_20260430_F002_reconcile-on-done-interrupted/) | Done/Interrupted 事件触发尾部重建确保流式与恢复路径一致 | message-pipeline | 2026-04-30 |
| [feature_20260430_F001_system-prompt-restructure](../archive/feature_20260430_F001_system-prompt-restructure/) | 系统提示词拆分为独立段落文件并支持 Feature 条件注入 | system-prompt | 2026-04-30 |
| [feature_20260429_F002_mouse-text-selection](../archive/feature_20260429_F002_mouse-text-selection/) | TUI 鼠标拖拽选中文本并复制到系统剪贴板 | mouse-selection | 2026-04-30 |
| [feature_20260429_F001_syntect-codeblock-highlight](../archive/feature_20260429_F001_syntect-codeblock-highlight/) | 使用 syntect 为 Markdown 多行代码块添加语法高亮 | code-highlight | 2026-04-30 |
| [feature_20260429_F001_skill-slash-trigger](../archive/feature_20260429_F001_skill-slash-trigger/) | Skills 触发键从 # 统一到 / 前缀 | skill-trigger | 2026-04-30 |
| [feature_20260428_F002_message-pipeline-unify](../archive/feature_20260428_F002_message-pipeline-unify/) | 统一流式与历史恢复的消息显示管线 | message-pipeline | 2026-04-30 |
| [feature_20260428_F001_llm-retry](../archive/feature_20260428_F001_llm-retry/) | LLM 暂时性错误自动重试（指数退避+抖动） | llm-retry | 2026-04-30 |
| [feature_20260428_F001_compact-redesign](../archive/feature_20260428_F001_compact-redesign/) | 全面增强 Micro/Full Compact 策略与压缩后重新注入 | compact | 2026-04-30 |
| [feature_20260427_F004_token-tracking-auto-compact](../archive/feature_20260427_F004_token-tracking-auto-compact/) | Token 累积追踪与上下文窗口感知的自动压缩机制 | token-tracking | 2026-04-30 |
| [feature_20260427_F003_model-config-refactor](../archive/feature_20260427_F003_model-config-refactor/) | Provider 自包含三级别模型名，/login 与 /model 职责分离 | model-config | 2026-04-30 |
| [feature_20260427_F002_permission-mode](../archive/feature_20260427_F002_permission-mode/) | 支持 5 级权限模式，Shift+Tab 循环切换 | hitl-permissions | 2026-04-30 |
| [feature_20260427_F001_relay-removal](../archive/feature_20260427_F001_relay-removal/) | 完整删除废弃的 Relay Server 远程控制功能 | code-architecture | 2026-04-30 |
| [feature_20260427_F001_ratatui-widget-lib](../archive/feature_20260427_F001_ratatui-widget-lib/) | 抽取 TUI 重复 UI 代码为独立可复用 ratatui widget crate | tui-widgets | 2026-04-30 |
| [feature_20260427_F001_claude-code-info-display](../archive/feature_20260427_F001_claude-code-info-display/) | 对标 Claude Code 新增 Spinner、工具调用、消息块三个 widget | tui-widgets | 2026-04-30 |
| [20260408_F001_askuser-dialog-height](../archive/feature_20260408_F001_askuser-dialog-height/) | AskUser 弹窗高度计算修复，滚动可见高度动态化 | tui | 2026-04-27 |
| [20260331_F001_history-workspace-tag](../archive/feature_20260331_F001_history-workspace-tag/) | /history 面板按 cwd 过滤只显示当前工作区对话 | tui | 2026-04-27 |
| [20260330_F005_tui-setup-wizard](../archive/feature_20260330_F005_tui-setup-wizard/) | 首次启动三步引导（Provider → API Key → Model Alias） | tui | 2026-04-27 |
| [20260330_F004_langfuse-client](../archive/feature_20260330_F004_langfuse-client/) | workspace 内 langfuse-client crate 替代 langfuse-ergonomic | langfuse | 2026-04-27 |
| [20260330_F003_cron-loop-command](../archive/feature_20260330_F003_cron-loop-command/) | /loop /cron 定时任务系统，cron 表达式注册管理 | agent | 2026-04-27 |
| [20260330_F002_tui-color-refresh](../archive/feature_20260330_F002_tui-color-refresh/) | 配色系统 v1.1 降噪，橙色聚焦交互，工具名三级分层 | tui | 2026-04-27 |
| [20260330_F001_sticky-human-message-header](../archive/feature_20260330_F001_sticky-human-message-header/) | 聊天区顶部固定最后一条 Human 消息摘要 | tui | 2026-04-27 |
| [20260329_F005_legacy-cleanup](../archive/feature_20260329_F005_legacy-cleanup/) | Agent trait 层级清理与废弃 API 移除 | agent | 2026-04-27 |
| [20260329_F004_app-refactor](../archive/feature_20260329_F004_app-refactor/) | App 结构体拆分为 AppCore/AgentComm/RelayState/LangfuseState | tui | 2026-04-27 |
| [20260329_F003_compact-thread-migration](../archive/feature_20260329_F003_compact-thread-migration/) | /compact 执行后创建新 Thread 保留旧历史 | tui | 2026-04-27 |
| [20260329_F003_ui-display-fixes](../archive/feature_20260329_F003_ui-display-fixes/) | 修复空消息欢迎页、长文本截断、子 Agent 空状态显示 | tui | 2026-04-27 |
| [20260329_F002_subagent-model-switch](../archive/feature_20260329_F002_subagent-model-switch/) | 子 Agent 支持独立模型配置，LLM Factory 签名升级 | agent | 2026-04-27 |
| [20260329_F001_tui-welcome-card](../archive/feature_20260329_F001_tui-welcome-card/) | 空消息时显示品牌 ASCII Art Logo + 功能亮点 | tui | 2026-04-27 |
| [20260328_F004_settings-env-injection](../archive/feature_20260328_F004_settings-env-injection/) | settings.json env 字段替代 .env 注入环境变量 | tui | 2026-03-29 |
| [20260328_F003_test-coverage-improvement](../archive/feature_20260328_F003_test-coverage-improvement/) | 四高风险区域补充 55+ 单元测试提升覆盖率 | tui | 2026-03-29 |
| [20260328_F002_relay-multi-user-isolation](../archive/feature_20260328_F002_relay-multi-user-isolation/) | UserNamespace 分层实现多用户完全隔离 | relay-server | 2026-03-29 |
| [20260328_H2_thread-store](../archive/feature_20260328_H2_thread-store/) | （无设计文档）| — | 2026-03-28 |
| [20260328_F001_skill-preload-on-send](../archive/feature_20260328_F001_skill-preload-on-send/) | TUI 发送含 #skill-name 消息时自动全文预加载对应 skill | tui | 2026-03-28 |
| [20260328_F001_ask-user-question-align](../archive/feature_20260328_F001_ask-user-question-align/) | ask_user 工具全面对齐 Claude AskUserQuestion 接口规范 | agent | 2026-03-28 |
| [20260327_M3_system-prompt](../archive/feature_20260327_M3_system-prompt/) | with_system_prompt() 消除 PrependSystemMiddleware 注册顺序约束 | agent | 2026-03-28 |
| [20260327_H3_interaction-unify](../archive/feature_20260327_H3_interaction-unify/) | 提取 UserInteractionBroker trait 统一 HITL 和 AskUser 交互机制 | agent | 2026-03-28 |
| [20260327_H1_relay-decouple](../archive/feature_20260327_H1_relay-decouple/) | （无设计文档）| relay-server | 2026-03-28 |
| [20260327_F002_relay-command-sync](../archive/feature_20260327_F002_relay-command-sync/) | Web 端发 /compact 命令及 Agent 侧 thread 状态双向同步 | relay-server | 2026-03-28 |
| [20260327_F002_fix-agent-history-storage](../archive/feature_20260327_F002_fix-agent-history-storage/) | （无设计文档）| agent | 2026-03-28 |
| [20260327_F001_web-ask-user-interrupt](../archive/feature_20260327_F001_web-ask-user-interrupt/) | 补全 AskUser 协议字段并支持 Web 端中断 Agent 运行 | relay-server | 2026-03-28 |
| [20260327_F001_relay-mobile-layout](../archive/feature_20260327_F001_relay-mobile-layout/) | Relay Web 前端移动端完整适配含汉堡侧边栏和面板 Tab 切换 | relay-server | 2026-03-28 |
| [20260327_F001_preact-no-bundle-migration](../archive/feature_20260327_F001_preact-no-bundle-migration/) | 前端从命令式 DOM 迁移到 Preact+Signals+htm 声明式组件体系 | relay-server | 2026-03-28 |
| [20260327_F001_frontend-message-id-dedup](../archive/feature_20260327_F001_frontend-message-id-dedup/) | 前端消息基于 UUIDv7 ID 实现 upsert 去重防重复显示 | relay-server | 2026-03-28 |
| [20260326_F010_relay-loading-state-sync](../archive/feature_20260326_F010_relay-loading-state-sync/) | Agent 执行状态同步到 Web 前端显示「正在思考…」 | relay-server | 2026-03-27 |
| [20260326_F009_relay-message-id-propagation](../archive/feature_20260326_F009_relay-message-id-propagation/) | TextChunk/ToolStart/ToolEnd 携带 message_id 支持 update-in-place | agent | 2026-03-27 |
| [20260326_F008_statusbar-msgcount-relay-flag](../archive/feature_20260326_F008_statusbar-msgcount-relay-flag/) | 状态栏消息计数，禁止 relay 隐式自动连接 | tui | 2026-03-27 |
| [20260326_F007_relay-server-logging](../archive/feature_20260326_F007_relay-server-logging/) | 补充 Relay Server 连接/认证失败/消息转发日志 | relay-server | 2026-03-27 |
| [20260326_F006_message-uuid-v7](../archive/feature_20260326_F006_message-uuid-v7/) | BaseMessage 四变体增加 UUID v7 全局唯一 ID | agent | 2026-03-27 |
| [20260326_F005_subagent-skill-preload](../archive/feature_20260326_F005_subagent-skill-preload/) | Agent 定义声明 skills 字段，启动时全文预加载 | agent | 2026-03-27 |
| [20260326_F004_remote-control-panel](../archive/feature_20260326_F004_remote-control-panel/) | /relay 命令面板：TUI 内配置持久化远程控制参数 | tui | 2026-03-27 |
| [20260326_F001_subagent-message-hierarchy](../archive/feature_20260326_F001_subagent-message-hierarchy/) | SubAgent 执行消息分层为可折叠块，滑动窗口展示 | tui | 2026-03-27 |
| [20260326_F001_specialized-agents](../archive/feature_20260326_F001_specialized-agents/) | 预置 Explorer + WebResearcher 声明式专用 Agent | agent | 2026-03-27 |
| [20260326_F001_relay-frontend-mobile-redesign](../archive/feature_20260326_F001_relay-frontend-mobile-redesign/) | Relay 前端移动端重设计（无设计文档） | relay-server | 2026-03-27 |
| [20260325_F004_subagent-langfuse-nesting](../archive/feature_20260325_F004_subagent-langfuse-nesting/) | 子 Agent Langfuse 嵌套追踪迭代探索（无设计文档） | langfuse | 2026-03-27 |
| [20260325_F003_langfuse-observation-types](../archive/feature_20260325_F003_langfuse-observation-types/) | 规范化 Langfuse 观测层级与类型命名 | langfuse | 2026-03-27 |
| [20260325_F002_large-file-refactor](../archive/feature_20260325_F002_large-file-refactor/) | app/mod.rs 和 main_ui.rs 大文件拆分为多子文件 | tui | 2026-03-27 |
| [20260325_F001_tui-langfuse-session](../archive/feature_20260325_F001_tui-langfuse-session/) | Thread 级 LangfuseSession 使多轮消息归属同一 Session | langfuse | 2026-03-27 |
| [20260325_F001_subagent-middleware-injection](../archive/feature_20260325_F001_subagent-middleware-injection/) | 子 Agent 补全三个缺失中间件使上下文一致 | agent | 2026-03-27 |
| [20260325_F001_langfuse-subagent-nesting](../archive/feature_20260325_F001_langfuse-subagent-nesting/) | Langfuse 子 Agent 嵌套追踪迭代探索（无设计文档） | langfuse | 2026-03-27 |
| [20260325_F001_langfuse-nested-subagent-trace](../archive/feature_20260325_F001_langfuse-nested-subagent-trace/) | Langfuse 嵌套子 Agent 追踪迭代探索（无设计文档） | langfuse | 2026-03-27 |
| [20260324_F002_relay-server-ui-redesign](../archive/feature_20260324_F002_relay-server-ui-redesign/) | Relay Web 前端重设计为 Claude 风格多分屏界面 | relay-server | 2026-03-27 |
| [20260324_F001_ratatui-markdown-renderer](../archive/feature_20260324_F001_ratatui-markdown-renderer/) | pulldown-cmark 替代 tui-markdown，自制 ratatui 渲染器 | tui | 2026-03-27 |
| [20260324_F001_rust-langfuse-client](../archive/feature_20260324_F001_rust-langfuse-client/) | Langfuse 客户端早期探索（无设计文档） | langfuse | 2026-03-27 |
| [20260324_F001_langfuse-tui-monitoring](../archive/feature_20260324_F001_langfuse-tui-monitoring/) | TUI 层接入 Langfuse 全链路追踪 | langfuse | 2026-03-27 |
| [20260324_F001_tui-clipboard-image-paste](../archive/feature_20260324_F001_tui-clipboard-image-paste/) | Ctrl+V 粘贴剪贴板图片作为多模态消息发送 | tui | 2026-03-24 |
| [20260324_F001_compact-context-command](../archive/feature_20260324_F001_compact-context-command/) | /compact 指令调用 LLM 将对话历史压缩为结构化摘要 | tui | 2026-03-24 |
| [20260323_F006_ws-event-sync](../archive/feature_20260323_F006_ws-event-sync/) | WebSocket 事件扁平化+seq序列号+会话 Sync 同步 | relay-server | 2026-03-24 |
| [20260323_F004_remote-control-access](../archive/feature_20260323_F004_remote-control-access/) | Relay Server + Web 前端实现远程访问控制本地 Agent | relay-server | 2026-03-24 |
| [20260323_F005_tui-bug-fixes](../archive/feature_20260323_F005_tui-bug-fixes/) | 修复弹窗滚动/粘贴换行/loading 输入锁死三个 TUI bug | tui | 2026-03-24 |
| [20260323_F001_model-alias-provider-mapping](../archive/feature_20260323_F001_model-alias-provider-mapping/) | Opus/Sonnet/Haiku 三级别名映射，支持 /model <alias> 快捷切换 | tui | 2026-03-24 |
| [20260323_F003_tui-status-panel](../archive/feature_20260323_F003_tui-status-panel/) | TODO 状态固定面板、工具调用颜色分层、路径参数缩短 | tui | 2026-03-24 |
| [20260323_F002_tui-headless-mode](../archive/feature_20260323_F002_tui-headless-mode/) | Headless 测试模式：TestBackend + 渲染线程零 sleep 同步 | tui | 2026-03-24 |
| [20260323_F001_tui-render-perf](../archive/feature_20260323_F001_tui-render-perf/) | 双线程渲染架构：独立渲染线程 + 按需重绘，消除消息多时卡顿 | tui | 2026-03-24 |
| [20260322_F002_data-pipeline-unification](../archive/feature_20260322_F002_data-pipeline-unification/) | 实时流式与历史恢复统一工具调用参数显示，含 tool_call_id 匹配 | tui | 2026-03-24 |
| [20260322_F001_message-render-refactor](../archive/feature_20260322_F001_message-render-refactor/) | MessageViewModel 中间层重构，tui-markdown 渲染，工具折叠 | tui | 2026-03-24 |
| [20260322_F001_agent-storage-refactor](../archive/feature_20260322_F001_agent-storage-refactor/) | SQLite WAL 持久化替代 JSONL，MessageAdapter 双向转换 | agent | 2026-03-24 |
| [20260321_F001_subagents-execution](../archive/feature_20260321_F001_subagents-execution/) | launch_agent 工具支持子 Agent 委派，防递归，工具过滤 | agent | 2026-03-24 |

## 领域索引

- [agent](./domains/agent.md) — Agent 核心（ReAct 执行器、消息系统、工具抽象、持久化）— 13 features
- [tui](./domains/tui.md) — TUI 界面（渲染、交互、命令、面板）— 26 features
- [relay-server](./domains/relay-server.md) — Relay Server（WebSocket 中继、远程控制）— 12 features
- [langfuse](./domains/langfuse.md) — 可观测性（Langfuse 全链路追踪、Session/Trace/Generation/Tool 层级）— 8 features
- [model-config](./domains/model-config.md) — 模型配置（Provider 自包含模型名、/login 与 /model 分离）— 1 feature
- [token-tracking](./domains/token-tracking.md) — Token 追踪与压缩（累积追踪、上下文窗口感知、自动压缩）— 1 feature
- [llm-retry](./domains/llm-retry.md) — LLM 重试（暂时性错误自动重试、指数退避）— 1 feature
- [message-pipeline](./domains/message-pipeline.md) — 消息管线（统一流式与历史恢复、PipelineAction）— 2 features
- [skill-trigger](./domains/skill-trigger.md) — Skills 触发（从 # 统一到 / 前缀）— 1 feature
- [code-highlight](./domains/code-highlight.md) — 代码高亮（syntect 语法高亮）— 1 feature
- [mouse-selection](./domains/mouse-selection.md) — 鼠标选区（拖拽选中文本、剪贴板复制）— 1 feature
- [system-prompt](./domains/system-prompt.md) — 系统提示词（段落化、Feature 条件注入）— 1 feature
- [file-search](./domains/file-search.md) — 文件搜索（grep crate 进程内搜索）— 1 feature
- [hitl-permissions](./domains/hitl-permissions.md) — HITL 权限（5 级权限模式）— 1 feature
- [tui-widgets](./domains/tui-widgets.md) — TUI 组件（Spinner/ToolCall/MessageBlock widget + widget 库抽取）— 2 features
- [compact](./domains/compact.md) — 上下文压缩增强（Micro/Full Compact 策略）— 1 feature
- [code-architecture](./domains/code-architecture.md) — 代码架构（Relay 移除等结构性变更）— 1 feature

---
*最后更新: 2026-04-30 — 由 15 个 feature 归档批量更新*
