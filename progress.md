# Design Review Progress

## 2026-04-30 第21轮

Cron 缓冲消息合并缺陷：多个 cron 触发在 agent 执行期间被缓冲到 pending_messages，但 Done/Error 处理器用 \n\n 连接后作为单条消息提交——独立的 cron 任务提示被合并为语义混淆的组合请求。改为 flush_pending_messages 每次只提交第一条、保留其余至后续 Done 周期逐一发送，保证各 cron 任务独立到达 LLM。833 测试全通过。

## 2026-04-30 第20轮

RetryableLLM 逻辑清理：generate_reasoning 方法存在不可达死代码（Err(last_error.unwrap())在第106行），循环结构 0..=max_retries 配合 attempt < max_retries 条件使最终迭代必走 Err(e) => return 分支。将循环重构为 0..max_retries 重试 + 末尾最终尝试，消除死代码和潜在 panic。BashTool 超时参数无下限保护——timeout_secs=0 会导致 Duration::from_secs(0) 立即超时命令永不执行，改为 clamp(1, 300)。新增4个测试（零超时被clamp、300上限、RetryableLLM最终尝试不重试、max_retries=0单次调用）。833测试通过。

## 2026-04-30 第19轮

ContextBudget 事件链路审查：发现 AgentEvent::ContextWarning 事件定义完整但从未被 executor 发出——executor 的上下文监控仅产 tracing 日志（用户不可见），TUI 的 map_executor_event 也将其映射为 return None。为 executor 的 ContextBudget 路径和回退路径新增 ContextWarning 事件发出（仅当阈值达标时），TUI 新增 ContextWarning 变体并映射到 auto-compact 触发逻辑。新增 3 个测试覆盖 budget/回退/低用量三种场景。829 测试通过。

## 2026-04-30 第18轮

LLM 适配层审查：发现 BaseModelReactLLM::context_window() 用字符串前缀硬编码上下文窗口（claude→200K/deepseek→128K/gpt-4o→128K），导致 GPT-3.5-turbo（真实 16K）等模型返回错误的 200K 默认值。为 BaseModel trait 新增 context_window() 默认 200K，ChatOpenAI 覆盖为精确模型名推断（gpt-4→128K/o1→200K/gpt3.5→16K/deepseek→128K），适配器改为委托 model.context_window()。新增 7 个测试验证各模型窗口值。826 测试通过。

## 2026-04-30 第17轮

Anthropic Prompt Caching 审查：发现 apply_cache_to_messages 将 cache_control 标记放在最后一条 user 消息上，但 ReAct 循环中该消息每轮变化导致缓存失效命中率为零。改为在第一条 user 消息上加 cache_control（稳定边界），与 system 缓存共同构成稳定缓存段，后续轮次持续命中。新增 5 个测试覆盖边界行为（首条/跳过assistant/多block/空block/无user消息）。823 测试通过。

## 2026-04-30 第16轮

Token 追踪模块审查：发现 ContextBudget 虽在 token.rs 完整定义了 auto_compact/warning 阈值计算逻辑（含测试），但 executor 从未使用——而是硬编码 80% 作为上下文警告阈值，且与 CompactConfig 的 70% 默认 warning_threshold 不一致，定义层与执行层脱节。为 ReActAgent 新增 context_budget 字段和 with_context_budget() builder 方法，execute 循环改为优先使用 ContextBudget::should_warn()，无配置时回退硬编码逻辑。新增 2 个测试。818 测试通过。

## 2026-04-30 第15轮

SubAgent 模块审查优化：发现 invoke 方法中 agent 定义文件被 parse_agent_file 解析后又通过 load_overrides 重新读取解析同一文件，造成冗余 I/O。新增 overrides_from_agent_def 从已解析数据直接提取 AgentOverrides 消除二重解析。同时发现子 agent execute 调用始终传入 None 取消令牌——用户 Ctrl+C 无法中断子 agent 执行。为 SubAgentTool/SubAgentMiddleware 新增 cancel 令牌传递链路，TUI 注入父 agent 取消令牌。新增 4 个测试覆盖 overrides 提取和取消中断。816 测试全部通过。

## 2026-04-30 第14轮

业务逻辑层审查优化：发现 HITL 中间件 process_batch 批量审批方法已定义但 ReActExecutor 从未调用——每个敏感工具单独触发弹窗打断用户。新增 Middleware trait before_tools_batch 钩子（默认逐条回退），MiddlewareChain 新增 run_before_tools_batch 链式批量执行，HITL 覆盖该钩子调用已有 process_batch。Executor 阶段一从逐个 before_tool 改为批量调用。多个敏感工具现在合并为一次审批弹窗。新增 3 个测试覆盖混合审批、批量等价性、端到端执行。812 测试全部通过。

