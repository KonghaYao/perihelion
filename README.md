# Perihelion

> 用 Rust 打造的高性能 AI Agent 框架。把 AI 编程助手的工作流搬进你自己的应用——Agent 定义、Skills、HITL 审批，全部开箱即用。

```bash
cargo run -p rust-agent-tui
```

## 你已有的配置，这里直接能跑

不需要迁移，不需要重新学习。把项目丢进来，Agent 就知道该怎么做：

- **`.claude/agents/`** — 子 Agent 定义直接复用，`tools`、`maxTurns`、`disallowedTools` 全部识别
- **`.claude/skills/`** — Skills 自动扫描加载，TUI 内 `#` 触发补全
- **`AGENTS.md` / `CLAUDE.md`** — 项目指引文件自动注入 System Prompt
- **`ask_user` 协议** — 标准问答交互，单选/多选/自定义输入
- **HITL 审批** — 敏感操作强制拦截，支持 Approve / Edit / Reject / Respond
- **`launch_agent`** — 把复杂任务拆给专门的子 Agent，防递归，工具集可精确控制

## 核心能力

- **ReAct 循环** — 思考 → 工具调用 → 反馈，自主推进直到完成
- **可插拔中间件** — 文件读写、终端命令、HITL、子 Agent，按需组装
- **多 LLM 支持** — OpenAI / Anthropic / 任意兼容接口
- **交互式 TUI** — 终端内完整对话体验，多会话持久化，`/model` 随时切换

## 快速上手

```bash
cargo run -p rust-agent-tui        # 启动（默认 YOLO，跳过审批）
cargo run -p rust-agent-tui -- -a  # 启用 HITL 审批模式
```

## 架构

```
rust-create-agent/       核心：ReAct 执行器、LLM 适配、工具系统
rust-agent-middlewares/  中间件：文件系统、终端、HITL、子 Agent
rust-agent-tui/          交互式 TUI
```

## License

MIT
