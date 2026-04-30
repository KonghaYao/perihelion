# Design Review Progress

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
