# 架构全景

## 系统组件

| 组件 | 类型 | 职责 |
|------|------|------|
| `rust-create-agent` | 核心库 | ReAct 执行器、LLM 适配层、Middleware trait、工具系统、消息类型、线程持久化（SQLite + Filesystem）、遥测（OTel） |
| `rust-agent-middlewares` | 中间件库 | 文件系统、终端、HITL（含 SharedPermissionMode/Auto 分类器）、SubAgent、Skills、SkillPreload、AgentsMd、AgentDefine、Todo、PrependSystem、AskUser、CronMiddleware、grep 进程内搜索 等具体实现 |
| `rust-agent-tui` | 可执行文件 | 基于 ratatui 的交互式 TUI，异步渲染、多会话管理、HITL/AskUser 弹窗、配置面板、Langfuse 追踪 |

## Workspace 依赖关系

```
rust-create-agent           ← 零内部依赖，纯核心框架
    ↑
rust-agent-middlewares      ← 依赖 rust-create-agent
    ↑
perihelion-widgets          ← 零内部依赖，仅依赖 ratatui + pulldown-cmark
    ↑
rust-agent-tui              ← 依赖 perihelion-widgets + rust-agent-middlewares
```

## 模块划分

### rust-create-agent 内部模块

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
│   ├── sqlite_store.rs   — SqliteThreadStore：WAL 模式，parking_lot::Mutex 串行写
│   ├── filesystem.rs     — FilesystemThreadStore：文件系统持久化备选实现
│   └── types.rs          — ThreadId（UUID v7）、ThreadMeta
├── hitl/                 — HitlDecision 枚举（Approve/Edit/Reject/Respond）、HitlHandler trait、BatchItem
├── ask_user/             — AskUserInvoker trait、AskUserBatchRequest、AskUserQuestionData、AskUserOption
├── error.rs              — AgentError / AgentResult 统一错误类型、LlmHttpError（携带 HTTP status code）
└── telemetry/
    ├── subscriber.rs     — tracing-subscriber 初始化（env-filter + fmt + json）
    └── otel.rs           — OpenTelemetry OTLP HTTP 导出，tracing-opentelemetry 桥接
```

### rust-agent-middlewares 内部模块

```
src/
├── middleware/
│   ├── filesystem.rs     — FilesystemMiddleware（提供 6 个工具，见工具清单）
│   ├── terminal.rs       — TerminalMiddleware（bash 工具，120s 超时，跨平台）
│   ├── prepend_system.rs — PrependSystemMiddleware（before_agent 注入 system prompt）
│   └── todo.rs           — TodoMiddleware（after_tool 解析 todo_write，推送 channel）
├── hitl/
│   ├── mod.rs            — HumanInTheLoopMiddleware（before_tool 拦截 + requires_approval 判断）
│   ├── shared_mode.rs    — SharedPermissionMode (Arc<AtomicU8> 无锁共享权限模式)
│   └── auto_classifier.rs — LlmAutoClassifier (Auto 模式分类器)
├── subagent/
│   ├── mod.rs            — SubAgentMiddleware（挂载 launch_agent 工具 + LLM 工厂 + system builder）
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

### rust-agent-tui 内部模块

