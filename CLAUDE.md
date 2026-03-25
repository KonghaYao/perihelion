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
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run -p rust-agent-tui
# Jaeger UI: http://localhost:16686
```

## 数据流

### ReAct 循环（rust-create-agent）

```
AgentInput
  └─ state.add_message(Human)
  └─ chain.collect_tools(cwd)        # ToolProvider + 中间件工具合并，手动注册的优先级最高
  └─ chain.before_agent(state)       # 按注册顺序：AgentsMd→AgentDefine→Skills→Filesystem→Terminal→Todo→HITL→SubAgent→PrependSystem
                                     #   AgentsMd/Skills/SubAgent 在 state 头部 prepend_message(System)
                                     #   PrependSystem 最后注册、最后 prepend → 系统提示词位于消息列表最前
  └─ loop(max_iterations=50):
      └─ llm.generate_reasoning(state.messages, tools)
      │    └─ BaseModel.invoke(LlmRequest{messages, tools, system})
      │    └─ stop_reason==ToolUse  → Reasoning{tool_calls}
      │       stop_reason==EndTurn  → Reasoning{final_answer}
      │
      ├─ [有工具调用]:
      │   └─ state.add_message(Ai{tool_calls})
      │   └─ for each tool_call:
      │       └─ chain.before_tool()   # HITL 在此拦截：requires_approval? → handler.request_approval()
      │       └─ tool.invoke(input)    # BaseTool::invoke，ask_user 在此挂起等待 oneshot 回复
      │       └─ chain.after_tool()   # TodoMiddleware 在此解析 todo_write 结果，推送 channel
      │       └─ emit(ToolStart/ToolEnd)
      │       └─ state.add_message(Tool{result})
      │
      └─ [最终回答]:
          └─ emit(TextChunk(answer))
          └─ chain.after_agent(state, output) → AgentOutput
```

### TUI 异步通信（rust-agent-tui）

```
submit_message()
  ├─ mpsc(32): AgentEvent channel ──→ agent task
  │                                       └─ run_universal_agent() 产生事件
  │                                       └─ emit → tx.try_send(AgentEvent)
  │  ← poll_agent() 每帧 try_recv ←──────
  │       ToolCall/AssistantChunk → 追加 messages[]
  │       ApprovalNeeded          → app.hitl_prompt = Some(...)  [break, 等用户操作]
  │       AskUserBatch            → app.ask_user_prompt = Some(...) [break, 等用户操作]
  │       Done/Error              → set_loading(false), agent_rx=None
  │
  └─ mpsc(4): ApprovalEvent channel ──→ 转发 task
       ApprovalEvent::Batch        → YOLO: 直接 response_tx.send(Approve×N)
                                     非YOLO: tx.send(AgentEvent::ApprovalNeeded)
       ApprovalEvent::AskUserBatch → tx.send(AgentEvent::AskUserBatch)  [始终转发]

用户操作弹窗后:
  hitl_confirm()     → response_tx.send(decisions)   → HITL before_tool 的 oneshot 解除阻塞
  ask_user_confirm() → response_tx.send(answers)     → AskUserTool::invoke 的 oneshot 解除阻塞
