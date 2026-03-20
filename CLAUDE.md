# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

这是一个 Rust Agent 框架项目，包含三个 Workspace Crate：

- **`rust-create-agent`**：核心框架，实现 ReAct 循环与可组合中间件系统，与 TypeScript 端的 `@langgraph-js/standard-agent` 概念对齐
- **`rust-agent-middlewares`**：具体中间件实现（文件系统操作、终端执行、Skills 摘要、Human-in-the-Loop 等）
- **`rust-agent-tui`**：交互式 TUI 终端 playground 应用

## 开发命令

```bash
# 构建
cargo build                          # 构建所有 crate
cargo build -p rust-create-agent      # 构建指定 crate

# 运行
cargo run -p rust-agent-tui           # 运行 TUI 应用
cargo run -p rust-agent-tui --features otel  # 启用 OTLP 导出

# 测试
cargo test                            # 运行所有测试
cargo test -p rust-create-agent --lib # 运行特定 crate 的单元测试

# 单文件/单测试运行
cargo test -p rust-create-agent --lib -- test_function_name  # 运行匹配名称的测试

# 启用 OpenTelemetry（需先启动 Jaeger）
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run -p rust-agent-tui --features otel

# 启动本地 Jaeger（用于 OTLP 可视化）
docker compose -f docker-compose.otel.yml up -d   # 启动
docker compose -f docker-compose.otel.yml down    # 停止
# UI: http://localhost:16686  |  OTLP HTTP: http://localhost:4318
```

## 架构概览

### rust-create-agent（核心框架）

```
src/
├── agent/          # ReActAgent（ReAct 循环执行器）+ 状态管理
├── middleware/     # MiddlewareChain + Middleware trait 定义
├── llm/            # LLM 适配器 trait（ReactLLM）+ OpenAI/Anthropic 实现
├── tools/          # 工具注册与调用系统
├── messages/       # 消息内容类型（ContentBlock、ToolCallRequest 等）
├── telemetry/      # tracing + OpenTelemetry 集成
├── thread/         # 多会话线程管理
├── hitl/           # Human-in-the-Loop 审批系统
├── ask_user/       # 用户交互请求
└── error.rs        # AgentError/AgentResult 统一错误类型
```

核心数据流：`AgentInput` → `ReActAgent`（ReAct 循环）→ 工具调用 → `AgentOutput`

### rust-agent-middlewares（中间件实现）

```
src/
├── middleware/
│   ├── filesystem.rs   # FilesystemMiddleware：文件系统读写编辑
│   ├── terminal.rs      # TerminalMiddleware：shell 命令执行
│   └── todo.rs          # TodoMiddleware：任务状态管理
├── tools/filesystem/    # 具体工具：ReadFile、WriteFile、EditFile、Glob、Grep、Folder 等
├── agents_md.rs         # AgentsMdMiddleware：注入 AGENTS.md/CLAUDE.md 项目指引
├── skills/              # SkillsMiddleware：渐进式 Skills 摘要注入
├── hitl/                # HumanInTheLoopMiddleware：敏感操作需用户审批
└── ask_user/            # AskUserTool：向用户提问的工具
```

### rust-agent-tui（TUI 应用）

```
src/
├── app/          # App 状态机、Agent 管理、HITL 审批面板
├── command/      # 命令处理（help、clear、history、model）
├── config/       # 配置持久化
├── thread/       # 会话线程管理
└── prompt.rs     # System prompt 模板加载
```

入口：`src/main.rs`，使用 ratatui 构建 TUI，支持 `--yolo` / `-y` 参数跳过 HITL 审批。

## 关键模式

### Middleware 生命周期钩子

`before_agent` → `before_tool` → `after_tool` → `after_agent`，出错时触发 `on_error`。

### 中间件注册到执行器

```rust
ReActAgent::new(llm)
    .register_tool(Box::new(my_tool))
    .add_middleware(Box::new(LoggingMiddleware::new()))
    .with_event_handler(Arc::new(handler))
```

### Telemetry 自动埋点

`ReActAgent::execute()` 和每次工具调用均已自动埋点，无需额外代码。`init_tracing()` 需在 `main` 入口调用一次。

### YOLO 模式

通过 `YOLO_MODE=true` 环境变量或 `--yolo` / `-y` 命令行参数启用，跳过 HITL 审批。

## 环境变量

| 变量 | 说明 |
|------|------|
| `OPENAI_API_KEY` | LLM API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE` | 跳过 HITL 审批 |
| `RUST_LOG` | 日志级别（默认 `info`） |
| `RUST_LOG_FORMAT=json` | JSON 格式日志输出 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | 启用 OTLP 导出 |

## 注意事项

- `.env` 文件已被 `.gitignore` 排除，包含敏感信息（API Key），勿提交
- OTEL 功能通过 Cargo feature `otel` 控制，未启用时仅输出到 stdout
- `rust-agent-tui/.env` 是本地开发配置，包含实际的 API Key 和配置
