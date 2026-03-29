# 已有功能清单

![功能模块概览](./images/05-feature-modules.png)

## 核心引擎（rust-create-agent）

- **ReAct 循环执行器:** `ReActAgent` 支持最多 50 次迭代，思考 → 工具调用 → 反馈自动推进，parallel 工具调用（同轮多工具同时执行）
- **MockLLM 测试工具:** `MockLLM::tool_then_answer()` 按脚本回放推理序列，无需真实 API，覆盖单元测试场景
- **OpenAI 适配器:** 支持 `message.reasoning_content`（DeepSeek-R1/o 系列），streaming SSE，`type:"function"` 工具格式
- **Anthropic 适配器:** Prompt Cache（默认开启，最后消息末尾 `cache_control:ephemeral`），Extended Thinking（`budget_tokens`），`system` 字段 blocks 格式
- **MessageAdapter 双向转换:** `OpenAiAdapter` / `AnthropicAdapter` 实现 `MessageAdapter` trait，`BaseMessage` ↔ Provider 原生 JSON
- **ContentBlock 完整支持:** Text / Image（Base64 & URL）/ Document / ToolUse / ToolResult / Reasoning / Unknown 透传
- **Middleware Chain:** `Middleware<S>` trait，`before_agent` / `after_agent` / `before_tool` / `after_tool` / `collect_tools` 五个钩子

## 中间件（rust-agent-middlewares）

- **FilesystemMiddleware:** 提供 `read_file`、`write_file`、`edit_file`、`glob_files`、`search_files_rg`、`folder_operations` 六个工具；只读工具无需 HITL
- **TerminalMiddleware:** 提供 `bash` 工具，120 秒超时，跨平台（Windows: `cmd /C`，其他: `bash -c`）
- **HitlMiddleware:** `before_tool` 拦截敏感操作（bash/write/edit/delete/rm/folder），四种决策：Approve / Edit / Reject / Respond；oneshot channel 异步等待用户决策
- **SubAgentMiddleware:** 提供 `launch_agent` 工具，读取 `.claude/agents/{id}.md`，工具集过滤（tools 白名单 + disallowedTools 黑名单），防递归（始终排除 `launch_agent` 自身），返回格式含工具调用摘要
- **SkillsMiddleware:** `before_agent` 扫描加载 Skills（`~/.claude/skills/` → `skillsDir` → `./.claude/skills/`），prepend System prompt
- **AgentsMdMiddleware:** `before_agent` 自动读取 `CLAUDE.md` / `AGENTS.md`，prepend System prompt
- **TodoMiddleware:** `after_tool` 解析 `todo_write` 结果，推送 Todo 状态到渲染 channel
- **AskUserTool:** `ask_user_question` 工具（对齐 Claude AskUserQuestion），入参为 `questions` 数组（1–4 个），每题含 `question` 问题文字、`header` 短标签（≤12字）、`multi_select` 字段、`options`（每项含 `label` + `description`），始终允许自定义输入；oneshot channel 挂起等待用户输入

## TUI 界面（rust-agent-tui）

- **多会话历史:** `SqliteThreadStore` 持久化会话，`/history` 面板浏览（j/k 导航，d 删除，Enter 打开，Esc 新建）
- **模型别名映射:** Opus/Sonnet/Haiku 三级别名，`/model` 三 Tab 面板，`/model <alias>` 快捷切换
- **TUI 命令:** `/clear` 清空消息、`/help` 命令列表、`/compact` 上下文压缩
- **Skills 补全:** 输入 `#` 触发 Skills 浮层，Tab 导航，Enter 补全为 `#skill-name`；发送含 `#skill-name` 的消息时自动通过 `SkillPreloadMiddleware` 将 skill 全文注入 agent state（fake read_file 工具调用序列）
- **HITL 弹窗:** `ApprovalNeeded` 事件触发审批弹窗，展示工具名称和参数，支持 Approve / Edit / Reject / Respond
- **AskUser 弹窗:** `AskUserBatch` 事件触发问答弹窗，支持批量问题，单选/多选
- **YOLO 模式:** `-y` 参数启动，自动 Approve 所有 HITL 请求（不影响 ask_user）
- **剪贴板图片粘贴:** `Ctrl+V` 读取 PNG 图片，Base64 编码为 Image ContentBlock，支持多张图片
- **渲染线程分离:** 独立渲染线程（`parking_lot::RwLock<RenderCache>` + `Notify` 驱动），零 sleep，与 Agent 执行线程解耦，按需重绘
- **Headless 测试模式:** `App::new_headless(w, h)` + `ratatui TestBackend`，与生产渲染管道完全一致，用于 CI 集成测试
- **弹窗滚动支持:** 所有面板（AskUser/Model/Agents/Thread）高度限制在屏幕 80%，内容超长可 ↑↓ 滚动
- **Bracketed Paste Mode:** `Ctrl+V` 粘贴多行文本，保留换行不触发 Enter 提交
- **Loading 输入缓冲:** Agent 运行中可继续输入，消息自动缓存，完成后合并发送
- **TODO 状态面板:** 输入框上方固定面板，颜色分类（InProgress 黄/Completed 暗灰/Pending 白）
- **工具颜色分层:** 工具名（颜色+BOLD）+ 参数（DarkGray），文件路径自动缩短
- **Relay 集成:** 可选连接 Relay Server，事件实时转发，支持远程操控；Web 端支持 `/compact` 命令触发压缩；Agent thread 状态变更（clear/history/compact）通过 `ThreadReset` 消息自动同步到 Web 前端；Web 端支持"停止"按钮（`CancelAgent` 消息）中断 Agent 运行

## 基础设施

- **SQLite 线程持久化:** WAL 模式，`parking_lot::Mutex<Connection>` 串行写，`append_messages` 事务保证 crash-safe，`StateSnapshot` 事件驱动增量写入
- **OpenTelemetry 追踪:** 内置 OTLP HTTP 导出，`OTEL_EXPORTER_OTLP_ENDPOINT` 环境变量控制开关，tracing-opentelemetry 桥接，兼容 Jaeger
- **结构化日志:** `RUST_LOG` 级别控制，`RUST_LOG_FORMAT=json` 切换 JSON 格式
- **配置持久化:** `~/.zen-code/settings.json` 存储 Provider/Model 配置，`AppConfig` 统一读写，`env` 字段替代 .env 文件注入环境变量
- **Relay Server:** axum + tokio-tungstenite，支持 WebSocket 多 Agent 会话管理、心跳、Tab 状态广播；可选 client feature 仅引入 tungstenite；多用户隔离（UserNamespace + 匿名注册 /register）

---
*最后更新: 2026-03-29 — 由 F004/F003/F002 归档更新：settings.json env 注入、Relay 多用户隔离*
