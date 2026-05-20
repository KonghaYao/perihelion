# CLAUDE.md

## 项目概述

Rust Agent 框架，7 个 Workspace Crate + 1 个独立 Node.js CLI（`peri-cli/`）。

| Crate | 职责 |
|-------|------|
| `peri-agent` | 核心：ReAct 循环、Middleware trait、LLM 适配器、工具系统、持久化（SQLite）、遥测 |
| `peri-middlewares` | 中间件：文件系统、终端、HITL、SubAgent、Skills、Todo、Cron、MCP、Hooks、Plugin、LSP |
| `peri-widgets` | Widget 组件库（14 组件），仅依赖 ratatui + pulldown-cmark |
| `peri-acp` | **ACP 服务层**：Agent Client Protocol 实现，通过 MpscTransport/StdioTransport 桥接 TUI/IDE 与 Agent |
| `peri-tui` | TUI 应用，依赖 peri-acp（通过 ACP 协议与 Agent 通信）+ peri-widgets |
| `langfuse-client` | Langfuse 遥测客户端（独立） |
| `peri-lsp` | LSP 客户端库（独立，被 middlewares 使用） |

`rmcp` crate（v1.6.0）通过 `[patch.crates-io]` 指向本地 `rust-mcp-patch/`，上游修复后删除补丁目录即可。

**其他目录**：`peri-cli/`（Node.js CLI，版本管理/安装工具）、`scripts/`（启动脚本）、`peri-control/`、`peri-workflow-engine/`、`side-projects/`（实验性/空壳，未纳入 workspace）。

## 依赖关系

```
peri-agent ← peri-middlewares ← peri-acp ← peri-tui
    ↓              ↗ peri-lsp              ↗ peri-widgets
langfuse-client（独立）
peri-cli（独立，Node.js）
```

