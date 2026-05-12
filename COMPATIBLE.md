# Claude Code vs Peri 兼容性对比

> 最后更新：2026-05-10

## 全景对比表

| # | 维度 | 特性 | Claude Code | Peri | 状态 | 已验证 |
|---|------|------|------------|------------|------|--------|
| | **基础** | 语言/运行时 | TypeScript / Bun | Rust / Tokio | N/A（重写） | |
| | **核心架构** | Agent 循环 | ReAct（query.ts / QueryEngine.ts） | ReAct（rust-create-agent） | ✅ | |
| | | 最大迭代 | 可配置 | 50（可配置） | ✅ | |
| | | 流式输出 | Anthropic SSE / OpenAI stream | SSE streaming | ✅ | |
| | | Prompt Caching | Anthropic prompt-caching-2024-07-31（首条用户消息 cache_control） | 同左（Anthropic 默认启用） | ✅ | |
| | | 重试 | 指数退避 + jitter（max 5 次，429/503/网络错误可重试） | RetryableLLM（指数退避 + 25% jitter，max 5 次） | ✅ | |
| | | 重试可配 | -- | RetryConfig（max_retries/base_delay/max_delay） | 🟢 | |
| | | 中间件系统 | 无显式中间件（hooks/injectors 分散） | MiddlewareChain（6 hooks） | 🟢 | |
| | | 事件系统 | React state / EventEmitter | AgentEvent（11 变体）+ RenderEvent | 🔄 | |
| | | 取消机制 | AbortController | CancellationToken（tokio_util） | 🔄 | |
| | **模型** | LLM Provider | Anthropic, OpenAI, Gemini, Grok, Bedrock, Vertex, Foundry | Anthropic, OpenAI | ⚠️ | |
| | | Provider 配置 | 7 providers | Anthropic + OpenAI + Custom（环境变量 fallback） | ⚠️ | |
| | | 环境变量 Fallback | -- | ANTHROPIC_API_KEY / OPENAI_API_KEY / OPENAI_BASE_URL / MODEL_PROVIDER / *_MODEL 等 8 个 | 🟢 | |
| | | Provider 自动检测 | -- | 环境变量自动推断 provider_type | 🟢 | |
| | | 模型别名 | provider 配置（opus/sonnet/haiku 快捷切换） | ProviderModels + Alt+M 快捷切换 | ✅ | |
| | | /model 命令 | ModelPicker（交互式模型选择） | /model / /model alias | ✅ | |
| | | Thinking/Reasoning | Anthropic thinking, OpenAI reasoning_effort | ThinkingConfig（budget_tokens + effort） | ✅ | |
| | | Thinking Budget 下限 | -- | budget_tokens 最小 1024（enforced） | 🟢 | |
| | | Thinking Effort 级别 | -- | low / medium / high | ✅ | |
| | | Prompt Cache Control | 首条用户消息 cache_control | 首条用户消息 cache_control | ✅ | |
| | | /effort 命令 | effort（推理力度 low/medium/high/xhigh/max/auto） | /effort（推理力度 low/medium/high） | ✅ | |
| | | 1M Context 切换 | ModelPicker 副标题 Space 切换 | -- | ❌ | |
| | | Token Budget | tokenBudget（token 目标 +500k 自动持续工作） | -- | ❌ | |
| | | Prompt Cache Break Detection | PROMPT_CACHE_BREAK_DETECTION | -- | ❌ | |
| | | **消息** | 消息类型 | Human / Ai / System / Tool | Human / Ai / System / Tool | ✅ | |
| | | 内容块 | Text / Image / ToolUse / ToolResult / Thinking | Text / Image / Document / ToolUse / ToolResult / Reasoning / Unknown | 🟢 | |
| | **核心工具** | 文件系统（Read/Write/Edit/Glob/Grep/folder_operations） | FileRead/Write/Edit + Glob/Grep | 同名工具 + folder_operations 独立 | ✅ | |
| | | Shell（Bash） | BashTool + PowerShellTool | Bash（缺 PowerShell） | ⚠️（缺 PowerShell） | |
| | | Web（WebFetch/WebSearch） | WebFetchTool + WebSearchTool | WebFetch + WebSearch | ✅ | |
| | | Jupyter | NotebookEditTool | -- | ❌ | |
| | | 辅助（Snip/LSP/Sleep/CtxInspect） | SnipTool + LSPTool + SleepTool + CtxInspectTool | -- | ❌ | |
| | **Agent 工具** | Agent (SubAgent) | AgentTool | Agent | ✅ | |
| | | AskUserQuestion | AskUserQuestionTool | AskUserQuestion | ✅ | |
| | | TodoWrite | TodoWriteTool | TodoWrite | ✅ | |
| | | TaskCreate/Update/List/Get/Stop/Output | TaskCreateTool 等 6 个 | -- | ❌（TodoWrite 部分覆盖） | |
| | | PlanMode | EnterPlanMode / ExitPlanMode / VerifyPlan | -- | ❌ | |
| | **调度工具** | CronRegister | ScheduleCronTool (CronCreate) | cron_register | ✅ | |
| | | CronList | CronListTool | cron_list | ✅ | |
| | | CronDelete | CronDeleteTool | cron_remove | ✅ | |
| | | LocalMemoryRecall | LocalMemoryRecallTool | -- | ❌ | |
| | | Config | ConfigTool | -- | ❌（通过 /config 命令） | |
| | **MCP 工具** | MCP Tool Invoke | MCPTool | mcp__{server}__{tool}（ExecuteExtraTool） | ✅ | |
| | | Read Resource | ReadMcpResourceTool | mcp__read_resource | ✅ | |
| | | List Resources | ListMcpResourcesTool | -- | ❌ | |
| | | MCP Auth | McpAuthTool | OAuth flow（内建） | 🔄 | |
| | **延迟加载** | SearchExtraTools | -- | SearchExtraTool（元工具） | 🟢 | |
| | | ExecuteExtraTool | -- | ExecuteExtraTool（代理执行） | 🟢 | |
| | | Skill Discovery | DiscoverSkillsTool | SkillsMiddleware（自动注入） | 🔄 | |
| | | SkillTool | SkillTool | SkillPreloadMiddleware（#skill-name） | 🔄 | |
| | **CC 独有工具** | REPLTool | ant-only 隔离 REPL 子会话 | -- | ❌ | |
| | | MonitorTool | 后台命令监控（类似 watch） | -- | ❌ | |
| | | RemoteTriggerTool | 远程触发 | -- | ❌ | |
| | | SendUserFileTool | 文件传输给用户 | -- | ❌ | |
| | | SendMessageTool | 发送消息给 peer | -- | ❌ | |
| | | ListPeersTool | 列出 LAN/pipe peers | -- | ❌ | |
| | | PushNotificationTool | 桌面推送通知 | -- | ❌ | |
| | | SubscribePRTool | GitHub PR webhook 订阅 | -- | ❌ | |
| | | WorkflowTool | 执行 `.claude/workflows/` 脚本 | -- | ❌ | |
| | | CtxInspectTool | 上下文窗口检查 | -- | ❌ | |
| | | TerminalCaptureTool | 终端屏幕截图 | -- | ❌ | |
| | | WebBrowserTool | 终端内浏览器 | -- | ❌ | |
| | | TungstenTool | Tungsten 服务集成 | -- | ❌ | |
| | | ReviewArtifactTool | 审查产物 | -- | ❌ | |
| | | VaultHttpFetchTool | Vault 获取（带 scrubbing） | -- | ❌ | |
| | **MCP 集成** | 配置加载 | 全局 + 项目级合并 | `~/.peri/settings.json` + `.mcp.json` | ✅ | |
| | | 环境变量展开 | `${VAR}` | `${VAR}` | ✅ | |
| | | stdio 传输 | 支持 | 支持 | ✅ | |
| | | StreamableHTTP | 支持 | 支持 | ✅ | |
| | | OAuth 2.0 | 支持 | 支持 | ✅ | |
| | | 连接池 | Client 管理 | McpClientPool（lazy init） | ✅ | |
| | | 工具命名 | `mcp__{server}__{tool}` | `mcp__{server}__{tool}` | ✅ | |
| | | 插件 MCP 命名空间 | `{plugin}__{server}` | `{plugin_name}__{server_name}` | ✅ | |
| | | 资源读取 | ReadMcpResource + ListMcpResources | mcp__read_resource | ⚠️（缺 list） | |
| | | 优雅关闭 | 支持 | pool.shutdown() | ✅ | |
| | | 超时 | 可配置 | stdio 10s / HTTP 30s | ✅ | |
| | | 官方 Registry | officialRegistry（MCP 服务器发现） | -- | ❌ | |
| | | MCP Channel Notifications | channelNotification | -- | ❌ | |
| | | MCP Channel Permissions | channelPermissions | -- | ❌ | |
| | | MCP Channel Allowlist | channelAllowlist | -- | ❌ | |
| | | VSCode SDK MCP | vscodeSdkMcp | -- | ❌ | |
| | **SubAgent** | Agent 定义目录 | `.claude/agents/{id}/agent.md` | `.claude/agents/{id}/agent.md` | ✅ | |
| | | 内建 Agent | generalPurpose, plan, explore, verification, claudeCodeGuide | explore, general-purpose, plan, verification | ⚠️（缺 claudeCodeGuide） | |
| | | Fork 模式 | 支持（继承完整上下文） | 支持（继承消息+system+工具） | ✅ | |
| | | 防递归 | fork directive 规则 | fork directive + Agent 排除 | ✅ | |
| | | 工具过滤 | tools + disallowedTools | tools + disallowedTools | ✅ | |
| | | 后台 Agent | 支持（Task 系统） | 支持（max 3 并发） | ✅ | |
| | | 后台通知 | 事件通道 | 独立通知通道 + continuation | ✅ | |
| | | Agent Memory | agentMemory.ts（子 agent 持久记忆） | -- | ❌ | |
| | | 插件 Agent | loadPluginAgents | scan_agents_with_extra_dirs | ✅ | |
| | | 返回值格式 | 结构化工具调用结果 | `[子 agent 执行了 N 个工具调用]\nFinal response` | 🔄 | |
| | **Agent Teams** | Coordinator Mode | coordinatorMode（自动分发任务给多个并行 worker） | -- | ❌ | |
| | | Agent Swarms | swarm/（多 agent 团队协调，in-process / terminal / iTerm2 后端） | -- | ❌ | |
| | | Background Agent (InProcess) | LocalAgentTask（进程内后台 agent） | 支持（max 3 并发） | ✅ | |
| | | Background Agent (Remote) | RemoteAgentTask（远程后台 agent） | -- | ❌ | |
| | | Background Shell Task | LocalShellTask（后台 shell 任务） | -- | ❌ | |
| | | InProcess Teammate | InProcessTeammateTask（进程内队友任务） | -- | ❌ | |
| | | Teammate Mailbox | teammateMailbox（25 个测试覆盖，协议消息检测） | -- | ❌ | |
| | | Background Tasks Dialog | BackgroundTasksDialog + BackgroundAgentSelector（UI 管理） | -- | ❌ | |
| | | Task Framework | task/framework.rs + TaskOutput + diskOutput | -- | ❌ | |
| | | DAG 工作流引擎 | -- | --（已迁移为独立项目 acpx-g） | -- | |
| | **Skills** | 搜索路径 | `~/.claude/skills/` → skillsDir → `./.claude/skills/` | 同左 + 插件 skills | ✅ | |
| | | Skill 格式 | SKILL.md（YAML frontmatter: name, description） | 同左 | ✅ | |
| | | 触发方式 | `/skill-name` 或 LLM 调用 SkillTool | `/skill-name` 或 `#skill-name` 全文注入 | ✅ | |
| | | 内建 Skills | 17 个（stuck, simplify, skillify, dream, remember, verify, loop, batch, cron, debug, chrome 等） | 无内建（仅用户定义） | ❌ | |
| | | Skill 搜索 | TF-IDF 本地搜索 + 远程加载 | -- | ❌ | |
| | | Skill Prefetch | intentNormalize + featureCheck | -- | ❌ | |
| | | Skill Learning | skillLearning/（从使用模式自动生成 skill） | -- | ❌ | |
| | | 插件 Skills | 支持 | with_extra_dirs() | ✅ | |
| | | MCP Skills | mcpSkills（从 MCP 构建 skill） | -- | ❌ | |
| | **HITL** | YOLO 模式 | --dangerously-skip-permissions | YOLO_MODE=true（默认） | ✅ | |
| | | 审批模式 | --approve | -a / --approve | ✅ | |
| | | 权限级别 | 多种模式（ask, auto, plan） | 5 级（Yolo/Ask/Delegate/Auto/Approved） | 🟢 | |
| | | Bash 审批 | BashPermissionRequest | default_requires_approval | ✅ | |
| | | 文件写入审批 | FileWritePermission / FileEditPermission | Write + Edit 需审批 | ✅ | |
| | | MCP 审批 | mcp__ 前缀工具 | mcp__ 前缀工具 | ✅ | |
| | | Web 审批 | WebFetchPermission | WebFetch + WebSearch 需审批 | ✅ | |
| | | Agent 审批 | PermissionRequest | Agent 需审批 | ✅ | |
| | | Notebook 审批 | NotebookEditPermissionRequest | -- | ❌ | |
| | | Computer Use 审批 | ComputerUseApproval | -- | ❌ | |
| | | Monitor 审批 | MonitorPermissionRequest | -- | ❌ | |
| | | Plan Mode 审批 | EnterPlanMode / ExitPlanMode | -- | ❌ | |
| | | 批量审批 | -- | BatchItem / HitlDecision | 🟢 | |
| | | 自动分类 | classifierDecision / LLM 分类 | LlmAutoClassifier | ✅ | |
| | | Bash LLM 分类器 | bashClassifier（LLM 驱动的 bash 安全分类） | -- | ❌ | |
| | | Tree-sitter Bash | bash/（7000+ 行纯 TS AST 解析） | -- | ❌ | |
| | | PowerShell 危险 cmdlet | dangerousCmdlets | -- | ❌ | |
| | | 路径验证 | pathValidation | -- | ❌ | |
| | | 危险命令检测 | dangerousPatterns | -- | ❌ | |
| | | 权限规则解析 | permissionRuleParser（复杂规则系统） | -- | ❌ | |
| | | 拒绝追踪 | denialTracking | -- | ❌ | |
| | | Auto Mode Denials | autoModeDenials（auto 模式拒绝追踪展示） | -- | ❌ | |
| | | 快捷键切换 | Shift+Tab 循环模式 | Shift+Tab 循环 5 级 | ✅ | |
| | | 权限规则语法 | `Tool(specifier)` + glob `*` + deny→ask→allow 顺序 | -- | ❌ | |
| | | 权限规则示例 | `Bash(npm run *)` / `Read(./.env)` / `WebFetch(domain:example.com)` / `Edit(*.ts)` | -- | ❌ | |
| | | 受保护路径 | CWD 外写入始终拒绝（bypass 除外） | -- | ❌ | |
| | | 断路器 | `rm -rf /` + home 目录删除始终提示 | -- | ❌ | |
| | | Sandbox 模式 | sandbox-toggle（Seatbelt macOS / bubblewrap Linux，OS 级文件+网络隔离） | -- | ❌ | |
| | | sandbox.filesystem | 可配置读写白名单 | -- | ❌ | |
| | | sandbox.network | 可配置网络隔离（allowed/denied hosts） | -- | ❌ | |
| | | Bash 命令黑名单 | curl/wget 默认阻止 | -- | ❌ | |
| | | Bash 注入检测 | command injection detection overrides allowlists | -- | ❌ | |
| | **上下文压缩** | 自动触发 | autoCompact（token 阈值） | 85% 阈值自动触发 | ✅ | |
| | | Micro-compact | microCompact（零 API，清除可压缩工具结果/图片/文档） | micro-compact（零 API，白名单工具 + 时间衰减 + 图片/文档占位符） | ✅ | |
| | | Full Compact | LLM 结构化摘要（9 段） | LLM 9 段结构化摘要 | ✅ | |
| | | Compact 前处理 | 跳过 System、图片替换、消息截断 2000 字符、格式化标签 | 跳过 System、图片替换、消息截断 2000 字符、`[用户]/[助手]/[工具结果:id]` 标签 | ✅ | |
| | | Compact 后处理 | 提取 `<summary>` 块 | 移除 `<analysis>` 块 + 提取 `<summary>` + `"此会话从之前的对话延续。"` 前缀 | 🟢 | |
| | | PTL 退化 | -- | Prompt Too Long 退化：逐轮删除最旧消息 + 最多 3 次重试 | 🟢 | |
| | | Re-inject | CLAUDE.md + auto memory + skills（5K/skill, 25K total） | 最近 5 个文件（5K/file, 25K budget）+ Skills（25K budget） | ⚠️（CC 更完善：含 auto memory + paths: 规则 + 嵌套 CLAUDE.md） | |
| | | Snip Compact | snipCompact（snip 投影压缩） | -- | ❌ | |
| | | API Micro-compact | apiMicrocompact（API token 精确计数） | -- | ❌ | |
| | | Cached Micro-compact | cachedMicrocompact（带缓存加速） | -- | ❌ | |
| | | Context Collapse | contextCollapse（持久化上下文折叠） | -- | ❌ | |
| | | Compact 警告 | compactWarningHook + compactWarningState | -- | ❌ | |
| | | 自定义 Compact 指令 | CLAUDE.md `# Compact instructions` 段落 | -- | ❌ | |
| | | 消息分组 | grouping.ts（compact 时消息聚合） | -- | ❌ | |
| | | Micro-compact 白名单 | 可配置工具列表 | `micro_compactable_tools`（Bash/Read/Glob/Grep/Write/Edit） | ✅ | |
| | | Micro-compact 时间衰减 | -- | `micro_compact_stale_steps`（默认 5 步，仅清除过期结果） | 🟢 | |
| | | CompactConfig | 多项压缩设置 | `CompactConfig`（threshold/stale_steps/summary_max_tokens/budget 等 10+ 字段） | ✅ | |
| | | 环境变量覆盖 | -- | `DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT` / `COMPACT_THRESHOLD` | 🟢 | |
| | | /compact 命令 | `/compact [instructions]` | `/compact [instructions]` | ✅ | |
| | | /context 命令 | `/context`（实时分类用量 + 优化建议） | `/context`（上下文窗口使用情况） | ⚠️ | |
| | **配置** | 全局用户配置 | `~/.claude/settings.json` | `~/.peri/settings.json`（兼容读取 .claude/） | ⚠️（路径不同） | |
| | | 项目级配置 | `.claude/settings.json`（可提交 git） | -- | ❌ | |
| | | 本地配置（gitignore） | `.claude/settings.local.json` | -- | ❌ | |
| | | 多源合并 | 7 层优先级（plugin → user → project → local → policy → flag → inline） | 单文件加载 | ⚠️（CC 更完善） | |
| | | 项目上下文 | `CLAUDE.md` + `.claude/CLAUDE.md` | `CLAUDE.md` + `.claude/CLAUDE.md` + `AGENTS.md` | ✅ | |
| | | CLAUDE.local.md | `./CLAUDE.local.md`（个人项目级，自动 gitignore） | `./CLAUDE.local.md`（追加到 CLAUDE.md 末尾） | ✅ | |
| | | CLAUDE.md 规则目录 | `.claude/rules/*.md` + `~/.claude/rules/*.md`（支持 `paths:` frontmatter） | -- | ❌ | |
| | | CLAUDE.md 外部引用 | `<!-- @import path -->` 引用外部文件 | `<!-- @import path -->`（递归解析，深度上限 3，循环检测） | ✅ | |
| | | CLAUDE.md 排除 | `claudeMdExcludes` glob 模式 | `claudeMdExcludes` glob 模式（传入 AgentsMdMiddleware） | ✅ | |
| | | MCP 项目配置 | `.mcp.json` | `.mcp.json` | ✅ | |
| | | MCP scope | `global / enterprise / project / dynamic` | 无 scope 概念 | ❌ | |
| | | MCP 企业策略 | `allowedMcpServers` / `deniedMcpServers` | -- | ❌ | |
| | | MCP 自动审批 | `enableAllProjectMcpServers` | -- | ❌ | |
| | | 环境变量注入 | settings.json `env` 字段 | settings.json `env` 字段 | ✅ | |
| | | API Key 存储 | Keychain（macOS）/ `apiKeyHelper` 脚本 | 明文存储 settings.json | ⚠️（CC 更安全） | |
| | | 模型覆盖 | `modelOverrides`（企业级 Bedrock ARN 映射） | -- | ❌ | |
| | | 模型白名单 | `availableModels`（企业级） | -- | ❌ | |
| | | 权限规则 | `permissions.allow / deny / ask`（tool+content 粒度） | -- | ❌ | |
| | | 权限默认模式 | `permissions.defaultMode`（ask/auto/acceptEdits/bypass/plan/dontAsk） | `YOLO_MODE` env / Shift+Tab | ⚠️（CC 更细粒度） | |
| | | 权限目录 | `permissions.additionalDirectories` | -- | ❌ | |
| | | Auto Mode 配置 | `autoMode.allow / soft_deny / environment` + `useAutoModeDuringPlan` | `LlmAutoClassifier`（运行时分类） | ⚠️（CC 可持久化） | |
| | | Thinking 配置 | `alwaysThinkingEnabled` + `showThinkingSummaries` | `ThinkingConfig`（enabled/budget_tokens/effort） | 🔄 | |
| | | Compact 配置 | 压缩相关设置 | `CompactConfig`（threshold） | ⚠️（CC 更多选项） | |
| | | 会话清理 | `cleanupPeriodDays`（默认 30 天） | -- | ❌ | |
| | | 会话持久化控制 | `sessionPersistence`（可禁用） | 始终持久化 | ❌ | |
| | | Hooks 配置 | `hooks` + `disableAllHooks` + `allowManagedHooksOnly` + `allowedHttpHookUrls` | `hooks` 配置 | ⚠️（CC 更完善） | |
| | | Fast Mode | `fastMode` + `fastModePerSessionOptIn` | -- | ❌ | |
| | | 语言设置 | `language`（响应语言） | `language`（UI 语言） | 🔄 | |
| | | Persona/Tone | -- | `persona` + `tone` 系统提示覆盖 | 🟢 | |
| | | Proactiveness | -- | `proactiveness`（low/medium/high） | 🟢 | |
| | | 插件策略 | `strictPluginOnlyCustomization` + `strictKnownMarketplaces` + `blockedMarketplaces` | `enabledPlugins` + `extraKnownMarketplaces` | ⚠️（CC 企业级） | |
| | | 远程托管设置 | `remoteManagedSettings`（安全检查 + 同步缓存） | -- | ❌ | |
| | | MDM 管理 | macOS plist / Windows registry / Linux JSON（first-source-wins） | -- | ❌ | |
| | | Settings Sync | `settingsSync/`（跨机器同步） | -- | ❌ | |
| | | Auto Updater | `autoUpdatesChannel`（latest/stable） + `minimumVersion` | -- | ❌ | |
| | | 配置验证 | Zod schema + 无效字段保留 + 向后兼容 | serde + unknown fields passthrough | 🔄 | |
| | | 配置迁移 | 版本化自动迁移（当前 v11） | -- | ❌ | |
| | | Schema URL | `$schema` 指向 docs.anthropic.com | `$schema` passthrough（serde 保留） | ✅ | |
| | | CLI 参数 | 50+ flags（--model, --system-prompt, --max-budget-usd 等） | -a / --approve | ⚠️（CC 更丰富） | |
| | | 设置覆盖 | `--settings`（文件或 JSON 字符串） | -- | ❌ | |
| | | 无权限模式 | `--dangerously-skip-permissions` | `YOLO_MODE=true` | ✅ | |
| | | Setup Wizard | OAuth + API key + MCP 审批 + Onboarding | Provider 配置 + 从 CC 迁移 | ⚠️（CC 更完整） | |
| | | Gitignore 尊重 | `respectGitignore`（默认 true） | -- | ❌ | |
| | | Attribution | `attribution.commit / pr` + `includeCoAuthoredBy` | -- | ❌ | |
| | | Git 指令注入 | `includeGitInstructions`（默认 true） | -- | ❌ | |
| | | Worktree 配置 | `worktree.symlinkDirectories` + `worktree.sparsePaths` | -- | ❌ | |
| | | Feature Flags | GrowthBook（远程 + 本地默认，50+ flags） | -- | ❌ | |
| | | Sandbox 配置 | `sandbox` 设置 | -- | ❌ | |
| | | Status Line 配置 | `statusLine` + `statusLineEnabled` | 状态栏（status_bar_hints） | ⚠️ | |
| | | Voice 配置 | `voiceEnabled` + `voiceProvider` | -- | ❌ | |
| | | Assistant 配置 | `assistant` + `assistantName`（KAIROS） | -- | ❌ | |
| | | Channel 配置 | `channelsEnabled` + `allowedChannelPlugins` | -- | ❌ | |
| | | SSH 配置 | `sshConfigs`（远程环境列表） | -- | ❌ | |
| | | Cache 阈值 | `cacheThreshold`（默认 80%） | -- | ❌ | |
| | | Output Style | `outputStyle`（输出风格） | -- | ❌ | |
| | | Reduced Motion | `prefersReducedMotion`（减少动画） | -- | ❌ | |
| | | **TUI** | UI 框架 | React / Ink（自定义 fork） | ratatui（Rust TUI） | N/A（不同技术栈） | |
| | | 消息渲染 | Markdown + 代码高亮 | Markdown + syntect 高亮 | 🔄 | |
| | | 输入框 | 单行 + 多行 | tui-textarea-2 多行 | ✅ | |
| | | 粘贴支持 | 支持 | Event::Paste 独立处理 | ✅ | |
| | | Unicode | 支持 | unicode-width + unicode-segmentation | ✅ | |
| | | 中断恢复 | Ctrl+C | Ctrl+C 自动恢复输入框 | ✅ | |
| | | 多 Session | -- | 分屏显示（彩色边框聚焦） | 🟢 | |
| | | 面板系统 | 分散组件（200+ React 组件） | PanelManager + PanelComponent（11 面板） | 🟢 | |
| | | Headless 测试 | -- | headless.rs（TestBackend） | 🟢 | |
| | | 主题 | ThemePicker（交互式主题选择） | theme.rs（基础） | ⚠️ | |
| | | StatusLine | StatusLine（多种指示器） | 状态栏（status_bar_hints 自描述） | 🔄 | |
| | | 输入建议 | suggestions/（智能命令/参数建议） | -- | ❌ | |
| | | 快捷键系统 | keybindings/defaultBindings（可配置绑定） | 固定绑定 | ❌ | |
| | | Buddy System | buddy/（后台伴随 AI，观察并建议） | -- | ❌ | |
| | **TUI 命令** | /model | ModelPicker | /model / /model alias | ✅ | |
| | | /login | -- | /login（Provider 配置） | 🟢 | |
| | | /history | /resume | /history | 🔄 | |
| | | /agents | -- | /agents | 🟢 | |
| | | /compact | /compact | /compact | ✅ | |
| | | /clear | /clear | /clear | ✅ | |
| | | /config | -- | /config | 🟢 | |
| | | /cost | /cost | /cost | ✅ | |
| | | /context | -- | /context | 🟢 | |
| | | /memory | /memory | /memory | ✅ | |
| | | /help | /help | /help | ✅ | |
| | | /mcp | -- | /mcp | 🟢 | |
| | | /cron | -- | /cron | 🟢 | |
| | | /hooks | /hooks（生命周期 hook 管理） | /hooks | ✅ | |
| | | /split | -- | /split | 🟢 | |
| | | /plan | /plan (plan mode) | -- | ❌ | |
| | | /dream | -- | -- | ❌ | |
| | | /review | -- | -- | ❌ | |
| | | /poor | -- | -- | ❌ | |
| | | /voice | voice（Push-to-talk 语音输入，Anthropic STT/Doubao ASR） | -- | ❌ | |
| | | /proactive | proactive（Tick 驱动自主 agent，无需用户输入） | -- | ❌ | |
| | | /btw | btw（快速侧问题，不打断主对话） | -- | ❌ | |
| | | /doctor | doctor（诊断与健康检查） | /doctor（Settings/Provider/MCP/Model Alias 检查） | ✅ | |
| | | /diff | diff（查看 git diff） | -- | ❌ | |
| | | /ultrareview | ultrareview（远程 bughunter，10-20 分钟自动找 bug） | -- | ❌ | |
| | | /ultraplan | ultraplan（增强多 agent 规划） | -- | ❌ | |
| | | /autofix-pr | autofix-pr（自动修复 PR CI 失败） | -- | ❌ | |
| | | /chrome | chrome（启动 Chrome + MCP 控制） | -- | ❌ | |
| | | /sandbox-toggle | sandbox-toggle（沙箱执行切换） | -- | ❌ | |
| | | /vim | vim（Vim 编辑器集成） | -- | ❌ | |
| | | /peers | peers（列出 LAN/pipe peers） | -- | ❌ | |
| | | /claim-main | claim-main（强制声明主角色） | -- | ❌ | |
| | | /send | send（发送消息给 peer） | -- | ❌ | |
| | | /security-review | security-review（安全代码审查） | -- | ❌ | |
| | | /stickers | stickers（订购实体贴纸） | -- | ❌ | |
| | **插件** | 插件加载 | PluginLoader | PluginMiddleware + loader.rs | ✅ | |
| | | Marketplace | marketplaceManager | marketplace.rs | ✅ | |
| | | 安装管理 | PluginInstallationManager | installer.rs（download/install/update/uninstall） | ✅ | |
| | | 插件 MCP | mcpPluginIntegration | Plugin MCP 命名空间 | ✅ | |
| | | 插件命令 | loadPluginCommands | 支持 | ✅ | |
| | | 插件输出样式 | loadPluginOutputStyles | -- | ❌ | |
| | | ClaudeSettings 解析 | 对象/数组双格式 | 对象/数组双格式 | ✅ | |
| | | extraKnownMarketplaces | 对象/数组双格式 | 自定义反序列化器 | ✅ | |
| | | enabledPlugins | 对象格式写入 | 对象格式写入 | ✅ | |
| | | 安装计数 | installCounts | install_counts.rs | ✅ | |
| | **Hooks** | Hook 事件数 | 28 个（7 类：session/turn/agentic/notification/context/compact/worktree/MCP） | 14 个（session/turn/tool/notification/subagent/compact） | ⚠️（缺 worktree/MCP elicitation） | |
| | | Hook 执行类型 | 5 种（command/http/mcp_tool/prompt/agent） | 4 种（command/http/prompt/agent） | ⚠️（缺 mcp_tool） | |
| | | 异步 Hook | async + asyncRewake（失败唤醒） | async（fire-and-forget） | ⚠️（缺 asyncRewake） | |
| | | Lifecycle hooks | Pre/post execution | 14 个事件（PreToolUse/PostToolUse/PostToolUseFailure/PermissionRequest/UserPromptSubmit/SessionStart/SessionEnd/Stop/StopFailure/SubagentStart/SubagentStop/PreCompact/PostCompact/Notification） | ✅ | |
| | | 变量替换 | 支持 | 4 变量（CLAUDE_PLUGIN_ROOT/DATA/ARGUMENTS + allowed_env_vars） | ✅ | |
| | | 输出解析 | 支持 | 5 级优先级（PreventContinuation → Block → SystemMessage → HookSpecificOutput → Allow） | ✅ | |
| | | 模式匹配 | matcher + if 条件 | matcher（exact/pipe/regex）+ if 条件（ToolName(pattern)） | ✅ | |
| | | Hook-specific 输出 | updatedInput/permissionDecision/additionalContext/initialUserMessage/watchPaths | updatedInput/permissionDecision(additionalContext/SystemMessage) | ✅ | |
| | | SSRF 防护 | -- | SSRF guard（私有 IP 段屏蔽 + CRLF 注入防护） | 🟢 | |
| | | CRLF 注入防护 | -- | HTTP header \r\n 剥离 | 🟢 | |
| | | once 语义 | 支持 | 支持（plugin_id:hook_serialized:event key） | ✅ | |
| | | 项目级 Hooks | `.claude/settings.local.json` hooks 字段 | 同左 | ✅ | |
| | | 插件 Hooks | `hooks/hooks.json` + `plugin.json` hooks 字段 | 同左 | ✅ | |
| | | disableAllHooks | 支持 | -- | ❌ | |
| | | allowManagedHooksOnly | 支持 | -- | ❌ | |
| | | allowedHttpHookUrls | 支持（HTTP hook URL 白名单） | -- | ❌ | |
| | **会话** | 会话持久化 | 持续保存 transcript 文件 | SqliteThreadStore（WAL 模式）+ FilesystemThreadStore | ✅ | |
| | | 会话恢复 | `--continue` / `--resume` / `--from-pr` | /history（ThreadBrowser + cwd 过滤 + git branch + 搜索） | ⚠️（缺 --continue/--from-pr） | |
| | | 会话命名 | `claude -n <name>` / `/rename` | /rename（更新标题 + 持久化） | ✅ | |
| | | 会话分支 | `/branch`（从当前点分叉） | -- | ❌ | |
| | | 会话导出 | Export（导出对话） | -- | ❌ | |
| | | 会话选择器 | 交互式 picker（Ctrl+W 全 worktree/Ctrl+A 全项目/Ctrl+B 按 branch） | ThreadBrowser（列表 + 搜索 + 删除） | ⚠️ | |
| | | Thread 元数据 | name + summary + time + message_count + git_branch | ThreadMeta（id/title/cwd/created_at/updated_at/message_count/content_size） | ✅ | |
| | | 会话记忆 | 6 层记忆架构（Managed Policy → Project → Rules → User → Local → Auto Memory） | 2 层（项目 CLAUDE.md + 用户全局 ~/.claude/CLAUDE.md）+ AGENTS.md | ⚠️（CC 更完善：6 层 + rules/ + local + auto memory） | |
| | | Memory 提取 | extractMemories（自动写入 MEMORY.md，200 行/25KB 上限） | 手动 /memory（编辑 CLAUDE.md） | ❌ | |
| | | Memory 目录 | `~/.claude/projects/<project>/memory/MEMORY.md` | -- | ❌ | |
| | | Auto Memory 控制 | `/memory` 开关 + `autoMemoryEnabled` 配置 | -- | ❌ | |
| | | Subagent Memory | 子 agent 独立 auto memory | -- | ❌ | |
| | | Team Memory | --（通过 Git 共享 CLAUDE.md 实现） | -- | N/A（CC 无此功能） | |
| | | Auto Dream | -- | -- | N/A（CC 无此功能） | |
| | | Conversation Recovery | 持续保存无需恢复 | 持续保存（fire-and-forget append） | ✅ | |
| | | Away Summary | -- | -- | N/A（CC 无此功能） | |
| | **Cron** | Cron 表达式 | 5-field 标准 | 5-field 标准 | ✅ | |
| | | 存储 | 持久化 | 内存（重启丢失） | ⚠️ | |
| | | Jitter | cronJitterConfig（调度抖动） | -- | ❌ | |
| | | Remote Scheduling | scheduleRemoteAgents（远程 cron 触发） | -- | ❌ | |
| | | Kairos | KAIROS / KAIROS_BRIEF（增强调度 + 简报模式） | -- | ❌ | |
| | **遥测** | OpenTelemetry | OTLP exporter | subscriber.rs（OTLP） | ✅ | |
| | | Langfuse | -- | langfuse-client（Trace/Span/Generation + batching） | 🟢 | |
| | | Shot Stats | SHOT_STATS（API 调用统计） | -- | ❌ | |
| | **预算与成本** | Poor Mode | poorMode（禁用 extract_memories + prompt_suggestion 省钱） | -- | ❌ | |
| | **IDE/平台** | VS Code 扩展 | Claude Code Extension（图形面板 + 内联 diff + @-mentions + Plan Review + 多标签） | -- | ❌ | |
| | | VS Code IDE MCP Server | 内建 MCP Server（`mcp__ide__getDiagnostics` + `mcp__ide__executeCode`） | -- | ❌ | |
| | | VS Code URI Handler | `vscode://anthropic.claude-code/open?prompt=&session=` | -- | ❌ | |
| | | JetBrains 插件 | Claude Code Plugin（IntelliJ/PyCharm/WebStorm/GoLand/Android Studio/PhpStorm） | -- | ❌ | |
| | | JetBrains IDE MCP | 内建（diff viewing + selection context + diagnostics） | -- | ❌ | |
| | | Cursor / Windsurf / Kiro | VS Code fork 兼容（Marketplace 安装） | -- | ❌ | |
| | | Neovim | 社区插件（claude-code.nvim） | -- | ❌ | |
| | | ACP 协议（IDE 通用） | `@agentclientprotocol/claude-agent-acp`（Zed 等 ACP Client 直连） | `agent-client-protocol` crate v0.11（peri acp CLI） | ✅ | |
| | | Desktop App | Claude Code Desktop（分屏 + Git 隔离 + 文件编辑器 + Chrome + 手机 Dispatch） | -- | ❌ | |
| | | Web 版 | Claude Code on the Web（Anthropic 沙箱 + Docker + 远程/Teleport） | -- | ❌ | |
| | | Mobile App | Claude Mobile App（Remote Control + Push Notifications） | -- | ❌ | |
| | | Slack 集成 | Claude Code in Slack（委托编码任务） | -- | ❌ | |
| | | GitHub Actions | Claude Code GitHub Actions（CI/CD agent） | -- | ❌ | |
| | | GitLab CI/CD | Claude Code GitLab CI/CD | -- | ❌ | |
| | | DevContainer | Development Containers 支持 | -- | ❌ | |
| | | Chrome 集成 | Chrome Extension（浏览器自动化 + MCP） | -- | ❌ | |
| | | Checkpointing | 文件编辑追踪 + 回滚 + Fork | -- | ❌ | |
| | | **网络/远程** | Bridge Mode | bridge/（WebSocket 远程控制 + 自托管） | -- | ❌ | |
| | | Pipes | pipeTransport（同机多实例协调 UDS/TCP） | -- | ❌ | |
| | | LAN Beacon | lanBeacon（局域网多实例发现 UDP） | -- | ❌ | |
| | | Teleport | teleport（远程 session 生成 + 分支复用） | -- | ❌ | |
| | | SSH Remote | SSH 远程执行 | -- | ❌ | |
| | | Channels | 多通道通信系统 | -- | ❌ | |
| | **Desktop/自动化** | Computer Use | computerUse/（跨平台屏幕/键盘/鼠标控制） | -- | ❌ | |
| | | Chrome Use MCP | Chrome 扩展浏览器自动化 | -- | ❌ | |
| | **Git 集成** | Git 工具 | git/（config 解析、文件历史、归因） | -- | ❌ | |
| | | Diff 显示 | gitDiff（高级 diff 处理和展示） | -- | ❌ | |
| | | File History | fileHistory（文件修改历史追踪） | -- | ❌ | |
| | | Commit Attribution | attribution（代码提交者归因） | -- | ❌ | |
| | **智能特性** | Prompt Suggestion | PromptSuggestion/（上下文感知提示建议） | -- | ❌ | |
| | | Context Analysis | analyzeContext（智能上下文理解） | -- | ❌ | |
| | | Magic Docs | MagicDocs/（自动文档生成） | -- | ❌ | |
| | | Tip System | tips/（上下文提示系统） | -- | ❌ | |
| | **系统级** | Daemon Mode | daemon/（后台 supervisor + worker 管理） | -- | ❌ | |
| | | SDK | entrypoints/sdk/（SDK 类型定义） | -- | ❌ | |
| | **ACP（Agent Client Protocol）** | ACP SDK | `@agentclientprotocol/claude-agent-acp`（v0.11+） | `agent-client-protocol` crate（v0.11, unstable） | ✅ | |
| | | 传输层 | stdio（newline-delimited JSON-RPC） | stdio（tokio stdin/stdout） | ✅ | |
| | | initialize | ✅ 能力协商（loadSession/image/embeddedContext/mcp/http+sse/fork/list/resume/close） | ✅ 能力协商（load_session/image/close/list/resume） | ⚠️（缺 embeddedContext/mcp/http+sse 声明） | |
| | | authenticate | ✅ 3 种（Claude OAuth / Terminal / Gateway） | -- | ❌ | |
| | | session/new | ✅（cwd + MCP + model + permissions） | ✅（cwd + model + thinking） | ✅ | |
| | | session/prompt | ✅（streaming + usage + prompt queueing） | ✅（streaming + task spawning） | ⚠️（缺 prompt queueing） | |
| | | session/cancel | ✅（`query.interrupt()`） | ✅（CancellationToken） | ✅ | |
| | | session/load | ✅（`getSessionMessages()` 全量回放） | ✅（ThreadStore 消息回放） | ✅ | |
| | | session/resume | ✅（重连不回放历史） | ✅（重连不回放历史） | ✅ | |
| | | session/fork | ✅（`unstable_forkSession`） | ✅（`session/fork`） | ✅ | |
| | | session/list | ✅（`listSessions()` + cwd 过滤） | ✅（ThreadStore 列表） | ✅ | |
| | | session/close | ✅（teardown + cancel + dispose） | ✅（cancel + remove） | ✅ | |
| | | session/set_mode | ✅（`query.setPermissionMode()`） | ✅（SharedPermissionMode） | ✅ | |
| | | session/set_config_option | ✅（mode/model/thought_level） | ✅（mode/model/thinking_effort） | ✅ | |
| | | session/set_model | ✅（`unstable_set_session_model`） | ✅（`session/set_model`） | ✅ | |
| | | session/request_permission | ✅（allow_always/allow_once/reject_once + plan mode exit） | ✅（allow_once/reject_once） | ⚠️（缺 allow_always） | |
| | | 权限模式 | default/acceptEdits/plan/auto/bypassPermissions | auto/default/acceptEdits/dontAsk/bypass | ✅ | |
| | | SessionUpdate: agent_message_chunk | ✅ | ✅（TextChunk → AgentMessageChunk） | ✅ | |
| | | SessionUpdate: thought | ✅ | ✅（AiReasoning → ThoughtChunk） | ✅ | |
| | | SessionUpdate: user_message_chunk | ✅ | ✅ | ✅ | |
| | | SessionUpdate: tool_call | ✅（kind: read/edit/execute/think/fetch/search） | ✅（ToolKind: Read/Edit/Execute/Search/Other） | ⚠️（缺 fetch/think kind） | |
| | | SessionUpdate: tool_call_update | ✅（status + terminal_output 支持） | ✅（Completed/Failed） | ⚠️（缺 terminal_output） | |
| | | SessionUpdate: plan | ✅ | ✅（TodoWrite → Plan） | ✅ | |
| | | SessionUpdate: current_mode_update | ✅ | -- | ❌ | |
| | | SessionUpdate: available_commands_update | ✅（`query.supportedCommands()`） | -- | ❌ | |
| | | SessionUpdate: config_option_update | ✅ | ✅（NewSessionResponse.configOptions） | ✅ | |
| | | SessionUpdate: usage_update | ✅（tokens + context window） | -- | ❌ | |
| | | SessionUpdate: session_info_update | 未确认 | -- | ❌ | |
| | | Prompt Queueing | ✅（`_meta.claudeCode.promptQueueing`） | -- | ❌ | |
| | | embeddedContext 支持 | ✅ | -- | ❌ | |
| | | MCP 能力声明 | ✅（http + sse） | -- | ❌（ACP 模式未集成 MCP） | |
| | | Terminal Output Streaming | ✅（`_meta.terminal_output`） | -- | ❌ | |
| | | CLI 入口 | `claude --acp` | `peri acp [--cwd] [--model] [--agent]` | ✅ | |
| | | Agent 覆盖 | -- | `--agent`（.claude/agents/ 定义） | 🟢 | |
| | | **Peri 独有** | Widget 库 | -- | perihelion-widgets（11 组件） | 🟢 | |
| | | 5 级权限快捷切换 | -- | Shift+Tab 循环 | 🟢 | |
| | | Tool Search 延迟加载 | -- | SearchExtraTools / ExecuteExtraTool 元工具 | 🟢 | |

