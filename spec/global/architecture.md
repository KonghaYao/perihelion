# 架构全景

## 系统组件

| 组件 | 类型 | 职责 |
|------|------|------|
| `peri-agent` | 核心库 | ReAct 执行器、LLM 适配层、Middleware trait、工具系统、消息类型、线程持久化（SQLite + Filesystem）、遥测（OTel） |
| `peri-middlewares` | 中间件库 | 文件系统、终端、HITL（含 SharedPermissionMode/Auto 分类器）、SubAgent、Skills、SkillPreload、AgentsMd、AgentDefine、Todo、CronMiddleware、MCP（Client 连接池、OAuth 2.0、工具桥接）、grep 进程内搜索 等具体实现 |
| `peri-acp` | ACP 服务层 | Agent Client Protocol 实现：Session 管理、Agent 构建（Middleware Chain 组装）、事件映射（ExecutorEvent→SessionNotification）、HITL/AskUser 桥接（AcpTransportBroker）、Langfuse 追踪、Hooks、LSP、系统提示词、Provider/Model 解析、上下文压缩执行 |
| `peri-tui` | 可执行文件 | 基于 ratatui 的交互式 TUI，通过 ACP 协议与 Agent 通信（AcpTuiClient），MessagePipeline 消费 SessionNotification，异步渲染、多会话管理、HITL/AskUser 弹窗、配置面板 |
| `peri-widgets` | Widget 库 | 独立 UI 组件库，仅依赖 ratatui + pulldown-cmark |

## Workspace 依赖关系

```
peri-agent           ← 零内部依赖，纯核心框架
    ↑
peri-middlewares      ← 依赖 peri-agent
    ↑
peri-acp              ← 依赖 peri-agent + peri-middlewares + peri-lsp + langfuse-client
    ↑
peri-widgets          ← 零内部依赖，仅依赖 ratatui + pulldown-cmark
    ↑
peri-tui              ← 依赖 peri-widgets + peri-acp
```

## 模块划分

### peri-agent 内部模块

```
src/
├── agent/
│   ├── react.rs          — ReAct 循环主体：max_iterations(50)、工具并发分发、事件发射
│   ├── executor.rs       — ReActAgent：组装 middleware chain + LLM + 取消令牌
│   ├── state.rs          — AgentState：消息历史（只追加）、cwd、工具注册表
│   ├── token.rs          — TokenTracker / ContextBudget（Token 累积追踪与上下文窗口预算）
│   ├── compact/          — Micro/Full Compact 实现
│   │   ├── config.rs     — CompactConfig（阈值、策略配置）
│   │   ├── micro.rs      — Micro-compact：清除可压缩工具结果/图片/文档
│   │   ├── full.rs       — Full Compact：LLM 生成 9 段摘要替换历史
│   │   ├── re_inject.rs  — 重新注入最近文件 + Skills
│   │   └── invariant.rs  — Compact 不变量校验
│   └── events.rs         — AgentEvent 枚举（11 种变体，见下方事件系统）
├── llm/
│   ├── adapter.rs        — BaseModel trait 定义（invoke → LlmResponse）
│   ├── anthropic.rs      — ChatAnthropic：Prompt Cache + Extended Thinking + system blocks
│   ├── openai.rs         — ChatOpenAI：SSE streaming + reasoning_content（DeepSeek-R1/o系列）
│   ├── react_adapter.rs  — BaseModelReactLLM：BaseModel → ReactLLM trait 适配
│   ├── retry.rs          — RetryableLLM<L> 装饰器（指数退避+随机抖动）
│   └── types.rs          — TokenUsage、LlmRequest/LlmResponse 类型定义
├── middleware/
│   ├── trait.rs          — Middleware<S> trait（5 个钩子：before/after_agent、before/after_tool、collect_tools）
│   ├── chain.rs          — MiddlewareChain：按注册顺序执行所有中间件
│   └── base.rs           — LoggingMiddleware / MetricsMiddleware / NoopMiddleware
├── messages/
│   ├── message.rs        — BaseMessage（Human/Ai/System/Tool）、MessageContent、MessageId（UUID v7）
│   ├── content.rs        — ContentBlock 7 种变体（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）
│   └── adapters/         — MessageAdapter trait：OpenAiAdapter / AnthropicAdapter 双向转换
├── tools/
│   ├── mod.rs            — BaseTool trait + ToolDefinition（JSON Schema）
│   └── provider.rs       — ToolProvider trait（批量动态提供工具）
├── thread/
│   ├── store.rs          — ThreadStore trait（异步，list/get/create/append/delete）
│   ├── sqlite_store.rs   — SqliteThreadStore：sqlx SqlitePool 连接池，WAL 模式，原生 async
│   ├── filesystem.rs     — FilesystemThreadStore：文件系统持久化备选实现
│   └── types.rs          — ThreadId（UUID v7）、ThreadMeta
├── hitl/                 — HitlDecision 枚举（Approve/Edit/Reject/Respond）、HitlHandler trait、BatchItem
├── ask_user/             — AskUserInvoker trait、AskUserBatchRequest、AskUserQuestionData、AskUserOption
├── error.rs              — AgentError / AgentResult 统一错误类型、LlmHttpError（携带 HTTP status code）
└── telemetry/
    ├── subscriber.rs     — tracing-subscriber 初始化（env-filter + fmt + json）
    └── otel.rs           — OpenTelemetry OTLP HTTP 导出，tracing-opentelemetry 桥接
```

