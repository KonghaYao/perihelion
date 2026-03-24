# Perihelion

用 Rust 构建的 AI Agent 框架——ReAct 推理、可组合中间件、交互式终端。

```bash
cargo run -p rust-agent-tui
```

## 核心能力

- **ReAct 循环** — 思考 → 调用工具 → 反馈，自主完成复杂任务
- **可插拔中间件** — 文件读写、终端命令、HITL 审批、子 Agent 委派，按需组合
- **多 LLM 支持** — OpenAI / Anthropic / 兼容接口，统一接入
- **交互式 TUI** — 终端内对话，命令补全、多会话持久化、Skills 扩展

## 快速上手

```bash
cargo run -p rust-agent-tui        # 启动
cargo run -p rust-agent-tui -- -y  # YOLO 模式（跳过审批）
```

TUI 内用 `/model` 配置模型，`#` 触发 Skills 补全。

## 架构

```
rust-create-agent/       核心：ReAct 执行器、LLM 适配、工具系统
rust-agent-middlewares/  中间件：文件系统、终端、HITL、子 Agent
rust-agent-tui/          交互式 TUI
```

## License

MIT
