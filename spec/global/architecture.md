# 架构全景

![系统架构](./images/03-system-architecture.png)

## 系统组件

| 组件 | 类型 | 职责 |
|------|------|------|
| `rust-create-agent` | 核心库 | ReAct 执行器、LLM 适配层、Middleware trait、工具系统、消息类型、线程持久化、遥测 |
| `rust-agent-middlewares` | 中间件库 | 文件系统、终端、HITL、SubAgent、Skills、AgentsMd、Todo、AskUser 等具体实现 |
| `rust-agent-tui` | 可执行文件 | 基于 ratatui 的交互式 TUI，异步渲染、多会话管理、HITL 弹窗、配置面板 |
| `rust-relay-server` | 可执行文件 + 客户端库 | axum WebSocket 中继服务，支持远程控制本地 Agent；client feature 供 TUI 集成 |

## 模块划分

### rust-create-agent 内部模块

```
src/
├── agent/
│   ├── react.rs        — ReActAgent 主体：max_iterations 循环、工具分发、事件发射
│   ├── executor.rs     — AgentExecutor：组装 middleware chain + LLM 调用入口
│   ├── state.rs        — AgentState：消息历史、cwd、工具注册表
│   └── events.rs       — AgentEvent 枚举（ToolStart/ToolEnd/TextChunk/Done 等）
├── llm/
│   ├── anthropic.rs    — Anthropic API 适配（Prompt Cache, Extended Thinking）
│   ├── openai.rs       — OpenAI 兼容适配（streaming, reasoning_content）
│   ├── react_adapter.rs — BaseModelReactLLM: BaseModel → ReactLLM 适配
│   └── adapter.rs      — BaseModel trait 定义
├── middleware/
│   ├── trait.rs        — Middleware<S> trait（before_agent/after_agent/before_tool/after_tool/collect_tools）
│   ├── chain.rs        — MiddlewareChain：顺序执行所有中间件钩子
│   └── base.rs         — 默认 no-op 实现
├── messages/
│   ├── message.rs      — BaseMessage（Human/Ai/System/Tool）、MessageContent
│   ├── content.rs      — ContentBlock 完整变体（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）
│   └── adapters/       — MessageAdapter trait，OpenAiAdapter / AnthropicAdapter 双向转换
├── thread/
│   ├── sqlite_store.rs — SqliteThreadStore：WAL 模式，parking_lot::Mutex 串行写
│   ├── store.rs        — ThreadStore trait
│   └── types.rs        — Thread、StateSnapshot 等类型
├── hitl/               — HitlDecision 枚举（Approve/Edit/Reject/Respond）、HitlHandler trait
├── ask_user/           — AskUserInvoker trait、AskUserBatch 类型
└── telemetry/          — OpenTelemetry 初始化（OTLP HTTP 导出），tracing-opentelemetry 桥接
```

### rust-agent-middlewares 内部模块

```
src/
├── middleware/
│   ├── filesystem.rs   — FilesystemMiddleware（提供 read/write/edit/glob/search/folder 工具）
│   ├── terminal.rs     — TerminalMiddleware（提供 bash 工具，120s 超时）
│   └── todo.rs         — TodoMiddleware（解析 todo_write 结果，推送 channel）
├── hitl/               — HitlMiddleware（before_tool 拦截，requires_approval 判断）
├── subagent/
│   ├── mod.rs          — SubAgentMiddleware（挂载 launch_agent 工具）
│   └── tool.rs         — launch_agent 工具实现（读 agent 定义、创建子 Agent 实例）
├── skills/
│   ├── loader.rs       — 多路径扫描加载 Skills（~/.claude/skills/ → skillsDir → ./.claude/skills/）
│   └── mod.rs          — SkillsMiddleware（before_agent prepend system prompt）
├── agents_md.rs        — AgentsMdMiddleware（读 CLAUDE.md/AGENTS.md 注入 system）
├── agent_define.rs     — Agent 定义文件解析（YAML frontmatter: tools/disallowedTools/maxTurns）
├── claude_agent_parser.rs — .claude/agents/*.md 文件解析器
└── tools/              — 具体工具实现（FilesystemTools, AskUserTool, TodoTool）
```

### rust-agent-tui 内部模块