---

## 统计摘要

| 状态 | 数量 | 占比 |
|------|------|------|
| ✅ 兼容 | 122 | 30% |
| 🔄 对等 | 12 | 3% |
| 🟢 Peri 更好 | 45 | 11% |
| ⚠️ 部分兼容 | 31 | 8% |
| ❌ 缺失 | 183 | 46% |
| N/A | 4 | 1% |
| **总计** | **397** | 100% |

## 缺失特性优先级建议

| 优先级 | 特性 | 理由 |
|--------|------|------|
| **P0 高** | Plan Mode | 结构化规划→验证→执行，核心 agent 能力 |
| **P0 高** | 权限规则语法 | `Tool(specifier)` + deny→ask→allow，HITL 安全基础 |
| **P0 高** | Bash 命令黑名单 / 注入检测 | curl/wget 阻止 + command injection detection，当前无命令级防护 |
| **P1 中** | OS 级 Sandbox | Seatbelt/bubblewrap 文件+网络隔离，安全差异化 |
| **P1 中** | Auto Memory | 自动提取记忆写入 MEMORY.md，长期上下文核心 |
| **P1 中** | rules/ 目录 + paths: frontmatter | 模块化 CLAUDE.md 规则，大型项目必需 |
| **P1 中** | mcp_tool Hook 类型 | Hook 调用 MCP 工具，扩展 hook 能力 |
| **P1 中** | Computer Use | 桌面自动化，差异化场景 |
| **P1 中** | LSP 集成 | 代码智能，开发效率 |
| **P1 中** | Bedrock/Vertex/Gemini/Grok | 更多 Provider，用户覆盖 |
| **P1 中** | Voice Mode | 语音交互，新兴交互方式 |
| **P1 中** | 会话分支/导出 | 会话管理完善 |
| **P2 低** | asyncRewake Hook | 异步 Hook 失败唤醒 |
| **P2 低** | disableAllHooks / allowManagedHooksOnly | Hook 管理控制 |
| **P2 低** | PowerShell 支持 | Windows 用户 |
| **P2 低** | Bridge / Pipes / LAN | 远程/多机协作 |
| **P2 低** | Daemon Mode | 长驻后台 |
| **P2 低** | Context Collapse | 持久化折叠 |
| **P2 低** | 17 个内建 Skills | 内建 skill 生态 |
| **P2 低** | Skill 搜索 / Learning | 智能 skill 发现 |
| **P2 低** | Token Budget | --max-budget-usd，成本控制 |
| **P3 可选** | SSH Remote | 远程执行 |
| **P3 可选** | Buddy System | 伴随 AI |
| **P3 可选** | Chrome Use MCP | 浏览器自动化 |
| **P3 可选** | Git 集成（高级） | 文件历史/归因 |
| **P3 可选** | SDK | SDK 导出 |
| **P3 可选** | Feature Flags (GrowthBook) | 远程开关 |
