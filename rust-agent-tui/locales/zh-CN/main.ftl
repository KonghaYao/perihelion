# ============================================================
# Peri TUI — 简体中文 (zh-CN) Translation File
# Key names must match en/main.ftl exactly.
# ============================================================

# ---- i18n 基础设施测试 key ----
test-hello = 你好，世界！
test-greeting = 你好，{ $name }！
ui-empty = 无

# ---- Command Descriptions ----

command-help-description = 列出所有可用命令
command-clear-description = 清空消息列表
command-compact-description = 压缩对话上下文（结构化摘要 + 重新注入最近文件/Skills）
command-model-description = 打开模型选择面板（Provider + 级别 + Thinking）；带参数时直接切换别名（opus/sonnet/haiku）
command-login-description = 管理 Provider 配置（新建/编辑/删除）
command-cost-description = 查看当前会话费用和 token 消耗
command-context-description = 查看上下文使用率和会话统计
command-agents-description = 打开 Agent 选择面板
command-mcp-description = 管理 MCP 服务器连接
command-memory-description = 编辑用户/项目级 CLAUDE.md 记忆文件
command-history-description = 打开历史对话浏览面板
command-loop-description = 注册定时循环任务（自然语言描述，如 /loop 每隔5分钟提醒我喝水）
command-cron-description = 查看和管理定时任务
command-plugin-description = 管理插件（浏览、安装、卸载）
command-config-description = 全局配置（autocompact、语言、系统提示词覆盖）
command-hooks-description = 查看 Hook 配置
command-effort-description = 查看或设置推理力度（low/medium/high/xhigh/max）
command-doctor-description = 诊断配置完整性
command-rename-description = 查看或修改当前会话标题
command-split-description = 新建分栏会话
command-lang-description = 切换界面语言（如 /lang zh-CN）
command-agent-description = 设置 Agent 定义，切换不同的 Agent 角色

# ---- Command Execution Messages ----

# help command
help-available-commands = 可用命令：
help-alias-prefix = （别名: /{ $aliases }）
help-skills-count = Skills（{ $count } 个可用）: 输入 # 前缀查看
help-skills-empty = Skills: 将 .md 文件放入 .claude/skills/ 目录即可添加
help-shortcuts = 快捷键：Shift+Tab 切换权限模式 │ Alt+M 切换模型 │ Esc 退出 │ Ctrl+C 中断

# compact command
compact-agent-running = Agent 运行中，无法执行压缩

# history command
history-agent-running = Agent 运行中，无法打开历史面板

# model command
config-save-failed = 配置保存失败: { $error }

# effort command
effort-set = 推理力度已设为 { $effort }
effort-current = 当前推理力度: { $effort }
effort-usage = 用法: /effort low|medium|high|xhigh|max

# loop command
loop-usage = 用法: /loop <自然语言时间描述> <提示词>
loop-example = 例如: /loop 每隔5分钟提醒我喝水

# rename command
rename-no-session = 当前无活跃会话，无法重命名
rename-current-title = 当前标题: { $title }
rename-updated = 会话标题已更新为: { $name }
rename-failed = 重命名失败: { $error }
rename-untitled = （无标题）

# doctor command
doctor-header = Doctor 检查结果：
doctor-settings-missing = Missing  无法获取 home 目录
doctor-no-api-key = Missing  未设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY
doctor-no-provider = No Provider  未配置任何 Provider
doctor-provider-not-found = No Provider  active_provider_id '{ $id }' 未找到
doctor-no-mcp = None  未配置 MCP 服务器
doctor-mcp-not-init = None  .mcp.json 存在但 MCP 未初始化
doctor-no-alias = No Alias  未配置任何模型别名
doctor-no-alias-no-provider = No Alias  无活跃 Provider
doctor-no-alias-not-configured = No Alias  未配置

# lang command
lang-switched = 语言已切换为 { $lang }
lang-available = 可用语言: { $langs }
lang-unsupported = 不支持的语言: { $lang }

# ---- Status Bar ----

statusbar-permission-dont-ask = Don't Ask
statusbar-permission-accept-edit = Accept Edit
statusbar-permission-auto = Auto Mode
statusbar-permission-bypass = Bypass
statusbar-copied = 已复制 { $count } 个字符
statusbar-no-agent = 无
statusbar-bg-indicator = [BG: { $count }]
statusbar-retrying = 重试 { $attempt }/{ $max } ({ $delay }s): { $error }
statusbar-mcp-connecting =  MCP ({ $connected }/{ $total })...
statusbar-mcp-ready =  MCP 就绪 ({ $total } 个服务器)
statusbar-mcp-failed =  MCP 失败: { $msg }
statusbar-lsp-diag = 诊断: { $errors }E/{ $warnings }W

# ---- Status Bar Shortcut Hints (main view) ----