### peri-middlewares 内部模块

```
src/
├── middleware/
│   ├── filesystem.rs     — FilesystemMiddleware（提供 6 个工具，见工具清单）
│   ├── terminal.rs       — TerminalMiddleware（bash 工具，120s 超时，跨平台）
│   ├── prepend_system.rs — PrependSystemMiddleware（before_agent 注入 system prompt）
│   └── todo.rs           — TodoMiddleware（after_tool 解析 TodoWrite，推送 channel）
├── mcp/                   — McpMiddleware（config/transport/client/tool_bridge/resource_tool/middleware）
│   ├── config.rs         — 双层配置加载合并（全局 settings.json + 项目 .mcp.json），${VAR} 展开
│   ├── transport.rs      — stdio（子进程）/ StreamableHTTP（远程）传输工厂
│   ├── client.rs         — McpClientPool 连接池管理
│   ├── tool_bridge.rs    — MCP 工具 → BaseTool 桥接
│   ├── resource_tool.rs  — mcp_read_resource 工具
│   └── middleware.rs     — Middleware trait 实现
├── hitl/
│   ├── mod.rs            — HumanInTheLoopMiddleware（before_tool 拦截 + requires_approval 判断）
│   ├── shared_mode.rs    — SharedPermissionMode (Arc<AtomicU8> 无锁共享权限模式)
│   └── auto_classifier.rs — LlmAutoClassifier (Auto 模式分类器)
├── subagent/
│   ├── mod.rs            — SubAgentMiddleware（挂载 Agent 工具 + LLM 工厂 + system builder）
│   ├── tool.rs           — SubAgentTool（读 agent 定义、创建子 Agent、工具过滤/防递归）
│   └── skill_preload.rs  — SkillPreloadMiddleware（before_agent 注入 skill 全文为 fake tool 调用序列）
├── skills/
│   ├── loader.rs         — 多路径扫描（~/.claude/skills/ → skillsDir → ./.claude/skills/），同名先到先得
│   └── mod.rs            — SkillsMiddleware（before_agent prepend 摘要到 system prompt）
├── agents_md.rs          — AgentsMdMiddleware（读 CLAUDE.md / AGENTS.md 注入 system）
├── agent_define.rs       — AgentDefineMiddleware + AgentOverrides（覆盖 model/tone/maxTurns 等）
├── claude_agent_parser.rs — .claude/agents/*.md 文件解析器（YAML frontmatter 提取）
├── ask_user/             — parse_ask_user() 工具输出解析
└── tools/
    ├── filesystem/       — 6 个文件系统工具各自独立文件
    │   ├── read.rs       — ReadFileTool
    │   ├── write.rs      — WriteFileTool
    │   ├── edit.rs       — EditFileTool
    │   ├── glob.rs       — GlobFilesTool
    │   ├── grep.rs       — GrepTool（进程内搜索，grep+grep-regex crate）
    │   └── folder.rs     — FolderOperationsTool
    ├── ask_user_tool.rs  — AskUserTool（oneshot channel 挂起等待用户输入）
    ├── todo.rs           — TodoWriteTool + TodoItem / TodoStatus
    └── mod.rs            — BoxToolWrapper / ArcToolWrapper 适配器
```

### peri-acp 内部模块