```

### 消息类型

`BaseMessage` 四种变体（`Human/Ai/System/Tool`），内容统一用 `MessageContent`（纯文本 or `ContentBlock[]` or 原生 JSON）。

`ContentBlock` 完整变体：

| 变体 | 说明 |
|------|------|
| `Text` | 纯文本 |
| `Image` | 多模态图片（Base64 或 URL） |
| `Document` | 文档（Anthropic Documents beta） |
| `ToolUse` | AI 发起的工具调用（id/name/input） |
| `ToolResult` | 工具执行结果（tool_use_id/content/is_error） |
| `Reasoning` | 推理/CoT（支持 extended thinking 的 signature 缓存校验） |
| `Unknown` | 原生 block 透传，保证向前兼容 |

`Ai` 变体同时保存 `tool_calls: Vec<ToolCallRequest>`，与 `ContentBlock::ToolUse` 双写保持一致，`ai_from_blocks()` 自动同步。

### LLM 适配层

`BaseModel` trait（OpenAI/Anthropic 实现）→ `BaseModelReactLLM`（适配为 `ReactLLM`）。

| | OpenAI | Anthropic |
|---|---|---|
| system | 转为 `System` 角色消息 prepend | 提取到顶层 `system` 字段（blocks 格式） |
| 工具格式 | `type:"function"` + `function.arguments` | `type:"tool_use"` + `input_schema` |
| 推理内容 | `message.reasoning_content`（deepseek-r1/o系列） | `Reasoning` ContentBlock |
| Prompt Cache | — | 默认开启，最后消息末尾加 `cache_control:ephemeral` |
| 扩展思考 | — | `.with_extended_thinking(budget_tokens)`（3.7+） |

tool result 消息：Anthropic 要求合并到前一条 user 消息的 content blocks；OpenAI 作为独立 tool 角色消息发送。

测试用 `MockLLM::tool_then_answer()` 按脚本回放推理，无需真实 API。

### HITL 决策

`HitlDecision` 四种结果：`Approve` / `Edit(new_input)` / `Reject` → `ToolRejected` 错误 / `Respond(msg)` → `ToolRejected`（向 LLM 回复原因）。

默认需审批工具：`bash`、`write_*`、`edit_*`、`delete_*`、`rm_*`、`folder_operations`。只读工具（`read_file`、`glob_files`、`search_files_rg`）无需审批。

### Skills 搜索顺序

`~/.claude/skills/` → `skillsDir`（`~/.zen-code/settings.json`）→ `./.claude/skills/`

同名 skill 以先出现的为准。每个 skill 是一个子目录，内含 `SKILL.md`（YAML frontmatter: `name`, `description`）。

## 工具清单（rust-agent-middlewares）

| 工具 | 来源中间件 | 需 HITL |
|------|-----------|---------|
| `read_file` | FilesystemMiddleware | — |
| `write_file` | FilesystemMiddleware | ✓ |
| `edit_file` | FilesystemMiddleware | ✓ |
| `glob_files` | FilesystemMiddleware | — |
| `search_files_rg` | FilesystemMiddleware | — |
| `folder_operations` | FilesystemMiddleware | ✓ |
| `bash` | TerminalMiddleware | ✓ |
| `todo_write` | TodoMiddleware | — |
| `ask_user` | 手动注册（AskUserTool） | — |
| `launch_agent` | SubAgentMiddleware | — |

`bash` 默认超时 120 秒，超时返回错误。跨平台：Windows 用 `cmd /C`，其他用 `bash -c`。

### SubAgents（子 Agent 委派）

`launch_agent` 工具允许 LLM 将子任务委派给 `.claude/agents/{agent_id}.md` 定义的专门 agent 执行。

**工具参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| `agent_id` | string（必填） | agent 定义文件名（不含 `.md`），如 `code-reviewer` |
| `task` | string（必填） | 委派给子 agent 的任务描述 |
| `cwd` | string（可选） | 子 agent 工作目录，默认继承父 agent cwd |

**工具过滤规则：**

- `tools` 字段为空 → 子 agent 继承所有父工具（但始终排除 `launch_agent` 自身，防递归）
- `tools` 字段有值 → 仅保留名称在允许列表中的工具
- `disallowedTools` 字段 → 从结果中额外排除指定工具

**返回值格式：**

子 agent 执行结果以字符串形式返回给父 agent 作为工具调用结果：
- 无工具调用：直接返回最终回答文本
- 有工具调用：`[子 agent 执行了 N 个工具调用: tool1, tool2]\n\n最终回答`（中间结果舍弃，避免 token 膨胀）

**系统提示词注入：**

`SubAgentMiddleware` 支持 `.with_system_builder(builder)` 设置系统提示构建器，签名为 `Fn(Option<&AgentOverrides>, &str) -> String`。调用 `launch_agent` 时，工具自动：
1. `AgentDefineMiddleware::load_overrides(cwd, agent_id)` 加载 agent 的 persona/tone/proactiveness
2. 调用 builder 构建完整系统提示（含 tone 等）
3. 通过 `PrependSystemMiddleware` 注入到子 agent 的 state 消息中（Langfuse 可见）

## TUI 命令

输入 `/` 前缀触发，支持前缀唯一匹配（如 `/m` 匹配 `/model`）：

| 命令 | 说明 |
|------|------|
| `/model` | 打开 Provider/Model 配置面板（增删改，写入 `~/.zen-code/settings.json`） |
| `/model <alias>` | 直接切换激活别名（`opus` / `sonnet` / `haiku`），保存并更新状态栏，无需打开面板 |
| `/history` | 打开历史对话浏览面板（`j/k` 或 `↑↓` 导航，`d` 删除，`Enter` 打开，`Esc` 新建） |
| `/clear` | 清空当前消息列表 |
| `/help` | 列出所有命令 |

输入 `#` 前缀触发 Skills 浮层，`Tab` 导航，`Enter` 补全为 `#skill-name `。

## TUI Headless 测试模式

`rust-agent-tui` 支持无真实终端的 headless 集成测试，渲染管道（`main_ui::render`）与生产代码完全一致。

**入口：** `App::new_headless(width, height)` — 仅在 `#[cfg(test)]` 或 `feature = "headless"` 下编译。