key-command = 命令
key-switch-session = :切换Session
key-close = :关闭
key-scroll = :滚动
key-cancel = :取消
key-newline = :换行
key-open-browser = :打开浏览器
key-submit = :提交
key-switch = :切换
key-move = :移动
key-select = :选择
key-confirm = :确认

# ---- Welcome Page ----

welcome-title = Peri Agent Framework
welcome-divider = ────── 我能做什么？ ──────
welcome-feature-code = 让我帮你编写、调试或重构代码
welcome-feature-files = 管理文件和运行终端命令
welcome-feature-agents = 将任务委派给专业子 Agent
welcome-login-hint-1 = 请输入
welcome-login-hint-2 = 配置 API Key 开始使用
welcome-shortcut-quit = :退出
welcome-shortcut-stop = :停止
welcome-shortcut-newline = :换行
welcome-shortcut-mode = :模式
welcome-shortcut-model = :模型
welcome-skills-available = { $count } 个 skills 可用

# ---- Tips (18 items) ----

tip-0 = 按 / 输入命令，Tab 补全
tip-1 = Ctrl+C 中断 Agent，Shift+Tab 切换权限模式
tip-2 = Alt+M 快速切换模型（opus / sonnet / haiku）
tip-3 = Alt+Enter 在输入框中换行
tip-4 = 拖拽文件或图片到终端可自动附加到消息
tip-5 = 长按 Ctrl+V 粘贴剪贴板图片
tip-6 = Ctrl+U/D 滚动消息历史，↑/↓ 浏览输入历史
tip-7 = Ctrl+N/P 切换 Session，Ctrl+W 关闭
tip-8 = Esc 关闭弹窗或面板，Enter 确认选择
tip-9 = /compact 压缩上下文节省 token
tip-10 = /clear 清空当前对话
tip-11 = /model 切换 LLM 模型
tip-12 = /history 浏览历史对话记录
tip-13 = /loop 创建定时循环任务
tip-14 = /plugin 管理 Claude Code 插件
tip-15 = 在 .claude/skills/ 中添加自定义 Skills
tip-16 = 在 .claude/agents/ 中定义 SubAgent
tip-17 = 对复杂任务让 Agent 先制定计划再执行

# ---- Setup Wizard ----

setup-welcome-title =  ── Peri 设置 ── 欢迎
setup-choose-provider =  选择如何配置你的 Provider：
setup-source-custom-api = Custom API
setup-source-migrate = 从 Claude Code 迁移
setup-source-custom-desc = 手动输入 Provider 详情
setup-source-migrate-desc = 从 ~/.claude/ 导入配置
setup-key-confirm = :确认
setup-key-select = :选择
setup-key-quit = :退出
setup-configure-title =  ── Peri 设置 ── 配置 Providers
setup-submit = 提交
setup-key-edit-submit = :编辑/提交
setup-key-check = :勾选
setup-key-back = :返回
setup-edit-title =  ── 设置 ── 编辑: { $type } ({ $id })
setup-field-type = Type
setup-field-id = ID
setup-field-base-url = Base URL
setup-field-api-key = API Key
setup-field-opus = Opus
setup-field-sonnet = Sonnet
setup-field-haiku = Haiku
setup-model-label = Model
setup-confirm = 确认
setup-key-switch-type = :切换类型
setup-key-back-list = :返回列表
setup-complete-title =  ── 设置完成 ✓
setup-press-enter = 按
setup-to-start = 开始使用
setup-no-key = (无密钥)

# ---- Config Panel ----

config-panel-title-browse =  /config — 配置
config-panel-title-edit =  /config — 编辑配置
config-field-autocompact = Autocompact
config-field-compact-threshold = Compact 阈值
config-field-language = 语言
config-field-persona = Persona
config-field-tone = Tone
config-field-proactiveness = Proactiveness
config-value-on = 开
config-value-off = 关
config-saved = 配置已保存
config-key-navigate = :导航
config-key-edit = :编辑
config-key-save = :保存
config-key-toggle = :切换

# ---- Login Panel ----

login-panel-title-browse =  /login — Provider 管理
login-panel-title-edit =  /login — 编辑 Provider
login-panel-title-new =  /login — 新建 Provider
login-key-activate = :激活
login-key-new = :新建
login-key-delete = :删除
login-key-paste = :粘贴
login-confirm-delete = :确认删除

# ---- HITL Popup ----

hitl-single-title =  ⚠ 工具审批 (1 项)
hitl-batch-title =  ⚠ 批量工具审批
hitl-approved = [批准]
hitl-rejected = [拒绝]
hitl-summary = 已选: { $approved } 批准 / { $rejected } 拒绝

# ---- AskUser Popup ----

ask-user-placeholder = 输入自定义内容...

# ---- App Messages ----