**TUI→ACP 通信**: TUI 不直接依赖 peri-agent/peri-middlewares，通过 `peri-acp` 的 `MpscTransport`（in-memory channel pair）与 ACP Server 通信。ACP Server 持有 Agent 构建和执行逻辑，TUI 作为纯 ACP client 前端消费 `AcpNotification` 事件。

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p <crate>               # 构建指定 crate
cargo run -p peri-tui          # 运行 TUI
cargo run -p peri-tui -- -a    # HITL 审批模式
cargo test                           # 全量测试
cargo test -p <crate> --lib -- <test_name>  # 单个测试
lefthook install                     # 安装 git hooks
lefthook run pre-commit              # pre-commit（fmt/check/clippy）
scripts/start-tui.sh                 # 启动 TUI 并连接本地 Relay
scripts/start-relay.sh               # 启动 Relay Server（端口 8080）
```

**peri-cli**（Node.js）：`install`/`update`/`list`/`add-env`/`uninstall`/`clean`，用于版本管理和安装。

## 架构要点

**ReAct 循环**（`peri-agent`）：AgentInput → collect_tools → before_agent → loop(500) { LLM → [工具调用] before_tool → 并发执行 → after_tool → emit | [回答] → emit TextChunk + StateSnapshot → after_agent }。TUI 覆盖 `max_iterations(500)`（核心默认 10）。

**[TRAP]** `tool_dispatch.rs` 采用延迟写入模式：`collect_tool_results` 执行 before_tool + 并发工具调用 + 收集结果，**不写 state**；`dispatch_tools` 在最后一步统一写入 AI 消息 + 所有 tool_result。**修改此模块时不要在 `collect_tool_results` 中调用 `state.add_message`。** 错误路径分两类：before_tool 错误 / Cancel（`collect_tool_results` 返回 `Err`，state 未修改，无孤儿 tool_use 风险）；Cancel 在执行阶段 / deferred_error（`collect_tool_results` 返回 `Ok((results, true/false, ...))`，`dispatch_tools` 写入 state 后再返回 `Err`）。所有 17 个中间件的 `before_tool`/`after_tool`/`on_error` 均不读 `state.messages()`（已验证），新增中间件必须遵守此约束。`ExecutorEvent::MessageAdded` 被 TUI 的 `map_executor_event` 丢弃，TUI 通过 `StateSnapshot` + 流式事件维护状态，不依赖 `MessageAdded` 到达顺序。（详见 spec/global/domains/agent.md#issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error）

**[TRAP]** 新增/修改事件类型语义（如工具前文本从 AiReasoning 改为 TextChunk）时，必须同步检查 TUI 侧事件映射层（`map_executor_event`）。新增 ExecutorEvent 变体时必须同步更新映射，事件丢弃会导致下游状态不一致。（详见 spec/global/domains/agent.md#issue_2026-05-11-streaming-text-invisible-with-tools，spec/global/domains/message-pipeline.md#issue_2026-05-13-streaming-text-tool-aggregation-visual-issues）

**消息类型**：`BaseMessage`（Human/Ai/System/Tool），`ContentBlock`（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）。

**LLM 适配层**：`BaseModel` trait（OpenAI/Anthropic）→ `BaseModelReactLLM` → `ReactLLM`。`RetryableLLM<L>` 指数退避重试。

**TUI 消息渲染**（`peri-tui`）：所有消息更新通过统一 `RebuildAll` 路径触发（无增量更新）。`MessagePipeline`（`message_pipeline.rs`）维护规范状态，`build_tail_vms()` 构建尾部 VMs，`messages_to_view_models()` 是唯一转换入口。流式文本通过 100ms 节流触发 RebuildAll，非流式事件立即触发。独立 `RenderThread` 处理渲染，通过 `RenderCache(RwLock)` 与 UI 线程同步。

**[TRAP]** Ephemeral VM（SystemNote/CacheWarning）依赖锚点机制：`ephemeral_notes: Vec<(MessageId, MessageViewModel)>` 记录锚点消息的 `MessageId`（前一条有 message_id 的消息），RebuildAll 时通过 `position(|v| v.message_id() == Some(anchor_id))` 查找插入位置��`retain()` 路径检查 anchor 消息是否仍存在于 view_messages（HashSet 查找）。新增 ephemeral VM 类型必须同步更新过滤逻辑。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild）

**系统提示词**：`build_system_prompt(overrides, cwd, features, extra_agent_dirs, frozen_date)` 合成。`session/new` 时调用一次，传入 `Some(frozen_date)` 冻结日期，产出完整 system prompt 字符串存入 `SessionState.frozen_system_prompt`。后续所有 `session/prompt` 轮次直接使用 frozen 值，不再重建。段落文件位于 `peri-tui/prompts/sections/`（共 11 个：01-07 + 10-13），`peri-acp` 通过 `concat!(env!("CARGO_MANIFEST_DIR"), "/../peri-tui/prompts/sections/")` 交叉引用。`PromptFeatures` 控制条件段落注入。静态段落（01-06）与动态段落（07_env + feature-gated 10-13）通过 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔——标记前的内容被 Anthropic prompt cache 命中，标记后的内容变���不影响前缀缓存。`messages_to_anthropic()` 中 `split_system_blocks()` 负责拆分。Agent 构建（`build_agent()` in `peri-acp`）在 system prompt 末尾追加 Git Attribution 段落（`Co-Authored-By` 指令），位于动态区域内不影响缓存前缀。

## Thinking/推理模式

`ThinkingConfig` 控制推理参数。Anthropic 用 `thinking + output_config.effort`，OpenAI 用 `reasoning_effort`。`budget_tokens` 最小 1024，`max_tokens` 必须 > `budget_tokens`。

**OpenAI Reasoning 回传**（`openai.rs`）：

- `reasoning_content` 顶层字段：所有模型无条件回传
- content 数组 `thinking` 类型：仅 deepseek-v4-pro（`supports_thinking_content` 标志控制）

**[TRAP]** DeepSeek `unknown variant 'thinking'`：不要把 `Reasoning` block 序列化为 `{"type":"thinking"}` 发给不支持的 provider。**[TRAP]** `reasoning_content must be passed back`：过滤 `Reasoning` 时必须同时作为顶层字段回传。两个陷阱互相关联。（详见 spec/global/domains/agent.md#issue_2026-05-12-glm-reasoning-field-not-parsed，spec/global/domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped，spec/global/domains/agent.md#issue_2026-05-12-thinking-reasoning-dataflow-issues）

## 系统提示词稳定性（第一优先级）

**[原则] 系统提示词稳定性是第一优先级**：会话开始后，系统提示词必须完全稳定、不可变更。任何在会话进行中修改系统提示词的行为（包括通过 runtime config、模型切换、技能加载、中间件注入等方式间接改变其内容）都是禁止的。系统提示词内容的任何变化都会导致 Prompt Cache 失效、模型行为漂移，严重影响会话质量。

唯一例外是 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记之后动态区域内的占位符值变化（如日期、cwd），但即使是动态区域，其**结构/模板/段落数量**也必须在会话内保持不变。新增中间件在 `before_agent` 阶段注入 System 消息时，必须确保注入内容和位置跨轮次稳定。

## Tool Search 延迟加载

非核心工具通过 `SearchExtraTools` 按需发现、`ExecuteExtraTool` 代理执行。核心工具（12 个）：Read/Write/Edit/Glob/Grep/folder_operations/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite。

**[TRAP]** `Box<dyn BaseTool>` 不能直接转 `Arc<dyn BaseTool>`，用 `box_to_arc()` 通过 `ToolWrapper(ManuallyDrop<Box>)` 透传。**绝不能用 `Box::into_raw` + `Arc::from_raw`**——布局不同导致 UB。

**[TRAP]** Prompt Cache 前缀稳定性——通用原则：所有参与缓存前缀的数据（system prompt、tools 数组、消息顺序）必须保证跨请求稳定。这是实现上述"系统提示词稳定性"原则的技术手段。具体规则：

- （a）system prompt 中用 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔静态/动态内容，标记前可缓存，标记后不缓存
- （b）优先用 `add_message`（尾部追加）而非 `prepend_message`（头部插入）
- （c）动态占位符（日期、cwd、环境变量）放在边界标记之后
- （d）middleware 注入的 System 消息天然在边界标记之后（非缓存块）

三个已踩坑的违反模式：（1）HashMap 迭代顺序不确定导致序列化内容跨进程变化；（2）`prepend_message` 向消息头部插入内容改变了 `cache_control` 标记的第一条 user 消息位置；（3）system prompt 内动态占位符（`{{date}}` 每日变化、`{{cwd}}` 跨项目变化）导致整个缓存段失效。（详见 spec/global/domains/message-pipeline.md，spec/global/domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency）

**[TRAP]** `prepend_message` 的 `insert(0)` 右移导致 StateSnapshot 快照范围扩大，泄露 System 消息到 `agent_state_messages`。StateSnapshot 应始终 `.filter(|m| !m.is_system())`，`agent_state_messages` 不应包含 System 变体。（详见 spec/global/domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls，spec/global/domains/agent.md#issue_2026-05-14-deepseek-multi-turn-tool-result-duplication）

## 中间件链执行顺序

```
1.  AgentsMdMiddleware       ← CLAUDE.md/AGENTS.md 注入
2.  AgentDefineMiddleware    ← agent 定义，model/maxTurns 覆盖
3.  SkillsMiddleware         ← Skills 摘要注入（含插件 extra_dirs）
4.  SkillPreloadMiddleware   ← #skill-name 全文注入
5.  FilesystemMiddleware     ← 6 个文件系统工具
6.  GitAttributionMiddleware ← before_tool/after_tool 追踪 Write/Edit 贡献字符数
7.  TerminalMiddleware       ← Bash
8.  WebMiddleware            ← WebFetch/WebSearch
9.  TodoMiddleware           ← after_tool 解析 TodoWrite
10. CronMiddleware           ← Cron 调度
11. HookMiddleware           ← hooks 事件拦截（多组实例）
12. HumanInTheLoopMiddleware ← before_tool 拦截
13. SubAgentMiddleware       ← Agent 工具
14. McpMiddleware            ← MCP 工具和资源（pool 成功时注册）
15. ToolSearchMiddleware     ← SearchExtraTools/ExecuteExtraTool 代理
16. LspMiddleware            ← LSP 工具 + after_tool 文件变更同步
[ReActAgent.with_system_prompt()] ← prepend
```

插件通过 `plugin_skill_dirs` → `SkillsMiddleware.with_extra_dirs()`、`plugin_hooks` → `HookMiddleware` 注入，无独立 PluginMiddleware。

## ACP/TUI 分层架构

**概述**：`peri-acp` 是独立的 ACP 服务层 crate（依赖 peri-agent + peri-middlewares + peri-lsp + langfuse-client）。`peri-tui` 降级为纯 ACP client 前端，通过 `MpscTransport`（in-memory channel pair）与 `peri-acp` 通信。

**数据流**：
```
TUI 路径:
  TUI 输入 → AcpTuiClient.new_session() / .prompt()
           → MpscClientTransport.send_request/notification()
           → MpscServerTransport.recv() (ACP Server, tokio::spawn)
           → acp_server::requests::handle_request() → acp_server::prompt::execute_prompt()
           → peri_acp::session::executor::execute_prompt() + TransportEventSink
           → peri_acp::agent::builder::build_agent() → agent.execute()
           → ExecutorEvent → TransportEventSink.push_event()
             → peri/agent_event (TUI) + peri/* (compact) + session/update (标准ACP)
           → AcpTuiClient.pump_notifications() → AcpNotification::AgentEvent
           → agent_ops::acp_bridge::handle_acp_notification() → map_executor_event() → AgentEvent
           → agent_ops::handle_agent_event() → UI 更新

Stdio 路径:
  SDK on_receive_request("session/prompt")
    → peri_acp::session::executor::execute_prompt() + StdioEventSink
    → ExecutorEvent → StdioEventSink.push_event() → SessionNotification
    → SDK cx.send_notification() → stdout JSON-RPC
```

**Frozen Data Flow**（会话内不可变数据）：
```
session/new → chrono::Local::now() → frozen_date
            → AgentsMdMiddleware::read_frozen_content(cwd) → frozen_claude_md + frozen_claude_local_md
            → SkillsMiddleware::build_frozen_summary(cwd, plugin_skill_dirs) → frozen_skill_summary
            → build_system_prompt(None, cwd, features, agent_dirs, Some(&frozen_date)) → frozen_system_prompt
            → SessionState.frozen_*
            → TUI prompt::execute_prompt → FrozenSessionData
            → executor::execute_prompt → AcpAgentConfig.frozen_*
            → AgentsMdMiddleware::with_frozen_content / SkillsMiddleware::with_frozen_summary / system_builder(frozen_date)
```

**核心文件**：
| 文件 | 职责 |
|------|------|
| `peri-acp/src/session/executor.rs` | 共享 agent 执行管线：`execute_prompt()` + `EventSink` trait，TUI 和 stdio 共用 |
| `peri-acp/src/session/event_sink.rs` | `EventSink` trait + `TransportEventSink`（TUI）+ `StdioEventSink`（stdio） |
| `peri-acp/src/session/state_builders.rs` | ACP 协议状态构建器：modes/models/configOptions |
| `peri-acp/src/` | ACP 服务层：transport trait、agent builder、event mapper、broker、prompt、provider、session、langfuse、hooks、lsp |
| `peri-tui/src/acp_server/mod.rs` | ACP Server 配置（`SessionState`/`AcpServerConfig`）+ re-export state builders |
| `peri-tui/src/acp_server/requests.rs` | `handle_request()`：处理 `session/new`/`prompt`/`compact`/`set_model`/`set_mode`/`cancel` 等 ACP 请求 |
| `peri-tui/src/acp_server/prompt.rs` | `execute_prompt()`：TUI 侧 prompt 执行入口，委托 `executor::execute_prompt()` |
| `peri-tui/src/acp_server/compact.rs` | `execute_compact()`：手动 compact 入口，调用 `compact_runner::run_full_compact()` |
| `peri-tui/src/acp_server/notify.rs` | `handle_notification()`/`send_session_info_update()`：通知推送 |
| `peri-tui/src/acp_client/client.rs` | `AcpTuiClient`：TUI 端 ACP 封装，提供 `new_session()`/`prompt()`/`compact()`/`set_model()`/`set_mode()`/`cancel()`/`send_response()` |
| `peri-tui/src/app/agent_ops/mod.rs` | `handle_agent_event()`：AgentEvent 变体分发到 lifecycle/subagent/polling 子模块 |
| `peri-tui/src/app/agent_ops/acp_bridge.rs` | `handle_acp_notification()`：将 `AcpNotification` 桥接为 `AgentEvent` |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | Agent 生命周期处理：cleanup/done/interrupted/error |
| `peri-tui/src/app/agent_ops/subagent.rs` | SubAgent 启动/生命周期/Token 用量处理 |
| `peri-tui/src/app/agent_ops/polling.rs` | `poll_agent()`/`poll_background_events()`/`poll_cron_triggers()` |
| `peri-tui/src/app/agent_submit.rs` | `submit_message()`：通过 `acp_client.new_session()` + `acp_client.prompt()` 提交用户输入 |
| `peri-tui/src/app/agent.rs` | `map_executor_event()`：`ExecutorEvent` → `AgentEvent` 映射（由 ACP bridge 调用）；`compact_task()`（遗留代码，auto-compact 已迁移到 executor） |

**AcpNotification 变体**（`acp_client/client.rs`）：
- `AgentEvent { event }` — 携带 `ExecutorEvent` JSON，由 `map_executor_event()` 转换为 TUI `AgentEvent`
- `AgentDone { session_id }` — Agent 执行完成通知
- `RequestPermission { id, params }` — HITL 审批请求
- `Elicitation { id, params }` — AskUser 问答请求
- `SessionUpdate { .. }` — 标准 ACP SessionUpdate（保留给外部 IDE client）
- `Peri { method, params }` — `peri/*` 自定义通知（compact/session 生命周期等）
- `Other { msg }` — 未识别的通知

**ACP Server 请求处理**（`acp_server/requests.rs`）：
- `session/new` → 创建 session state，分配 session_id
- `session/prompt` → 构建 Agent，执行，推送 `notifications/agent_event`
- `session/compact` → 手动 compact，调用 `compact_runner::run_full_compact()`
- `session/set_model` → 模型切换（通过 peri_config.alias）
- `session/set_mode` → 权限模式切换
- `session/set_config_option` → 统一配置选项（mode/model/thinking_effort）
- `session/set_thinking` → 推理模式配置
- `$/cancel_request` → 取消当前 session 的 Agent 执行

**Transitional Note**: TUI 当前保留 `AgentEvent` 枚举和 `handle_agent_event()` 处理器（`agent_ops/mod.rs:handle_agent_event`）以复用战验过的 UI 逻辑。Config/LlmProvider 类型已统一（TUI re-export `peri-acp` 的定义）。Agent 执行逻辑已通过 `EventSink` trait + `executor::execute_prompt()` 统一到 `peri-acp`，TUI 和 stdio 各自提供 EventSink 实现。`peri-tui/Cargo.toml` 仍保留 `peri-agent`/`peri-middlewares` 直接依赖（用于 UI 渲染所需的 `BaseMessage`/`ContentBlock` 等类型和中间件组件初始化）。

**[TRAP]** Agent 构建和执行统一通过 `peri_acp::session::executor::execute_prompt()`（内部调用 `peri_acp::agent::builder::build_agent()`）。禁止在 TUI 层直接构建 ReActAgent 或手写事件泵——使用 `EventSink` 实现委托给 executor。

**[TRAP]** Session Config Options 覆盖旧的 Session Modes API。ACP 规范明确指出 `configOptions` 取代 `modes`/`models`，但过渡期内需同时发送两者以兼容旧客户端。IDE 客户端通过 `configOptions` 中条目的 `category` 字段决定渲染哪些 UI 控件：`category: "mode"` → 权限模式选择器，`category: "model"` → 模型下拉，`category: "thought_level"` → 推理强度。`build_config_options()` 必须按优先级顺序返回（mode → model → thinking_effort），`session/set_config_option` 处理器必须处理 `"mode"` 和 `"model"` config ID（除了已有的 `"thinking_effort"`）。仅发送 `modes`/`models` 而缺少对应 `configOptions` 条目的，已迁移到新 API 的 IDE 不会显示任何控件。

## HITL 审批

默认需审批：`Bash`、`folder_operations`、`Agent`、`Write`、`Edit`、`delete_*`、`rm_*`、`mcp__*`、`WebFetch`、`WebSearch`。

## 上下文压缩

**架构**：compact 触发 + 执行 + resubmit 全部在 `peri-acp/src/session/executor.rs` 的 `execute_prompt()` 循环内完成（TUI 和 stdio 共享）。`compact_runner` 模块封装 full/micro compact 的具体逻辑（hooks + 事件 + 取消）。TUI 侧 `handle_compact_completed` 只负责 UI 通知和 pipeline 状态清理。

**触发**：executor 在每轮 agent 执行后检查 `ContextBudget`：0.70 micro-compact（清除 ≥5 步前的工具结果），0.85 full compact（LLM 生成摘要 + re_inject）。Full compact 后自动 resubmit（最多 3 次）。环境变量覆盖：`DISABLE_COMPACT`、`DISABLE_AUTO_COMPACT`、`COMPACT_THRESHOLD`（0.0-1.0）。

**手动 /compact**：通过 ACP `session/compact` 请求 → ACP server `execute_compact()` → `compact_runner::run_full_compact()`。TUI 的 `start_compact()` 通过 `AcpTuiClient.compact()` 委托。

**核心文件**：
| 文件 | 职责 |
|------|------|
| `peri-agent/src/agent/events.rs` | `CompactStarted`/`CompactCompleted`/`CompactError` 事件变体，`CompactCompleted` 携带 `messages`、`files`、`skills` |
| `peri-acp/src/session/compact_runner.rs` | `run_full_compact()`/`run_micro_compact()`：hooks + 事件 + compact 执行 |
| `peri-acp/src/session/executor.rs` | auto-compact 循环：执行后检查阈值 → compact → resubmit |
| `peri-tui/src/app/agent_compact.rs` | TUI 侧 compact 事件处理：pipeline 清理 + UI 通知 |

**[TRAP]** `handle_compact_completed` 必须三步清理：① `pipeline.clear()` ② `pipeline.restore_completed(messages)` ③ `RebuildAll { prefix_len: 0 }`。缺少任一步都会导致旧消息残留或 system 消息泄漏到显示。`CompactCompleted` 事件必须携带 `messages: Vec<BaseMessage>`，TUI 用它更新 `agent_state_messages` 和 pipeline。禁止在 TUI 层触发 auto-compact——所有触发判断在 executor 内部。

**[TRAP]** compact 后消息结构必须以 `BaseMessage::human(summary + continuation)` 开头（与 Claude Code 实现对齐）。禁止将摘要放在 `BaseMessage::system()` 中——LLM 适配器（`messages_to_json`/`messages_to_anthropic`）将 System 消息提取到 system 字段不进入 messages 数组，导致发给 API 的 messages 数组中无 user/assistant 消息，DeepSeek/OpenAI 兼容 API 返回 400。compact 后的完整结构：`[Human(摘要+续接指令), System(文件)..., System(Skills)...]`。此约束适用于 `CompactMiddleware::do_full_compact()` 和 `acp_server::execute_compact()` 两条路径。

**[TRAP]** `restore_completed(messages)` 会把 messages 中的 system 消息放入 pipeline 的 completed 列表。compact 后的 messages 中 re_inject 产生的 System 消息是内部状态，不应被渲染。pipeline 的 reconcile 逻辑通过 `messages_to_view_models` 跳过 System 消息、`build_tail_vms` 用 `rposition(Human)` 定位到摘要 Human 消息作为 reconcile 起点。`round_start_vm_idx` 和 `completed_len_at_round_start` 必须正确设置，否则 view_messages 会泄漏 system 消息。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt）

## MCP 中间件

`McpMiddleware` 基于 `rmcp` crate。配置合并：全局 `~/.peri/settings.json` + 项目 `{cwd}/.mcp.json`（同名覆盖）。工具命名 `mcp__{server_name}__{tool_name}`。插件 MCP 使用 `{plugin_name}__{server_name}` 前缀命名空间。

**[TRAP]** `ClaudeSettings` 的 `extraKnownMarketplaces` 和 `enabledPlugins` 需同时支持对象和数组格式。**`enabledPlugins` 写入必须用对象格式** `{"id": true}`。

**Plugin Sources 旁路表**：`load_merged_config_full` 返回 `(McpConfigFile, HashMap<String, String>)`，key 格式 `"plugin:{name}:{server}"`，value `"name@marketplace"`。`load_installed_plugins` 路径需从 `claude_home` 参数推导。

## 插件系统

兼容 Claude Code 插件生态。配置：`~/.peri/settings.json`（全局）+ `~/.claude/plugins/cache/`（插件 manifest）。

**Hooks**（`peri-middlewares/src/hooks/`）：4 种执行类型（Command/Prompt/Http/Agent），14 种事件。exit code 控制流程：0=Allow，1=Warn，2=Block。SSRF 防护阻止内网地址（`ssrf_guard.rs`），回环地址允许。

**Frontmatter 解析**：skill 和插件命令用 `gray_matter` crate（YAML engine），必须复用 `Matter::<YAML>::new()` 模式。

**Skills**：搜索顺序 `~/.claude/skills/` → `skillsDir` → `./.claude/skills/` → 插件 skills。`SkillsMiddleware.with_extra_dirs()` 是插件扩展点。

## SubAgents

`.claude/agents/{agent_id}/agent.md` 定义。`tools` 为空继承父工具（排除 Agent 防递归），有值仅保留允许列表，`disallowedTools` 额外排除。插件 agent 通过 `scan_agents_with_extra_dirs` 追加搜索路径。

**[TRAP]** Background agent 工具完全依赖 `register_tool` 传递，跨 async 边界需确保 Arc 引用生命周期。多语义叠加（fork+background）需明确优先级，跨轮次累积数据（frozen_vms）必须有清理机制。**[TRAP]** Normal/Fork 子 Agent 透传 event_handler 导致事件溢出，StateSnapshot/ContextWarning/LlmRetrying 缺少 in_subagent() 守卫——新增事件类型时必须同步检查所有事件处理路径的守卫。**[TRAP]** 并发 SubAgent 场景：事件路由必须用 `source_agent_id` 精确匹配而非位置堆栈；流式循环必须 `tokio::select!` 竞争取消令牌防止 Ctrl+C 死锁；事件通道容量需基于 SubAgent 速率（≥4096）而非主 Agent；Agent 工具调用应从串行恢复为并发。（详见 spec/global/domains/agent.md#issue_2026-05-12-background-agent-display-and-continuation-bugs，spec/global/domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent，spec/global/domains/tui.md#issue_2026-05-15-concurrent-subagent-display-delay，spec/global/domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background，spec/global/domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially，spec/global/domains/agent.md#issue_2026-05-19-concurrent-subagent-duplicate-id）

## LSP 中间件

`LspMiddleware` + `LspTool` + `peri-lsp` 客户端库。10 种操作（goToDefinition/findReferences/hover 等），`after_tool` 自动同步文件变更（`didChange` + `didSave`）。

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE=true/false` | 跳过/启用 HITL 审批 |
| `RUST_LOG` | 日志级别（默认 info） |
| `RUST_LOG_FILE` | 日志文件路径 |
| `LANGFUSE_*` | Langfuse 追踪 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP 导出端点 |

配置通过 `~/.peri/settings.json` 的 `env` 字段注入。

## CLI 参数

`-a` 启用 HITL 审批。运行时 `Shift+Tab` 切换权限模式，`Alt+M` 切换模型。支持多 session 分屏。

## 编码规范

- Rust 2021 edition，tokio async/await + async-trait
- 库用 `thiserror`，应用层用 `anyhow::Result`
- 日志用 `tracing`，禁止 `println!`/`eprintln!`
- 测试与源码分离为同目录 `_test.rs` 文件（≥30 行必须分离）
- bin crate 集成测试在 `src/` 内（不支持 `tests/` 目录）
- 每模块一目录，`mod.rs` 入口；Workspace resolver = "2"，禁止下层依赖上层
- 禁止 `ℹ`（U+2139）符号和 `[i]` 前缀
- **字符串截断必须用字符级操作**：`s.chars().take(N).collect()` 或 `s.char_indices().nth(N)`，`&s[..N]` 对 CJK 会 panic
- 终端列宽用 `unicode-width` crate（CJK 占 2 列）
- **终端 UI 鼠标坐标转换**：鼠标事件坐标是显示列（unicode-width），光标位置是字符索引，需逐字符累加转换。Block padding/border 和水平滚动也要纳入偏移计算。（详见 spec/global/domains/tui.md#issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk）

## i18n 国际化

i18n 模块位于 `peri-tui/src/i18n/`，仅 TUI crate 使用，不向 `peri-widgets` 或 `peri-agent` 传递。`LcRegistry` 存储在 `ServiceRegistry.lc` 中，翻译资源通过 `include_str!` 编译时嵌入 `locales/{lang}/main.ftl`。

**[TRAP]** `FluentBundle::get_message` 返回的 `FluentMessage` 生命周期绑定在 bundle 上，`tr()` 方法必须返回 `String` 而非 `&str`，否则无法满足借用检查。`format_pattern` 的 args 参数类型是 `FluentArgs` 而非 `HashMap`。

Command trait 的 `description()` 接收 `&LcRegistry` 参数并返回 `String`，新增命令实现时必须遵循此签名。`PluginCommandAdapter` 的 description 直接返回 manifest 中的文本（不经过 LcRegistry）。`CommandRegistry::match_prefix()` 和 `list()` 均需 `&LcRegistry` 参数，调用时统一通过 `&app.services.lc` 获取。测试中使用 `LcRegistry::default()` 构造。

## 测试编写风格

- 注释、断言消息用中文；命名 `test_<被测对象>_<场景>`
- Arrange-Act-Assert，无空行分隔
- 断言优先 `assert_eq!`/`assert!`，`.unwrap()` 仅用于构造测试数据
- Mock 命名 `make_`/`mock_` 前缀，不跨文件共享
- 最小依赖：`assert!`/`assert_eq!`/`matches!` + `tempfile` + `tokio-test`

## 开发注意事项

- **BaseMessage vs MessageViewModel 维度混淆 [TRAP]**：`completed_len_at_round_start` 是 BaseMessage 长度，`prefix_len` 是 VM 索引，两者非 1:1。`prefix_len` 必须用 `round_start_vm_idx`，`drain` 必须钳位。**禁止 Pipeline 内部返回 `RebuildAll`**——Pipeline 不拥有 `round_start_vm_idx`。
- **MessageViewModel.message_id [INFO]**：所有 VM 变体均有 `message_id: MessageId`（SubAgentGroup 为 `Option<MessageId>`），新增 VM 变体时必须填充此字段。`message_id` 不参与 `PartialEq` 和 `Hash`（身份标识 ≠ 显示内容）。ToolCallGroup 使用 `MessageId::new()` 生成临时 ID（聚合函数无 Ai 消息上下文）。SubAgentGroup.message_id：流式路径为 `None`，restore/reconcile 路径透传 `Some(tool_msg.id())`。
- **`Interrupted`/`Error` + `Done` 互斥 [TRAP]**：`Interrupted`/`Error` 先 `request_rebuild()` + 添加通知，设 `reconcile_already_done=true`，后续 `Done` 跳过 `request_rebuild()` 防止覆盖通知。
- **快捷键设计**：禁止 `Shift+字母`（编辑态等同大写输入）。全局用 `Ctrl+字母`，面板用方向键/Space/Enter/Esc。
- **面板系统**：`PanelManager` + `PanelComponent` trait（`panel_manager.rs`/`panel_component.rs`），新增面板只需定义变体 + 实现 trait。面板内禁止渲染提示行，由 `status_bar_hints()` 统一描述。
- **`Event::Paste`**：独立于 key event 链，必须单独拦截。
- **测试隔离**：禁止写入全局配置。用 `App::save_config(cfg, self.config_path_override.as_deref())`。
- **`std::sync::RwLockReadGuard` 不是 `Send`**，async 中不能跨 `.await` 持有，用 `parking_lot::RwLock`。（详见 spec/global/domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit）
- **`CommandRegistry::dispatch` 借用限制 [TRAP]**：`&self` + `&mut App` 冲突，当前用 `std::mem::take` + put-back 解决。在 dispatch 期间改变 `app.session_mgr.active` 的命令（如 `/split`）存在 session index 竞态风险——take 前保存 index，归还时使用保存值。（详见 spec/global/domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active）
- **`ServiceRegistry` 与 `GlobalUiState`**：`App` 状态已拆分为 `ServiceRegistry`（跨会话共享服务：config/MCP/cron/provider）和 `GlobalUiState`（纯 UI 临时状态：高亮计时器/弹窗/鼠标检测）。面板 dispatch 宏（`with_global_panels!`/`with_session_panels!`）位于 `event/macros.rs`，封装了 `mem::take` 模式。
- **`app/mod.rs` 模块组织**：使用 `include!` 按功能类别分组声明（`modules_panels.inc`/`modules_agent.inc`/`modules_state.inc`/`modules_system.inc`），每个 `.inc` 文件声明一组相关模块。新增模块按类别加入对应的 `.inc` 文件，避免 `app/mod.rs` 膨胀。
- **`app/edit_utils.rs`**：从 `app/mod.rs` 拆出的文本编辑辅助函数（`build_textarea`/`handle_edit_key`/`ensure_cursor_visible`/`edit_display_parts`），供多面板共用。
- **插件面板拆分**：handler 从 `plugin_panel/handlers.rs` 拆分为 `handlers/plugin_handlers/` 9 个子模块（delete/discover_detail/discover_list/discover_search/install/installed_detail/marketplace/persistence/mod）。渲染从 `panels/plugin.rs` 拆分为 `panels/plugin/mod.rs` + `plugin_render/` 6 个子模块（add_marketplace/detail/discover_detail/discover_list/discover_search/list）。
- **面板统一列表状态**：面板内列表组件使用统一的 `ListState` 管理（选中/滚动/过滤），支持鼠标交互（滚轮/点击/拖拽）。（详见 `peri-tui/src/panels/`）
- **工具并发结果处理 [TRAP]**：多工具并发的结果处理循环中，P3/P4 错误路径提前返回会导致后续 tool_result 缺失。必须用 deferred_error 模式——先收集所有错误，循环结束后统一判断。所有 tool_result 必须始终写入 state。（详见 spec/global/domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result，spec/global/domains/agent.md#issue_2026-05-15-tool-execution-error-stops-agent，spec/global/domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially）
- **frozen_subagent_vms HashMap [TRAP]**：`frozen_subagent_vms: HashMap<String, MessageViewModel>` 按 agent_id 精确匹配，不再依赖位置索引。同一 agent_id 冻结两次会覆盖。轮次作用域状态（frozen_vms、ephemeral_notes）必须在 begin_round/done 时显式清空。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication）
- **sync 模块路径穿越防护**：`validate_and_resolve()` 是项目标准的路径穿越防护入口，使用三层校验（绝对路径拒绝 + 深度计数器检测 ParentDir + 解析后前缀验证）。任何需要接收用户侧相对路径并写入 base_dir 的场景都必须复用此函数。新增类似写入功能时禁止自行实现路径校验。
- **sync 模块路径安全 [TRAP]**：sync 模块中 FileEntry.path 来自外部不可信数据（网络传输的解密结果），写入前必须经过 `validate_and_resolve` 校验，禁止直接拼接路径或使用未校验的相对路径。
- **sync 模块跨平台路径分隔符 [TRAP]**：`Path::strip_prefix()` + `to_string_lossy()` 在 Windows 上产生 `\` 分隔符，但 sync 协议要求 `FileEntry.path` 使用 `/`（跨平台兼容 + 序列化一致性）。`scan_dir_recursive()` 构造 `FileEntry.path` 时必须 `.replace('\\', "/")`。新增类似路径字符串化场景必须归一化分隔符，否则 Windows 测试（如 `paths.contains(&"my-skill/SKILL.md")`）会失败。（详见 spec/global/domains/sync.md#issue_2026-05-20-windows-path-separator-breaks-tests）
- **`peri sync` 子命令**：使用标准终端 CLI（crossterm 交互 + println!），不经过 TUI 主循环。`Commands::Sync` 分支在 main.rs 中创建独立 tokio runtime。
- **sync 模块 crossterm 依赖**：sender.rs/receiver.rs/ui.rs 使用 crossterm 通过 ratatui 重导出路径 `ratatui::crossterm::*`，不引入独立 crossterm 依赖。