```
src/
├── lib.rs                — 模块入口
├── transport/
│   ├── mod.rs            — AcpTransport trait（MpscTransport/StdioTransport）
│   └── event_sink.rs     — EventSink trait + TransportEventSink/StdioEventSink
├── session/
│   ├── mod.rs            — Session 管理（SessionManager + SessionState）
│   ├── executor.rs       — execute_prompt() 共享 Agent 执行管线
│   ├── compact_runner.rs — run_full_compact()/run_micro_compact()
│   └── state_builders.rs — ACP 协议状态构建器（modes/models/configOptions）
├── agent/
│   └── builder.rs        — build_agent()：组装 Middleware Chain + LLM + 工具
├── broker.rs             — AcpTransportBroker（实现 UserInteractionBroker，HITL/AskUser 桥接）
├── dispatch.rs           — ACP 请求分发（session/new/prompt/set_model/set_mode 等）
├── event.rs              — ExecutorEvent → SessionNotification 事件映射
├── langfuse/
│   └── mod.rs            — Langfuse 追踪（Session/Tracer）
├── hooks/
│   └── mod.rs            — Hooks 系统集成
├── lsp/
│   └── mod.rs            — LSP 中间件集成
├── prompt/
│   └── mod.rs            — 系统提示词构建
├── provider/
│   └── mod.rs            — Provider/Model 解析
└── features.rs           — PromptFeatures + GitAttribution
```

### peri-tui 内部模块（更新后）

```
src/
├── main.rs               — 入口：CLI 参数解析、terminal 初始化、事件循环
├── acp_client/
│   └── client.rs         — AcpTuiClient：TUI 端 ACP 封装（new_session/prompt/compact/set_model/cancel 等）
├── acp_server/
│   ├── mod.rs            — ACP Server 配置（SessionState/AcpServerConfig）
│   ├── requests.rs       — handle_request()：处理 session/new/prompt/compact 等 ACP 请求
│   ├── prompt.rs         — TUI 侧 prompt 执行入口，委托 executor::execute_prompt()
│   ├── compact.rs        — 手动 compact 入口
│   └── notify.rs         — 通知推送
├── app/
│   ├── mod.rs            — App 结构体
│   ├── agent.rs          — map_executor_event()：ExecutorEvent → AgentEvent 映射
│   ├── agent_ops/        — Agent 事件处理（acp_bridge/lifecycle/subagent/polling）
│   ├── agent_submit.rs   — submit_message()
│   ├── agent_compact.rs  — TUI 侧 compact UI 处理
│   ├── message_pipeline/ — MessagePipeline 统一消息管线
│   ├── text_selection.rs — TextSelection 鼠标文字选区
│   ├── hitl.rs           — 审批/HITL 弹窗
│   ├── ask_user.rs       — AskUser 弹窗
│   ├── model_panel.rs    — /model 面板
│   ├── provider.rs       — Provider/Model 配置管理
│   ├── thread_ops.rs     — 线程操作
│   └── ...
├── ui/
│   ├── main_ui.rs        — 主渲染入口
│   ├── message_render.rs — 消息行渲染
│   ├── message_view.rs   — MessageViewModel
│   └── render_thread.rs  — 独立渲染线程
├── sync/                 — 配置同步模块
│   ├── mod.rs            — 入口
│   ├── protocol.rs       — WS 消息协议
│   ├── crypto.rs         — PBKDF2 + AES-256-GCM
│   ├── packer.rs         — SyncPackage 序列化
│   ├── scanner.rs        — 配置扫描
│   ├── writer.rs         — 文件写入 + 路径防护
│   ├── sender.rs         — sender 模式
│   ├── receiver.rs       — receiver 模式
│   └── ui.rs             — CLI 交互
├── config/
│   ├── store.rs          — PeriConfig
│   └── types.rs          — 配置类型
├── command/
│   ├── mod.rs            — CommandRegistry
│   └── ...
├── event.rs              — crossterm 事件适配
└── prompt.rs             — 系统提示词构建（已废弃，迁移到 peri-acp）
```

## 事件系统

### AgentEvent（核心层，11 种变体）

| 事件 | 说明 | 携带信息 |
|------|------|----------|
| `AiReasoning` | AI 推理/CoT 内容 | reasoning_text |
| `TextChunk` | LLM 最终文字输出 | message_id + chunk |
| `ToolStart` | 工具调用开始 | message_id + tool_call_id + name + input |
| `ToolEnd` | 工具调用结束 | message_id + tool_call_id + name + output + is_error |
| `StepDone` | 一轮 ReAct 完成 | step 序号 |
| `StateSnapshot` | 完整消息快照 | Vec\<BaseMessage\>（用于持久化） |
| `MessageAdded` | 增量消息 | 单条 BaseMessage（用于持久化和遥测） |
| `LlmCallStart` | LLM 调用开始 | step + messages 快照 + tools 定义（Langfuse） |
| `LlmCallEnd` | LLM 调用结束 | step + model + output + TokenUsage（Langfuse） |
| `LlmRetrying` | LLM 重试中 | attempt, max_attempts, delay_ms, error |
| `BackgroundTaskCompleted` | 后台 agent 任务完成 | task_id, agent_name, success, output, tool_calls_count, duration_ms |