```rust
#[tokio::test]
async fn test_example() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 注意：必须在发送事件前注册监听，避免错过通知
    let notified = handle.render_notify.notified();

    // 注入 AgentEvent（复用与生产相同的 handle_agent_event 路径）
    app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    notified.await;  // 等待渲染线程处理完成，零 sleep

    // 走完整 main_ui draw 路径
    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

    // 断言屏幕内容
    assert!(handle.contains("Hello"));
    let lines: Vec<String> = handle.snapshot();  // 每行去尾空格的纯文本
}
```

**关键注意事项：**

- `notified()` 必须在 `process_pending_events()` **之前**调用，否则可能错过通知
- `AssistantChunk` 事件会发送 2 个 `RenderEvent`（`AddMessage` + `AppendChunk`），需等待 2 次通知
- CJK 字符在 `TestBackend` buffer 中有宽字符填充（"你好" → "你 好"），`contains()` 断言应使用 ASCII 内容
- 测试文件位于 `rust-agent-tui/src/ui/headless.rs` 的 `#[cfg(test)] mod tests`（bin crate 不支持 `tests/` 目录集成测试）

**运行测试：**

```bash
cargo test -p rust-agent-tui              # 全量（含 headless 测试）
cargo test -p rust-agent-tui <test_name> -- --nocapture  # 单个测试
```

## 关键模式

```rust
// 组装 agent（系统提示词通过 PrependSystemMiddleware 注入，使 Langfuse 可见）
ReActAgent::new(BaseModelReactLLM::new(model))
    .max_iterations(50)
    .add_middleware(Box::new(FilesystemMiddleware::new()))       // collect_tools 自动提供工具
    .add_middleware(Box::new(PrependSystemMiddleware::new(prompt))) // 最后注册 → 提示词位于消息列表最前
    .register_tool(Box::new(AskUserTool::new(invoker)))         // 手动注册，优先级最高
    .with_event_handler(Arc::new(FnEventHandler(move |ev| { tx.try_send(ev); })))
    .execute(AgentInput::text(input), &mut AgentState::new(cwd))
```

**自定义工具**：实现 `BaseTool`（`name/description/parameters/async invoke`），`register_tool` 注册或 `ToolProvider` 批量提供。

**自定义中间件**：实现 `Middleware<S: State>`，只覆写需要的钩子，其余默认 no-op。`collect_tools(cwd)` 可动态按工作目录返回工具列表。

**SubAgent 委派**：

```rust
// 构建父工具集（Arc 共享，传给子 agent 使用）
let parent_tools: Arc<Vec<Arc<dyn BaseTool>>> = Arc::new(
    FilesystemMiddleware::new().tools(cwd)
        .into_iter()
        .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
        .collect()
);
// LLM 工厂：每次为子 agent 创建裸 LLM（不设 system，由 PrependSystemMiddleware 注入）
let llm_factory = Arc::new(move || {
    Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
});
// 系统提示构建器：根据 agent overrides 构建完整系统提示（含 tone/proactiveness）
let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
    build_system_prompt(overrides, cwd)
});
// 挂载中间件，LLM 即可调用 launch_agent 工具
ReActAgent::new(llm)
    .add_middleware(Box::new(
        SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
            .with_system_builder(system_builder)  // 子 agent 系统提示 Langfuse 可见
    ))
```

agent 定义文件放在 `.claude/agents/{agent_id}.md`，YAML frontmatter 支持 `tools`、`disallowedTools`、`maxTurns`、`description`。

**System prompt**：模板在 `rust-agent-tui/prompts/system.md`，`{{cwd}}` 占位符在运行时替换。系统提示通过 `PrependSystemMiddleware`（注册在链尾）注入 state，确保位于所有其他 System 消息之前，且在 Langfuse 等追踪工具中可见。

**Thread 持久化**：`SqliteThreadStore` 实现 `ThreadStore` trait，数据库路径 `~/.zen-core/threads/threads.db`（WAL 模式）。持久化由 `StateSnapshot` 事件驱动增量写入，用户消息在发送时立即持久化。`parking_lot::Mutex<Connection>` 串行化写操作，`append_messages` 在事务内执行保证 crash-safe。

**消息格式适配**：`MessageAdapter` trait（`rust-create-agent/src/messages/adapters/`）提供 `BaseMessage` 与 Provider 原生 JSON 的双向转换。`OpenAiAdapter` 和 `AnthropicAdapter` 两个实现。

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
| `OTEL_EXPORTER_OTLP_ENDPOINT` | 启用 OTLP 导出（设置即生效） |

`.env` 文件已 gitignore，本地开发配置在 `rust-agent-tui/.env`。
