# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Rust Agent 框架，包含三个 Workspace Crate：

- **`rust-create-agent`**：核心框架——ReAct 循环执行器、Middleware trait、LLM 适配器、工具系统
- **`rust-agent-middlewares`**：具体中间件实现（文件系统、终端、Skills、HITL、ask_user）
- **`rust-agent-tui`**：交互式 TUI playground，基于 ratatui

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p rust-create-agent     # 构建指定 crate
cargo run -p rust-agent-tui          # 运行 TUI
cargo run -p rust-agent-tui -- -y    # YOLO 模式（跳过 HITL 审批）
cargo test                           # 全量测试
cargo test -p rust-create-agent --lib -- test_name  # 运行单个测试

# OpenTelemetry（需先启动 Jaeger）
docker compose -f docker-compose.otel.yml up -d
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run -p rust-agent-tui --features otel
# Jaeger UI: http://localhost:16686
```

## 架构

### 核心执行流程（rust-create-agent）

```
AgentInput
  → ReActAgent::execute()
      → chain.before_agent()         ← 所有中间件按注册顺序执行
      → loop (max_iterations):
          → llm.generate_reasoning() ← 返回 Reasoning { tool_calls / final_answer }
          → chain.before_tool()      ← 可修改工具参数（HITL 在此挂入）
          → tool.invoke()            ← BaseTool trait
          → chain.after_tool()
          → emit(AgentEvent)         ← 发给 UI 层
      → chain.after_agent()
  → AgentOutput
```

**工具优先级**（同名时）：`register_tool()` 手动注册 > `ToolProvider` > 中间件 `collect_tools()`

**LLM 层**：`ReactLLM` trait，实现有 `OpenAI`（兼容所有 OpenAI 格式 API）和 `Anthropic`。`BaseModelReactLLM` 是统一包装层，通过 `.with_system()` 注入系统提示。

**测试用 Mock**：`MockLLM::always_answer()` / `MockLLM::tool_then_answer()` 按脚本回放推理结果。

### 中间件系统（rust-agent-middlewares）

中间件生命周期：`before_agent` → `before_tool` → `after_tool` → `after_agent`，出错时 `on_error`。

| 中间件 | 作用 | 挂入点 |
|--------|------|--------|
| `AgentsMdMiddleware` | 注入 AGENTS.md/CLAUDE.md 内容 | `before_agent` |
| `SkillsMiddleware` | 扫描 skills 目录，注入摘要系统消息 | `before_agent` |
| `FilesystemMiddleware` | 提供文件系统工具 | `collect_tools` |
| `TerminalMiddleware` | 提供 bash 工具 | `collect_tools` |
| `TodoMiddleware` | 任务状态管理，变更时通过 channel 通知 UI | `after_tool` |
| `HumanInTheLoopMiddleware` | 敏感工具调用前请求用户审批 | `before_tool` |

**Skills 搜索顺序**（优先级高到低）：`~/.claude/skills/` → `skillsDir`（~/.zen-code/settings.json）→ `./.claude/skills/`

**HITL**：`HumanInTheLoopMiddleware::from_env()` 检测 `YOLO_MODE`，YOLO 时 `disabled()`（直接放行）。非 YOLO 时通过 `HitlHandler::request_approval_batch()` 挂起等待 UI 回复。

**ask_user**：`AskUserTool`（`BaseTool`）→ `AskUserInvoker` trait → TUI 实现（`TuiAskUserHandler`）。工具调用时阻塞等待 `oneshot` channel 回复，不受 YOLO_MODE 影响。

### TUI 应用（rust-agent-tui）

**事件总线**：`submit_message()` 启动两个异步 channel：
- `mpsc(32)` — `AgentEvent`（工具调用、文字块、完成/错误）→ `poll_agent()` 每帧消费
- `mpsc(4)` — `ApprovalEvent`（HITL 审批、ask_user 提问）→ 转发 task → `AgentEvent`

**弹窗状态机**：
- `hitl_prompt: Option<HitlBatchPrompt>` — 审批弹窗（YOLO 时转发 task 直接 Approve）
- `ask_user_prompt: Option<AskUserBatchPrompt>` — 提问弹窗，多问题 Tab 切换

**配置**：`~/.zen-code/settings.json`，结构 `{ "config": { "provider_id", "model_id", "providers": [...], "skillsDir" } }`。`/model` 命令打开面板在线编辑。

**Skills hint**：输入框键入 `#` 触发浮层，`Tab` 导航，`Enter` 补全为 `#skill-name `；键入 `/` 触发命令浮层。

## 关键模式

**注册中间件**：
```rust
ReActAgent::new(llm)
    .max_iterations(50)
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .register_tool(Box::new(AskUserTool::new(invoker)))
    .with_event_handler(Arc::new(FnEventHandler(move |ev| { ... })))
```

**自定义工具**：实现 `BaseTool` trait（`name` / `description` / `parameters` / `async invoke`），用 `register_tool` 注册，或通过 `ToolProvider` trait 批量提供。

**自定义中间件**：实现 `Middleware<S: State>` trait，只需覆写用到的钩子方法，未覆写的默认 no-op。

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE=true` | 跳过 HITL 审批（不影响 ask_user） |
| `RUST_LOG` | 日志级别（默认 `info`） |
| `RUST_LOG_FORMAT=json` | JSON 格式日志 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | 启用 OTLP 导出（需 `--features otel`） |

`.env` 文件已 gitignore，本地开发配置在 `rust-agent-tui/.env`。