### TUI AgentEvent（应用层，扩展变体）

在核心事件基础上增加：`Done` / `Error` / `ApprovalNeeded` / `AskUserBatch` — 用于驱动 TUI 状态机。

## 数据流

### ReAct 循环（核心执行路径）

```
AgentInput（用户消息）
  ↓
state.add_message(Human)
  ↓
chain.collect_tools(cwd)        ← 所有 ToolProvider 合并工具集，手动注册优先
  ↓
chain.before_agent(state)       ← AgentDefine → AgentsMd → Skills → SkillPreload → PrependSystem
  ↓
┌─── ReAct 循环（max 50 次）──────────────────────────────────┐
│  emit(LlmCallStart{step, messages, tools})                   │
│  llm.generate_reasoning(messages, tools)                     │
│  emit(LlmCallEnd{step, model, output, usage})                │
│    ↓ stop_reason==ToolUse                                    │
│  state.add_message(Ai{tool_calls})                           │
│  emit(MessageAdded(Ai))                                      │
│  for each tool_call (并发 join_all):                         │
│    chain.before_tool()  ← HITL 可能在此阻塞等待审批          │
│    emit(ToolStart{...})                                      │
│    tool.invoke(input)   ← AskUser 可能在此阻塞等待输入       │
│    emit(ToolEnd{...})                                        │
│    chain.after_tool()   ← TodoMiddleware 解析结果             │
│    state.add_message(Tool{result})                           │
│    emit(MessageAdded(Tool))                                  │
│    ↓ stop_reason==EndTurn                                    │
│  emit(TextChunk) → 最终答案                                  │
│  emit(StateSnapshot) → 持久化                                │
└──────────────────────────────────────────────────────────────┘
  ↓
chain.after_agent(state, output)
  ↓
AgentOutput（最终结果）
```

### TUI/ACP 通信

```
TUI 路径:
  TUI 输入 → AcpTuiClient.new_session() / .prompt()
           → MpscClientTransport.send_request/notification()
           → MpscServerTransport.recv() (ACP Server, tokio::spawn)
           → acp_server::requests::handle_request() → executor::execute_prompt()
           → build_agent() → agent.execute()
           → ExecutorEvent → TransportEventSink.push_event()
             → peri/agent_event (TUI) + session/update (标准ACP)
           → AcpTuiClient.pump_notifications() → AgentEvent
           → MessagePipeline → View Models → Render

Stdio 路径:
  SDK on_receive_request("session/prompt")
    → executor::execute_prompt() + StdioEventSink
    → ExecutorEvent → SessionNotification → stdout JSON-RPC
```

### Langfuse 追踪层次

```
LangfuseSession（Thread 级别，跨多轮复用）
  └─ LangfuseTracer（Turn 级别，每次 submit_message 创建）
       └─ Trace（trace_id = turn UUID）
            └─ Span: "agent"（agent_span_id）
                 ├─ Generation: "llm-step-{n}"（LlmCallStart → LlmCallEnd）
                 │    └─ input: messages 快照, output: LLM 回复, usage: token 统计
                 ├─ Span: "tools-batch-{n}"（工具批次）
                 │    ├─ Span: "tool:{name}"（ToolStart → ToolEnd）
                 │    ├─ Span: "tool:{name}"
                 │    └─ ...
                 └─ Generation: "llm-step-{n+1}"
                      └─ ...
```

### 上下文压缩流程

```
LlmCallEnd 携带 usage
  → TokenTracker.accumulate()
  → context_usage_percent() > threshold
  → Micro-compact: 清除可压缩工具结果/图片/文档
  → Full Compact: LLM 生成 9 段摘要替换历史
  → re_inject: 重新注入最近文件 + Skills
```

### 配置同步流程

