# Peri

> 用 Rust 打造的高性能 AI Agent 框架。把 AI 编程助手的工作流搬进你自己的应用——Agent 定义、Skills、HITL 审批，全部开箱即用。

```bash
cargo run -p rust-agent-tui
```

## 你已有的配置，这里直接能跑

不需要迁移，不需要重新学习。把项目丢进来，Agent 就知道该怎么做：

- **`.claude/agents/`** — 子 Agent 定义直接复用，`tools`、`maxTurns`、`disallowedTools` 全部识别
- **`.claude/skills/`** — Skills 自动扫描加载，TUI 内 `/` 触发补全
- **`AGENTS.md` / `CLAUDE.md`** — 项目指引文件自动注入 System Prompt
- **`ask_user` 协议** — 标准问答交互，单选/多选/自定义输入
- **HITL 审批** — 敏感操作强制拦截，支持 Approve / Edit / Reject / Respond
- **`Agent` 工具** — 把复杂任务拆给专门的子 Agent，防递归，工具集可精确控制
- **MCP 协议** — 接入外部 MCP 服务器，stdio / Streamable HTTP / OAuth 2.0 全支持
- **上下文压缩** — Token 达到阈值自动压缩，Micro-compact（零 API）+ Full Compact（LLM 摘要）
- **后台 Agent** — 最多 3 个子 Agent 并发执行，父 Agent 完成后自动等待

## 核心能力

- **ReAct 循环** — 思考 → 工具调用 → 反馈，自主推进直到完成
- **可插拔中间件** — 文件读写、终端命令、HITL、子 Agent、MCP、Cron、Todo，按需组装
- **多 LLM 支持** — OpenAI / Anthropic / 任意兼容接口，`/model` 随时切换
- **Thinking/推理模式** — Anthropic `thinking`、OpenAI `reasoning_effort`，budget_tokens 可配
- **交互式 TUI** — 终端内完整对话体验，多会话分屏持久化，Markdown 渲染
- **遥测集成** — Langfuse + OpenTelemetry OTLP，开箱即用

## 快速上手

```bash
cargo run -p rust-agent-tui        # 启动（默认 YOLO，跳过审批）
cargo run -p rust-agent-tui -- -a  # 启用 HITL 审批模式
```

### 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE` | `true` 跳过审批（默认），`false` 启用 HITL |

环境变量也可通过 `~/.peri/settings.json` 的 `env` 字段配置。

## Workspace 架构

```
rust-create-agent/       核心：ReAct 执行器、LLM 适配、工具系统、线程持久化、OTel 遥测
    ↑
rust-agent-middlewares/  中间件：文件系统、终端、HITL、子 Agent、MCP、Cron、Todo、Skills
    ↑
perihelion-widgets/      独立 Widget 库（11 组件）：BorderedPanel、ScrollableArea、SelectableList 等
    ↑
rust-agent-tui/         交互式 TUI 应用，多会话分屏、Slash 命令、上下文压缩

langfuse-client/        Langfuse 遥测客户端（独立）
perihelion-lsp/         LSP 客户端库（独立，被 middlewares 使用）
```

## TUI 命令

输入 `/` 前缀触发命令面板：

| 命令 | 说明 |
|------|------|
| `/login` | 管理 Provider 配置 |
| `/model` | 模型选择面板（opus / sonnet / haiku） |
| `/history` | 历史对话浏览 |
| `/agents` | SubAgent 定义管理 |
| `/compact` | 上下文压缩 |
| `/clear` | 清空消息列表 |
| `/config` | 查看/编辑运行时配置 |
| `/cost` | Token 用量和成本 |
| `/context` | 上下文窗口使用情况 |
| `/memory` | 持久化记忆管理 |
| `/help` | 命令列表 |

## 中间件链

按顺序组装，每个中间件提供工具或拦截行为：

1. **AgentDefineMiddleware** — 解析 Agent 定义，设置 model/maxTurns 覆盖
2. **AgentsMdMiddleware** — 读 CLAUDE.md/AGENTS.md 注入 System Prompt
3. **SkillsMiddleware** — Skills 摘要注入 System Prompt
4. **SkillPreloadMiddleware** — `/skill-name` 全文注入
5. **FilesystemMiddleware** — Read / Write / Edit / Glob / Grep / folder_operations
6. **TerminalMiddleware** — Bash 工具
7. **TodoMiddleware** — TodoWrite 工具
8. **CronMiddleware** — Cron 调度工具
9. **HumanInTheLoopMiddleware** — 拦截敏感工具审批
10. **SubAgentMiddleware** — Agent 委派工具
11. **McpMiddleware** — MCP 工具和资源注入

## MCP 集成

通过 `McpMiddleware` 将外部 MCP 服务器注入 ReAct 循环：

- **配置来源**：全局 `~/.peri/settings.json` + 项目级 `.mcp.json`，同名项目级覆盖
- **传输方式**：stdio（子进程）/ Streamable HTTP（远程服务器）
- **OAuth 2.0**：支持 Client Credentials Flow
- **工具命名**：`mcp__{server_name}__{tool_name}`
- **资源读取**：`mcp__read_resource`，120 秒超时

## License

MIT