```
src/
├── main.rs               — 入口：CLI 参数解析、terminal 初始化、事件循环、Langfuse flush
├── app/
│   ├── mod.rs            — App 结构体：消息列表、loading、弹窗状态、渲染缓存、Langfuse session
│   ├── agent.rs          — run_universal_agent()：组装 Agent + 中间件链 + event handler
│   ├── events.rs         — TUI 层 AgentEvent（包装核心 AgentEvent + Done/Error/Approval 等 TUI 专有事件）
│   ├── message_pipeline.rs — MessagePipeline 统一消息管线
│   ├── text_selection.rs — TextSelection 鼠标文字选区
│   ├── hitl.rs           — ApprovalEvent / BatchApprovalRequest 定义
│   ├── hitl_prompt.rs    — HitlBatchPrompt 弹窗状态（工具名/参数/选中项/滚动）
│   ├── hitl_ops.rs       — HITL 弹窗操作逻辑（confirm/navigate/edit）
│   ├── ask_user_prompt.rs — AskUserBatchPrompt 弹窗状态
│   ├── ask_user_ops.rs   — AskUser 弹窗操作逻辑
│   ├── model_panel.rs    — /model 面板状态（三 Tab: AliasConfig/Browse/Edit）
│   ├── agent_panel.rs    — /agents 面板状态（SubAgent 定义管理）
│   ├── provider.rs       — Provider/Model 运行时管理
│   ├── tool_display.rs   — 工具调用显示格式化（颜色 + 路径缩短）
│   ├── panel_ops.rs      — 通用面板操作（打开/关闭/导航）
│   ├── thread_ops.rs     — 线程操作（新建/打开/删除会话）
│   ├── agent_ops.rs      — Agent 启动/停止操作
│   ├── hint_ops.rs       — Skills 提示浮层操作（# 触发）
│   └── login_panel.rs    — /login 面板（Provider CRUD）
├── ui/
│   ├── main_ui.rs        — 主 render() 入口：区域布局 + 分发到子组件
│   ├── main_ui/
│   │   ├── status_bar.rs — 底部状态栏（模型名/cwd/loading/token 计数）
│   │   ├── panels/       — 侧边面板渲染
│   │   │   ├── model.rs  — /model 面板 UI
│   │   │   ├── agent.rs  — /agents 面板 UI
│   │   │   └── thread_browser.rs — /history 面板 UI
│   │   └── popups/       — 模态弹窗渲染
│   │       ├── hitl.rs   — HITL 审批弹窗
│   │       ├── ask_user.rs — AskUser 问答弹窗
│   │       └── hints.rs  — Skills 提示浮层
│   ├── message_render.rs — 消息行渲染（Markdown 解析、代码高亮、工具折叠）
│   ├── message_view.rs   — MessageViewModel / ContentBlockView（渲染中间层）
│   ├── markdown.rs       — pulldown-cmark → ratatui Spans 转换
│   ├── render_thread.rs  — 独立渲染线程（RenderCache + Notify 驱动，零 sleep）
│   └── headless.rs       — Headless 测试模式（TestBackend + render_notify 同步）
├── langfuse/
│   ├── config.rs         — LangfuseConfig（从环境变量读取 LANGFUSE_* 配置）
│   ├── session.rs        — LangfuseSession（Thread 级别，持有 client + batcher + session_id）
│   └── tracer.rs         — LangfuseTracer（Turn 级别，Trace/Generation/Span 上报）
├── config/
│   ├── store.rs          — ZenConfig：~/.zen-code/settings.json 读写
│   └── types.rs          — 配置类型定义（Provider/Model）
├── thread/
│   ├── mod.rs            — ThreadStore re-export
│   └── browser.rs        — ThreadBrowser 线程历史浏览状态
├── command/
│   ├── mod.rs            — CommandRegistry + 命令分发
│   ├── model.rs          — /model 命令处理
│   ├── history.rs        — /history 命令处理
│   ├── agents.rs         — /agents 命令处理
│   ├── compact.rs        — /compact 命令处理
│   ├── clear.rs          — /clear 命令处理
│   ├── help.rs           — /help 命令处理
│   └── agent.rs          — agent 相关命令
├── event.rs              — crossterm 事件适配（键盘/鼠标/粘贴 → Action 枚举）
└── prompt.rs             — 系统提示词构建（段落化 sections/ + PromptFeatures 条件注入）
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

### TUI 异步通信

```
submit_message()
  ├─ mpsc(32): AgentEvent channel ──→ agent task
  │                                       └─ run_universal_agent() 产生事件
  │                                       └─ emit → tx.try_send(AgentEvent)
  │  ← poll_agent() 每帧 try_recv ←──────
  │       ToolStart/TextChunk → 追加 view_messages[]
  │       ApprovalNeeded      → app.hitl_prompt = Some(...)  [break]
  │       AskUserBatch        → app.ask_user_prompt = Some(...) [break]
  │       Done/Error          → set_loading(false), agent_rx=None
  │       LlmCallStart/End   → LangfuseTracer 上报 Generation
  │
  ├─ mpsc(4): ApprovalEvent channel ──→ 转发 task
  │    ApprovalEvent::Batch      → YOLO: 直接 response_tx.send(Approve×N)
  │                                非YOLO: tx.send(AgentEvent::ApprovalNeeded)
  │    ApprovalEvent::AskUserBatch → tx.send(AgentEvent::AskUserBatch)  [始终转发]
  │
  └─ oneshot: 弹窗确认后
       hitl_confirm()     → response_tx.send(decisions)
       ask_user_confirm() → response_tx.send(answers)

渲染管道：
  render_thread（独立线程）
    ← RenderEvent::Update 触发 → 更新 RenderCache（parking_lot::RwLock）
  主线程
    ← poll 超时 / 用户事件 → 读 RenderCache → terminal.draw()
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

## 中间件链执行顺序

中间件按注册顺序执行，典型组装顺序：

```
主 Agent（rust-agent-tui 组装）：
1. AgentDefineMiddleware      ← 解析 agent 定义，设置 model/maxTurns 等覆盖
2. AgentsMdMiddleware         ← 读 CLAUDE.md/AGENTS.md 注入 system
3. SkillsMiddleware           ← 扫描 Skills 目录，摘要注入 system
4. SkillPreloadMiddleware     ← 消息含 #skill-name 时注入 skill 全文（fake tool 序列）
5. FilesystemMiddleware       ← 提供 6 个文件系统工具
6. TerminalMiddleware         ← 提供 bash 工具
7. TodoMiddleware             ← after_tool 解析 todo_write 结果
8. HumanInTheLoopMiddleware   ← before_tool 拦截敏感工具
9. SubAgentMiddleware         ← 提供 launch_agent 工具
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
| SQLite | 本地文件 | — | `~/.zen-core/threads/threads.db` |
| OpenTelemetry Collector | HTTP OTLP Proto | — | `OTEL_EXPORTER_OTLP_ENDPOINT` |
| Langfuse | HTTPS REST | `LANGFUSE_PUBLIC_KEY` + `LANGFUSE_SECRET_KEY` | `LANGFUSE_HOST`（默认 cloud） |

## 部署拓扑

**标准模式（本地 TUI）：**

```
用户终端
  └─ cargo run -p rust-agent-tui
       ├─ 直接调用 Anthropic/OpenAI API（reqwest HTTP）
       ├─ 读写本地文件系统（FilesystemMiddleware）
       ├─ 执行 bash 命令（TerminalMiddleware）
       ├─ 写入 ~/.zen-core/threads/threads.db（SQLite WAL）
       └─ 上报 Langfuse（可选，环境变量控制）
```

**可观测性（可选）：**

```
rust-create-agent（tracing spans）
  ├─ opentelemetry-otlp HTTP → Jaeger / OTLP Collector
  └─ Langfuse（TUI 层 LangfuseTracer → Langfuse API）
       └─ Trace > Span > Generation 三级层次
```

---
*最后更新: 2026-04-30 — 由 15 个 feature 归档批量更新*