```
Sender                           Relay                          Receiver
  │                                │                                │
  │── request_pair ───────────────►│                                │
  │◄── pair_created("482917") ─────│                                │
  │   显示: 配对码 482917          │                                │
  │                                │◄── join_pair("482917") ────────│
  │◄── pair_joined ────────────────│─── pair_joined ───────────────►│
  │                                │                                │
  │   (等待选择)                    │                                │
  │                                │◄── sync_config({items}) ───────│
  │◄── sync_config({items}) ───────│   receiver 选择同步项 → confirm │
  │                                │                                │
  │   展示传输清单 → 打包加密        │                                │
  │── data_chunk(encrypted) ──────►│── data_chunk(encrypted) ──────►│
  │── data_chunk(encrypted) ──────►│── data_chunk(encrypted) ──────►│
  │── transfer_complete ──────────►│── transfer_complete ──────────►│
  │   ✅ 传输完成                   │              解密 → 解压 → 写入  │
```

Relay Server 为 Hono.js + Cloudflare Durable Objects 无状态密文转发，配对码 6 位数字/5 分钟过期/一次性使用。同步客户端通过 `peri sync sender/receiver` 子命令交互，使用 crossterm CLI 界面。

## 中间件链执行顺序

中间件按注册顺序执行，典型组装顺序：

```
主 Agent（peri-tui 组装）：
1. AgentDefineMiddleware      ← 解析 agent 定义，设置 model/maxTurns 等覆盖
2. AgentsMdMiddleware         ← 读 CLAUDE.md/AGENTS.md 注入 system
3. SkillsMiddleware           ← 扫描 Skills 目录，摘要注入 system
4. SkillPreloadMiddleware     ← 消息含 #skill-name 时注入 skill 全文（fake tool 序列）
5. FilesystemMiddleware       ← 提供 6 个文件系统工具
6. TerminalMiddleware         ← 提供 bash 工具
7. TodoMiddleware             ← after_tool 解析 TodoWrite 结果
8. CronMiddleware             ← cron_register/cron_list/cron_remove
9. HumanInTheLoopMiddleware   ← before_tool 拦截敏感工具
10. SubAgentMiddleware         ← 提供 Agent 工具（支持 fork/normal/background 三路径）
11. McpMiddleware             ← MCP 工具和资源注入（仅 pool 初始化成功时注册）
[ReActAgent.with_system_prompt()] ← system prompt 固定在 run_before_agent 之后 prepend，不依赖中间件顺序

子 Agent（SubAgentTool 内部组装）：
1. AgentsMdMiddleware
2. SkillsMiddleware
3. SkillPreloadMiddleware     ← 读取 agent 定义 frontmatter.skills 列表
4. TodoMiddleware
5. PrependSystemMiddleware    ← 子 agent 仍使用中间件方式（动态 system builder）
```

手动注册工具（`register_tool`）优先级最高，覆盖同名中间件工具。

## 外部集成

| 外部服务 | 协议 | 认证 | 端点 |
|---------|------|------|------|
| Anthropic API | HTTPS REST + SSE | `ANTHROPIC_API_KEY` header | `https://api.anthropic.com/v1/messages` |
| OpenAI 兼容 | HTTPS REST + SSE | `OPENAI_API_KEY` bearer | `OPENAI_BASE_URL` 环境变量 |
| SQLite | 本地文件 | — | `~/.peri/threads/threads.db` |
| OpenTelemetry Collector | HTTP OTLP Proto | — | `OTEL_EXPORTER_OTLP_ENDPOINT` |
| Langfuse | HTTPS REST | `LANGFUSE_PUBLIC_KEY` + `LANGFUSE_SECRET_KEY` | `LANGFUSE_HOST`（默认 cloud） |
| Relay Server (Sync) | WebSocket | 配对码（PBKDF2 派生 AES 密钥） | `ws://localhost:8080`（可配置） |

## 部署拓扑

**标准模式（本地 TUI）：**

```
用户终端
  └─ cargo run -p peri-tui
       ├─ 直接调用 Anthropic/OpenAI API（reqwest HTTP）
       ├─ 读写本地文件系统（FilesystemMiddleware）
       ├─ 执行 bash 命令（TerminalMiddleware）
       ├─ 写入 ~/.peri/threads/threads.db（SQLite WAL）
       └─ 上报 Langfuse（可选，环境变量控制）
```

**可观测性（可选）：**

```
peri-agent（tracing spans）
  ├─ opentelemetry-otlp HTTP → Jaeger / OTLP Collector
  └─ Langfuse（TUI 层 LangfuseTracer → Langfuse API）
       └─ Trace > Span > Generation 三级层次
```

---
*最后更新: 2026-05-20 — 由 feature_2026-05-18_F001_acp-tui-separation 和 feature_2026-05-17_F001_config-sync 归档时更新*