app-provider-ready = { $name } ({ $model }) 已就绪
app-not-configured = 未配置
app-empty = 无
app-no-api-key-warning = 警告: 未设置任何 API Key（ANTHROPIC_API_KEY 或 OPENAI_API_KEY）
app-interrupted-resumed = ⚠ 已强制中断（输入已恢复到输入框）
app-interrupt-done = ⚠ 已中断
app-interrupted-background = ⚠ 已强制中断（后台任务可能仍在运行）
app-config-saved = 配置已保存
app-config-save-failed = 配置保存失败: { $error }
app-provider-activated = 已激活 Provider: { $name }
app-provider-created = 已新建并激活 Provider: { $name }
app-provider-saved = 已保存并激活 Provider: { $name }
app-provider-deleted = 已删除 Provider: { $name }
app-provider-name-empty = 保存失败：Provider 名称不能为空
app-agent-reset = Agent 已重置（未设置 agent_id）
app-agent-switched = Agent 已切换为: { $name } ({ $id })
app-agent-disconnected = Agent 连接异常断开，请重试发送消息
app-compact-no-context = 无可压缩的上下文（历史消息为空）
app-compact-no-provider = 压缩失败: 未配置 LLM Provider（请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY）
app-compact-compressing = 压缩上下文
app-compact-done = 上下文已压缩
app-compact-failed = 压缩失败: { $error }
app-compact-auto-cleared = 自动清理：释放了 { $count } 个工具调用结果
app-compact-limit-reached = 上下文压缩后仍超出限制，已停止自动继续。请使用 /compact 手动压缩或 /clear 清空历史。
app-model-switched = 模型已切换为: { $alias } ({ $effort } effort)
app-prompt-cache-low = Prompt cache 命中率 { $rate }% < 80% (req: { $req })
app-no-mcp-configured = 无 MCP 服务器配置（请在 .mcp.json 或 settings.json 中添加）
app-no-cron-tasks = 无定时任务
app-cron-deleted = 已删除定时任务: { $preview }
app-submit-attachments = { $input } [ { $count } 张图片 ]
app-no-provider-submit = 未配置 API Key，请输入 /login 配置 Provider
app-bg-task-done = [后台任务 { $id } 已完成] Agent: { $agent } | 工具调用: { $tools } | 耗时: { $duration }ms
app-bg-task-done-with-result = [后台任务 { $id } 已完成] Agent: { $agent } | 工具调用: { $tools } | 耗时: { $duration }ms\n结果:\n{ $result }
app-bg-task-failed = [后台任务 { $id } 执行失败] Agent: { $agent } | { $error }
app-bg-task-failed-with-error = [后台任务 { $id } 执行失败] Agent: { $agent }\n错误:\n{ $error }

# ---- Panel Status Bar Hints ----

# Login panel
hint-login-browse = :导航
hint-login-activate = :激活
hint-login-edit = :编辑
hint-login-new = :新建
hint-login-delete = :删除
hint-login-close = :关闭
hint-login-field = :字段
hint-login-save = :保存
hint-login-paste = :粘贴
hint-login-toggle = :切换
hint-login-back = :返回

# Config panel
hint-config-browse-navigate = :导航
hint-config-browse-edit = :编辑
hint-config-edit-field = :字段
hint-config-edit-save = :保存

# Model panel
hint-model-navigate = :导航
hint-model-confirm = :确认
hint-model-effort = :Effort
hint-model-close = :关闭

# Agent panel
hint-agent-select = :选择
hint-agent-confirm = :确认
hint-agent-cancel = :取消

# MCP panel
hint-mcp-navigate = :导航
hint-mcp-detail = :详情
hint-mcp-reconnect = :重连
hint-mcp-delete = :删除
hint-mcp-execute = :执行
hint-mcp-back = :返回
hint-mcp-close = :关闭

# Plugin panel
hint-plugin-uninstall = :确认卸载
hint-plugin-cancel = :取消
hint-plugin-delete = :确认删除
hint-plugin-add = :添加
hint-plugin-exit-search = :退出搜索
hint-plugin-tab = :Tab
hint-plugin-install = :安装
hint-plugin-remove = :Remove
hint-plugin-navigate = :导航
hint-plugin-execute = :执行
hint-plugin-back = :返回列表
hint-plugin-select = :选择
hint-plugin-search = :搜索

# Cron panel
hint-cron-confirm-delete = :确认删除
hint-cron-navigate = :导航
hint-cron-toggle = :切换
hint-cron-delete = :删除
hint-cron-close = :关闭

# Status panel
hint-status-tab = :切换Tab
hint-status-close = :关闭

# History panel
hint-history-confirm-delete = :确认删除
hint-history-exit-search = :退出搜索
hint-history-close = :关闭

# Hooks panel
hint-hooks-navigate = :导航
hint-hooks-close = :关闭

# Memory panel
hint-memory-select = :选择
hint-memory-edit = :编辑
hint-memory-close = :关闭

# ---- Plugin Panel Messages ----

app-plugin-updating = 正在更新 marketplace: { $name }
app-plugin-delete-failed = 删除失败: { $error }
app-plugin-add-failed = 添加失败: { $error }
app-plugin-added = Marketplace 已添加: { $name } (正在获取内容...)