## 2026-04-29 第1轮

修复4个UX问题：thread_browser和login_panel的'd'键删除功能缺失（帮助栏提示但未实现）、Welcome Card缺少全局快捷键提示、所有配置保存点从静默忽略改为检查错误并显示反馈。772个测试全部通过。

## 2026-04-29 第2轮

修复2个UX问题：cron面板'd'键删除未连接（同第1轮同类问题），thread_browser删除后增加反馈消息显示被删对话标题。772个测试全部通过。

## 2026-04-29 第4轮

修复3个UX问题：AskUser弹窗添加底部快捷键提示行（Tab/Space/Enter），Model面板帮助栏Space从"切换"改为"Thinking开关"避免误导，Thread Browser标题栏精简防止窄屏截断。775个测试全部通过。

## 2026-04-29 第3轮

全面排查并修复单字母快捷键违规：HITL弹窗移除y/n/t改为Space+Enter；Thread/Cron删除改Ctrl+D；Login编辑/新建/删除改Enter/Ctrl+N/Ctrl+D。同步更新所有面板提示文字和状态栏。新增3个headless测试验证合规性。241测试通过。

## 2026-04-29 第4轮

修复面板空状态/缺省引导：Agent面板空列表时显示.claude/agents/添加引导+补全↑↓导航提示；Model面板无Provider时从"未选择"改为"未配置"并加/login引导行。新增3个headless测试覆盖空状态引导。244测试通过。

## 2026-04-29 第5轮

补全面板操作反馈与状态栏提示：Cron面板添加空列表/loop引导和删除反馈消息；Login编辑模式补Ctrl+V粘贴提示，保存失败时显示错误；Login删除Provider后显示确认消息；状态栏为Cron/Login/Model面板补充快捷键提示。新增2个headless测试。246测试通过。

## 2026-04-29 第6轮

优化首次体验与工具状态展示：Welcome Card未配置Provider时显示"请输入/login配置API Key"首次引导；命令栏补/agents并精简快捷键（移除Ctrl+V/Paste冗余项）；工具调用Running状态添加"运行中…"文字标签。新增1个headless测试。247测试通过。

## 2026-04-29 第7轮

增强信息辨识度：Thread Browser当前打开的对话添加✓标识+强调色高亮；ToolCallGroup折叠状态添加▶展开提示符号；/help命令末尾补Skills使用说明（含数量提示和添加方式引导）。247测试通过。

## 2026-04-30 第8轮

提升功能可发现性：输入框添加"输入消息… (Alt+Enter 换行)"占位提示解决新用户首屏困惑；Welcome Card和状态栏补充Alt+Enter换行快捷键提示；命令前缀匹配多个时显示候选列表（如"/c匹配/clear, /compact, /cron"）取代通用错误；状态栏空闲时显示/命令和Alt+Enter快捷键提示。新增3个headless测试。250测试通过。

## 2026-04-30 第9轮

改善错误消息可操作性和运行状态感知：未配置Provider从空error改为"请输入/login配置"引导消息；channel断开从英文改为中文"请重试发送消息"；状态栏loading时显示⏱任务运行时长。784测试通过。

## 2026-04-30 第10轮

系统消息颜色自动分级：SystemNote按内容检测错误（❌/失败）用ERROR红色、警告（⚠/中断）用WARNING橙色，普通保持SAGE绿色，解决所有系统消息视觉权重相同的问题。/compact命令启动时添加"正在压缩上下文…"即时反馈，用户不再疑惑操作是否开始。新增2个测试。252测试通过。

恢复历史对话添加确认反馈：新增open_thread_with_feedback方法，加载对话后显示"已加载「标题」"确认消息，让用户明确知道对话已成功切换。784测试通过。

## 2026-04-30 第11轮

提升消息可读性：ToolBlock错误结果从MUTED灰色改为ERROR红色高亮（含边框），让用户一眼区分成功和失败；/help末尾补全局快捷键提示行（Shift+Tab权限模式、Esc退出、Ctrl+C中断）。784测试通过。

## 2026-04-30 第12轮

优化compact操作体验：/compact命令在loading时阻止重复触发；start_compact设置spinner为"压缩上下文"文字提示；micro-compact消息从"Micro-compact: 清除了"改为中文"自动清理：释放了"。786测试通过。

## 2026-04-30 第13轮

清理Tips虚假功能引用：移除6条引用不存在命令的提示（/rename, /config, /todo, /feedback, /color, /export）和2条未实现的快捷键（双按Esc, Ctrl+O），用户尝试这些会得到"未知命令"错误；新增Alt+Enter换行提示和回归测试防止未来引入不存在命令。252测试通过。
