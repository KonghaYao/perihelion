# 项目概述

![项目全景](./images/02-project-overview.png)

## 项目目标

Perihelion 是一个用 Rust 构建的高性能 AI Agent 框架，目标是让开发者能够将 AI 编程助手的完整工作流（ReAct 推理、工具调用、HITL 审批、子 Agent 委派）嵌入自己的应用中，开箱即用、性能可靠。

核心价值：
- **高兼容**：完全复用已有的 `.claude/agents/`、`.claude/skills/`、`CLAUDE.md` / `AGENTS.md` 配置，零迁移成本
- **可插拔**：中间件模式，按需组合文件系统、终端、HITL、子 Agent 等能力
- **生产可用**：异步运行时、线程持久化、OpenTelemetry 追踪，面向实际部署

## 核心用户

**应用开发者**：希望在自己的产品中集成 AI Agent 能力，例如构建代码辅助工具、自动化运维系统、交互式命令行助手。

**工具作者**：需要快速原型验证 Agent 行为，通过 TUI 交互界面实时调试 ReAct 循环和工具调用链路。

## 系统边界

系统与以下外部服务交互：

- **Anthropic API**：Claude 系列模型，支持扩展思考（Extended Thinking）和 Prompt Cache
- **OpenAI 兼容接口**：任意支持 `/chat/completions` 接口的服务（OpenAI、DeepSeek、本地模型等）
- **Relay Server（内置）**：WebSocket 中继服务，允许远程访问本地运行的 Agent
- **SQLite（本地）**：会话线程持久化，路径 `~/.zen-core/threads/threads.db`
- **文件系统（本地）**：Agent 工具对本地文件的读写操作
- **Shell / 终端**：`bash` 工具执行系统命令

## 核心业务流程

### 1. ReAct 推理循环
用户提交消息 → 中间件 `before_agent` 预处理 → LLM 生成推理 → 循环执行工具调用（每轮调用 before/after 钩子）→ 获取最终答案 → 中间件 `after_agent` 后处理 → 返回结果。

### 2. HITL 审批流程
敏感工具（`bash`、写文件、编辑文件等）调用前，`HitlMiddleware` 拦截 → 发送 `ApprovalNeeded` 事件到 TUI → 用户选择 Approve / Edit / Reject / Respond → 结果通过 oneshot channel 返回给 Agent 继续执行。

### 3. SubAgent 委派
父 Agent 调用 `launch_agent` 工具 → 读取 `.claude/agents/{agent_id}.md` 定义 → 创建子 Agent 实例（独立 LLM、继承或过滤工具集）→ 执行子任务 → 以字符串形式返回执行结果给父 Agent。

---
*最后更新: 2026-03-24 — 初始化生成*