```
src/
├── app/
│   ├── mod.rs          — App 状态中枢：消息列表、loading、hitl_prompt、ask_user_prompt
│   ├── agent.rs        — run_universal_agent()：启动 Agent task，处理 AgentEvent
│   ├── hitl.rs         — hitl_confirm()：发送 decisions 到 response_tx
│   ├── model_panel.rs  — /model 配置面板状态
│   └── agent_panel.rs  — /history 历史浏览面板状态
├── ui/
│   ├── main_ui.rs      — 主界面渲染（ratatui draw 函数）
│   ├── message_render.rs — 消息渲染（Markdown 支持、工具调用展示、颜色主题）
│   ├── render_thread.rs — 独立渲染线程（Notify 驱动，zero-sleep）
│   └── headless.rs     — Headless 测试模式（TestBackend + render_notify）
├── config/             — AppConfig：~/.zen-code/settings.json 读写，Provider/Model 管理
├── thread/             — 会话历史浏览、线程加载
└── command/            — /model /history /clear /help /compact 命令处理
```

## 数据流

![数据流](./images/04-data-flow.png)

```
AgentInput（用户消息）
  ↓
state.add_message(Human)
  ↓
chain.collect_tools(cwd)        ← 所有 ToolProvider 合并工具集，手动注册优先
  ↓
chain.before_agent(state)       ← AgentsMd → Skills（prepend System prompt）
  ↓
┌─── ReAct 循环（max 50 次）──────────────────────────┐
│  llm.generate_reasoning(messages, tools)             │
│    ↓ stop_reason==ToolUse                            │
│  state.add_message(Ai{tool_calls})                   │
│  for each tool_call:                                 │
│    chain.before_tool()  ← HITL 可能在此阻塞等待审批  │
│    tool.invoke(input)   ← AskUser 可能在此阻塞等待输入│
│    chain.after_tool()   ← TodoMiddleware 解析结果    │
│    state.add_message(Tool{result})                   │
│    ↓ stop_reason==EndTurn                            │
│  emit(TextChunk) → 最终答案                          │
└──────────────────────────────────────────────────────┘
  ↓
chain.after_agent(state, output)
  ↓
AgentOutput（最终结果）
```

**TUI 异步通信通道：**
- `mpsc(32)` AgentEvent 通道：Agent task → TUI poll 帧
- `mpsc(4)` ApprovalEvent 通道：HITL/AskUser 事件转发
- `oneshot` 通道：HITL 决策 / AskUser 回复的单次响应

## 外部集成

| 外部服务 | 协议 | 认证 | 端点 |
|---------|------|------|------|
| Anthropic API | HTTPS REST + SSE | `ANTHROPIC_API_KEY` header | `https://api.anthropic.com/v1/messages` |
| OpenAI 兼容 | HTTPS REST + SSE | `OPENAI_API_KEY` bearer | `OPENAI_BASE_URL` 环境变量 |
| Relay Server | WebSocket (ws://) | 无（本地） | `axum` 默认端口，静态文件内嵌 |
| SQLite | 本地文件 | — | `~/.zen-core/threads/threads.db` |
| OpenTelemetry Collector | HTTP OTLP Proto | — | `OTEL_EXPORTER_OTLP_ENDPOINT` |

## 部署拓扑

**标准模式（本地 TUI）：**

```
用户终端
  └─ cargo run -p rust-agent-tui
       ├─ 直接调用 Anthropic/OpenAI API（reqwest HTTP）
       ├─ 读写本地文件系统（FilesystemMiddleware）
       ├─ 执行 bash 命令（TerminalMiddleware）
       └─ 写入 ~/.zen-core/threads/threads.db（SQLite）
```

**远程控制模式（可选）：**

```
用户浏览器 / 远端客户端
  └─ WebSocket 连接 rust-relay-server
       └─ 中继 AgentEvent / ApprovalEvent
            └─ 本地运行的 rust-agent-tui（client feature 集成）
```

**可观测性（可选）：**

```
rust-create-agent（tracing spans）
  └─ opentelemetry-otlp HTTP 导出
       └─ Jaeger / OTLP Collector（docker-compose.otel.yml）
```

---
*最后更新: 2026-03-24 — 初始化生成*
