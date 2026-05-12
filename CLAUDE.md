# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Rust Agent 框架，包含 **6 个 Workspace Crate** 和 **1 个独立的 Node.js CLI**：

- **`rust-create-agent`**：核心框架——ReAct 循环执行器、Middleware trait、LLM 适配器、工具系统、线程持久化（SQLite）、遥测（OTel）
- **`rust-agent-middlewares`**：中间件实现（文件系统、终端、HITL、SubAgent、Skills、Todo、Cron、MCP、Hooks、Plugin、LSP 等）
- **`perihelion-widgets`**：独立 widget crate（BorderedPanel/ScrollableArea/SelectableList 等 11 组件），零内部依赖，仅依赖 ratatui + pulldown-cmark
- **`rust-agent-tui`**：交互式 TUI 应用，基于 ratatui
- **`langfuse-client`**：Langfuse 遥测客户端
- **`perihelion-lsp`**：LSP 客户端库，被 `rust-agent-middlewares` 的 LSP 中间件使用
- **`peri-cli`**（Node.js）：`peri install/list/update/uninstall/clean` 包管理 CLI

核心价值：高兼容（复用 `.claude/` 配置零迁移）、可插拔（中间件模式按需组合）、生产可用（异步+OTel 追踪）。

`rmcp` crate（v1.6.0）通过 `[patch.crates-io]` 指向本地 `rust-mcp-patch/`，修复部分 MCP 服务器对 `notifications/initialized` 返回 HTTP 200 + 空 body 导致的 `UnexpectedContentType(None)` 错误。上游发布修复后删除补丁目录即可。

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p rust-create-agent     # 构建指定 crate
cargo run -p rust-agent-tui          # 运行 TUI
cargo run -p rust-agent-tui -- -a    # 启用 HITL 审批模式
cargo test                           # 全量测试
cargo test -p rust-create-agent --lib -- test_name  # 运行单个测试
lefthook install                     # 安装 git hooks
lefthook run pre-commit              # 手动运行 pre-commit（fmt/check/clippy）
```

## Workspace 依赖关系

```
rust-create-agent (核心框架，零内部依赖)
    ↑
rust-agent-middlewares (中间件实现，依赖 rust-create-agent + perihelion-lsp)
    ↑
perihelion-widgets (零内部依赖，仅依赖 ratatui + pulldown-cmark)
    ↑
rust-agent-tui (TUI 应用，依赖 widgets + middlewares)

langfuse-client (遥测客户端，独立)
perihelion-lsp (LSP 客户端库，独立，被 middlewares 使用)

peri-cli (Node.js 包管理 CLI，独立)
```

## 数据流

**ReAct 循环**（`rust-create-agent`）：AgentInput → chain.collect_tools → chain.run_before_agent → loop(max_iterations=50) { LLM generate_reasoning → [有工具调用] before_tool → 并发执行 → after_tool → emit events | [最终回答] → emit TextChunk + StateSnapshot → after_agent }。

**TUI 异步通信**（`rust-agent-tui`）：submit_message() 通过 mpsc(32) AgentEvent channel 驱动 agent task，poll_agent() 每帧 try_recv 更新 UI。审批事件通过 mpsc(4) ApprovalEvent channel 转发，弹窗确认通过 oneshot 解除。渲染管道独立线程：RenderEvent → RenderCache(RwLock) → terminal.draw()。

**系统提示词**：`build_system_prompt(overrides, cwd, features)` 合成，段落文件位于 `rust-agent-tui/prompts/sections/`（静态 01-08 + Feature-gated 10-13 + 动态覆盖块）。`PromptFeatures` 控制条件段落注入（hitl/subagent/cron/skills）。

**消息类型**：`BaseMessage` 四变体（Human/Ai/System/Tool），`ContentBlock` 七变体（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）。

**LLM 适配层**：`BaseModel` trait（OpenAI/Anthropic 实现）→ `BaseModelReactLLM`（适配为 `ReactLLM`）。`RetryableLLM<L>` 装饰器提供指数退避重试。

**Thinking/推理模式**：`ThinkingConfig`（`rust-agent-tui/src/config/types.rs`）控制推理参数，Anthropic 用 `thinking + output_config.effort`，OpenAI 用 `reasoning_effort`。`budget_tokens` 最小 1024，`max_tokens` 必须 > `budget_tokens`。

**OpenAI 兼容 Reasoning 回传规则**（`rust-create-agent/src/llm/openai.rs`）：

| 通道 | 格式 | 适用模型 |
|------|------|---------|
| `reasoning_content` 顶层字段 | `{"role":"assistant","content":"...","reasoning_content":"思考内容"}` | **所有模型**，无条件回传（无 reasoning 时为空字符串） |
| content 数组 `thinking` 类型 | `{"type":"thinking","thinking":"思考内容"}` | deepseek-v4-pro（通过 `supports_thinking_content` 标志控制） |

`ChatOpenAI` 的 `supports_thinking_content` 字段控制 content 数组中是否包含 `thinking` 块，`detect_thinking_content_support()` 根据模型名自动检测（默认 false，仅 deepseek-v4 开启）。`extract_reasoning_text()` 从 `Reasoning` blocks 提取文本写入 `reasoning_content` 顶层字段，此行为对所有模型生效（不支持的字段会被忽略）。`OpenAiAdapter::from_base_messages`（持久化层）同样回传 `reasoning_content` 顶层字段，确保跨轮次 reasoning 不丢失。

**[TRAP]** DeepSeek 错误 `unknown variant 'thinking', expected 'text'`：把 `Reasoning` block 序列化为 content 数组中的 `{"type":"thinking"}` 发给了不支持的 provider。**[TRAP]** DeepSeek 错误 `reasoning_content must be passed back`：从 content 中过滤了 `Reasoning` 但没作为顶层字段回传。两个陷阱互相关联，不能只修一个。

## 消息渲染管线

### agent_ops 模块结构

`rust-agent-tui/src/app/agent_ops*.rs` 按职责拆分为 7 个文件，所有函数均为 `impl App` 方法（Rust 允许跨文件分片），零签名改动：

| 模块 | 行数 | 职责 |
|------|------|------|
| `agent_ops.rs` | ~1170 | 骨架：`handle_agent_event` match dispatch + `poll_agent`/`poll_background_events`/`poll_cron_triggers` + Done/Interrupted/Error/InteractionRequest 核心分支 |
| `agent_render.rs` | ~90 | Render bridge：`render_rebuild`、`render_rebuild_with_anchor`、`request_rebuild`、`apply_pipeline_action` |
| `agent_submit.rs` | ~380 | Agent 启动：`submit_message`、`flush_pending_messages`、`extract_skill_tokens` |
| `agent_compact.rs` | ~240 | 上下文压缩：`handle_compact_done`、`handle_compact_error`、`start_micro_compact` |
| `agent_events_oauth.rs` | ~90 | OAuth/MCP 事件：`handle_oauth_needed/completed/failed`、`handle_mcp_action_completed` |
| `agent_events_plugin.rs` | ~115 | 插件事件：`handle_plugin_action_completed` |
| `agent_events_bg.rs` | ~140 | 后台任务：`handle_background_task_completed` |

`handle_agent_event` 中被提取的分支通过 `pub(crate)` 委托方法调用（如 `self.handle_compact_done(summary)`），保留在骨架中的分支（Done/Interrupted/Error/InteractionRequest）因与 `reconcile_already_done`、`pending_bg_continuation` 等核心状态强耦合而不宜再拆。

### 全局架构：统一 RebuildAll

```
┌──────────────┐    AgentEvent     ┌──────────────────┐                    ┌─────────────┐   RenderEvent   ┌──────────────┐
│  ReAct Loop   │ ───────────────→ │  MessagePipeline  │  handle_event()   │  agent_ops   │ ─────────────→ │ RenderThread │
│ (rust-create- │   mpsc(256)      │  (规范状态管理)   │ ──── returns ───→ │ (桥接层)      │   unbounded     │ (独立渲染线程)│
│   agent)      │                  │  message_pipeline │    None           │               │                  │ render_thread│
└──────────────┘                  └──────────────────┘                    └─────────────┘                  └──────┬───────┘
                                                                                                               │
                                                                                                    RenderCache(RwLock)
                                                                                                               │
                                                                                                    terminal.draw()
```

**核心原则**：所有消息更新（流式文本、工具事件、SubAgent、Done/Interrupted）都通过 `RebuildAll` 触发消息流重构。没有增量路径——`handle_event()` 只更新 pipeline 内部状态，返回 `PipelineAction::None`；`agent_ops` 在非流式事件后立即调用 `request_rebuild()`，流式文本通过 100ms 节流 `check_throttle()` 触发。

**统一转换函数**：`messages_to_view_models(base_messages, cwd)`（`message_pipeline.rs`）是**唯一**的 BaseMessage → MessageViewModel 转换入口，`build_tail_vms()` 的 reconcile 路径和历史恢复都经过它，保证最终显示一致。

### 第一层：核心事件（AgentEvent）

**核心层**（`rust-create-agent/src/agent/events.rs`）发射事件，TUI 层（`rust-agent-tui/src/app/events.rs`）定义扩展变体：

| 核心事件 | 含义 | Pipeline 内部操作 | RebuildAll 触发 |
|----------|------|-------------------|-----------------|
| `AssistantChunk(chunk)` | LLM 流式文本片段 | `push_chunk()` / SubAgent 内部路由 + arm throttle | `check_throttle()`（100ms 节流） |
| `AiReasoning(text)` | 思维链/推理内容 | `push_reasoning()` / SubAgent 推理更新 + arm throttle | `check_throttle()`（100ms 节流） |
| `ToolStart { id, name, input }` | 工具调用开始 | `finalize_current_ai()` + `pending_tools` 插入 | 立即 `request_rebuild()` |
| `ToolEnd { id, output, is_error }` | 工具调用结束 | `pending_tools` 移除 + `completed_tools` 插入 | 立即 `request_rebuild()` |
| `StateSnapshot(msgs)` | 完整消息快照 | `set_completed()` + 清除流式缓冲/pending/completed_tools | 立即 `request_rebuild()` |
| `SubAgentStart/End` | 子 agent 生命周期 | 委托 `tool_start_internal`/`tool_end_internal` | 立即 `request_rebuild()` |

**TUI 扩展事件**（Pipeline 返回 `None`，由 `agent_ops` 直接处理）：`Done`、`Error`、`Interrupted`、`CompactDone/Error`、`TokenUsageUpdate`、`InteractionRequest`、`TodoUpdate`、`LlmRetrying`、`ContextWarning`、`OAuth*`、`BackgroundTaskCompleted`。

### 第二层：MessagePipeline（规范状态管理）

**位置**：`rust-agent-tui/src/app/message_pipeline.rs`

维护规范消息状态和流式缓冲区：

```rust
pub struct MessagePipeline {
    completed: Vec<BaseMessage>,           // 已完成消息（可持久化）
    current_ai_text: String,               // 流式 AI 文本缓冲
    current_ai_reasoning: String,          // 流式推理缓冲
    current_ai_tool_calls: Vec<ToolCallRequest>,  // 当前轮工具调用
    current_ai_finalized: bool,            // 当前 AI 消息是否已 finalize
    pending_tools: HashMap<String, PendingTool>,  // 已开始未结束的工具
    completed_tools: Vec<CompletedTool>,   // ToolEnd 后、StateSnapshot 前的工具结果
    subagent_stack: Vec<SubAgentState>,    // SubAgent 执行栈
    frozen_subagent_vms: Vec<MessageViewModel>,  // SubAgentEnd 时固化的 VM
    // 节流状态
    throttle_armed: bool,                  // 有待发射的节流 RebuildAll
    throttle_last_fire: Option<Instant>,   // 上次节流发射时间
    // 轮次追踪
    completed_len_at_round_start: usize,   // 本轮开始时 completed 的长度
    has_snapshot_this_round: bool,         // 本轮是否收到过 StateSnapshot
}
```

**`build_tail_vms()`** 是核心重建方法，从 pipeline 规范状态构建尾部 VMs：

1. **reconcile**（仅 `has_snapshot_this_round=true`）：从 `completed[last_human_offset..]` 调用 `messages_to_view_models()` 重建
2. **流式 AssistantBubble**：从 `current_ai_text`/`current_ai_reasoning`/`current_ai_tool_calls` 构建
3. **pending tools**：`pending_tools` 中未完成的工具（跳过 `name=="Agent"`，由 subagent_stack 表示）
4. **completed tools**：`completed_tools` 中 ToolEnd 后、StateSnapshot 前的工具结果
5. **SubAgentGroup**：`has_snapshot=true` 时用 `merge_frozen_subagents()` 替换 reconcile 产出的占位符；`has_snapshot=false` 时直接从 `subagent_stack` 构建（运行中或已冻结）
6. **聚合**：`aggregate_tool_groups()` 折叠相邻只读工具

**100ms 节流机制**：`AssistantChunk`/`AiReasoning` 事件 arm throttle（`throttle_armed=true`），`poll_agent()` 每帧调用 `check_throttle()`，100ms 间隔发射 RebuildAll。`ToolStart`/`ToolEnd` 等非流式事件重置 throttle（`throttle_armed=false`）并立即触发 `request_rebuild()`。

**SubAgent 生命周期**：SubAgent 内部事件（ToolStart/ToolEnd/AssistantChunk）路由进 `subagent_stack.last_mut()` 的 `recent_messages` 滑动窗口（最多 4 条，FIFO）。`SubAgentEnd` 时立即构建完整 SubAgentGroup VM 并推入 `frozen_subagent_vms`（不再等 Done）。

**`finalize_current_ai()`**：设置 `current_ai_finalized=true` 但**不清空** `current_ai_text`/`current_ai_reasoning`——在 StateSnapshot 到达前，`build_tail_vms()` 仍需要这些内容来显示 AI 已输出的文本。`set_completed()` 到达时才清空。

### 第三层：PipelineAction → RenderEvent（桥接层）

**位置**：`rust-agent-tui/src/app/agent_ops.rs`（`apply_pipeline_action()`）

PipelineAction 精简为 3 种：

| PipelineAction | 含义 | 对应 RenderEvent | 说明 |
|----------------|------|-------------------|------|
| `None` | 无 UI 变化 | — | 跳过 |
| `AddMessage(vm)` | 新增完整消息 | `Rebuild` | 外部通知（OAuth/MCP/Plugin/TokenUsage）+ 用户消息 |
| `RebuildAll { prefix_len, tail_vms }` | 尾部重建 | `RebuildWithAnchor` | 所有消息更新统一走此路径 |

**`request_rebuild()`** 辅助方法：`build_rebuild_all(round_start_vm_idx)` → `apply_pipeline_action()`。所有非流式事件处理器（ToolStart/ToolEnd/StateSnapshot/SubAgentStart/SubAgentEnd/Done）在 `handle_event()` 后调用 `request_rebuild()`。

**`RebuildAll` 滚动锚点**：通过 `wrap_map` 将当前 `scroll_offset`（视觉行号）映射到消息索引 `anchor_message_idx`，渲染线程重建后计算该消息在新布局中的视觉行起始位置，写入 `cache.scroll_anchor`，UI 线程读取后恢复滚动位置。

### 第四层：RenderThread（独立渲染线程）

**位置**：`rust-agent-tui/src/ui/render_thread.rs`

```rust
struct RenderTask {
    messages: Vec<MessageViewModel>,    // 私有消息副本
    cache: Arc<RwLock<RenderCache>>,    // 共享渲染缓存
    notify: Arc<Notify>,                // 更新通知
    width: u16,                         // 终端宽度
}
```

**RenderCache**（UI 线程读取，渲染线程写入）：

```rust
pub struct RenderCache {
    pub lines: Vec<Line<'static>>,          // 所有消息渲染后的行
    pub message_offsets: Vec<usize>,        // 每条消息的起始行索引
    pub total_lines: usize,                 // wrap 后的真实视觉行数
    pub version: u64,                       // 版本号（UI 比较是否需要重绘）
    pub wrap_map: Vec<WrappedLineInfo>,     // 每行的换行映射（用于滚动定位和文本选择）
    pub width: u16,                         // 当前渲染宽度
    pub scroll_anchor: Option<usize>,       // RebuildAll 后的滚动锚点
}
```

**RenderEvent 处理**：

| RenderEvent | 渲染策略 | 性能特征 |
|-------------|---------|----------|
| `Rebuild` | 替换全部消息 → `rebuild_all()` | O(全部消息) |
| `RebuildWithAnchor` | `rebuild_all()` + 计算锚点 | O(全部消息) |
| `Resize` | `rebuild_all()` | O(全部消息) |
| `Clear` | 清空所有缓存 | O(1) |
| `ToggleToolMessages` | 更新标志位（后续 Rebuild 驱动实际渲染） | O(1) |

**Hash diff 优化**：渲染线程维护 `last_messages` + `message_hashes`，`rebuild_all()` 时对比每条消息的 hash，跳过未变更的消息，只重渲染变更部分。这使得统一 RebuildAll 路径的性能接近增量更新。

**无界 channel**：渲染事件处理耗时微秒级，不会积压。有界 channel 的 `try_send` 静默丢弃会导致渲染线程与 App 状态分叉。

### 视图模型体系

**MessageViewModel**（`rust-agent-tui/src/ui/message_view.rs`）— 7 种变体：

| 变体 | 用途 | 流式行为 |
|------|------|---------|
| `UserBubble { content, rendered }` | 用户输入 | 不可变 |
| `AssistantBubble { blocks, is_streaming, collapsed }` | AI 回复 | `append_chunk()` 追加文本，`dirty` 标记延迟 markdown 渲染 |
| `ToolBlock { tool_name, tool_call_id, content, collapsed, color }` | 工具调用结果 | ToolStart 创建空内容，ToolEnd 填充 |
| `SubAgentGroup { agent_id, task_preview, total_steps, recent_messages, is_running, final_result }` | 子 agent 执行块 | recent_messages 滑动窗口实时更新 |
| `ToolCallGroup { category, tools, collapsed }` | 只读工具聚合 | Read/Grep/Glob/AskUserQuestion 相邻折叠 |
| `SystemNote { content }` | 系统消息 | 不可变 |
| `CacheWarning { content }` | 缓存率警告 | 合成 VM，不在 BaseMessage[] 中，rebuild 时丢弃 |

**ContentBlockView**（3 种，AssistantBubble 的内部块）：

| 变体 | 数据 | 说明 |
|------|------|------|
| `Text { raw, rendered, dirty }` | 原文 + markdown 渲染缓存 + 脏标记 | `dirty=true` 时延迟渲染，`ensure_rendered()` 按需调用 |
| `Reasoning { char_count }` | 仅字数 | 不存储推理全文，只显示 "Thought for N chars" |
| `ToolUse { name }` | 工具名 | 仅显示名称，参数在 ToolBlock 中 |

**`append_chunk()` 机制**（`message_view.rs`）：如果最后一个 block 是 `Text`，直接 `push_str` + 标记 `dirty`；否则创建新 `Text` block。`collapsed` 状态在有内容追加时自动展开。

### Done/Interrupted 时的处理流程

```
1. pipeline.done() / pipeline.interrupt()
   ├── finalize_current_ai()：设置 finalized 标志（不清空文本）
   ├── 清理 pending_tools / completed_tools
   └── 清理节流状态

2. request_rebuild()
   ├── build_tail_vms()：从 pipeline 规范状态构建完整尾部
   │   ├── reconcile（has_snapshot=true 时从 completed 重建）
   │   ├── streaming bubble + pending/completed tools
   │   ├── subagent_stack 或 frozen_subagent_vms
   │   └── aggregate_tool_groups()
   └── apply_pipeline_action(RebuildAll { prefix_len, tail_vms })

3. 渲染线程处理 RebuildWithAnchor
   ├── rebuild_all()（含 hash diff 跳过未变更消息）
   ├── 计算锚点消息在新布局中的视觉行位置
   └── cache.scroll_anchor = Some(visual_row)
```

**[TRAP]** `Interrupted`/`Error` 处理器会先调用 `request_rebuild()` 并添加通知消息（`AddMessage`），然后设置 `reconcile_already_done = true`。后续 `Done` 事件检测到该标记后跳过 `request_rebuild()`，只清除 streaming 标志 + `render_rebuild()`——防止 RebuildAll 覆盖已添加的通知消息。

### UI 线程与渲染线程同步

**版本号机制**：`cache.version` 每次更新自增，UI 线程比较 `cache.version != last_render_version` 决定是否调用 `terminal.draw()`。

**双份数据**：主线程维护 `view_messages: Vec<MessageViewModel>`（权威状态），渲染线程维护私有 `messages: Vec<MessageViewModel>`（副本），通过 `RenderEvent` channel 同步。所有消息更新通过 `RebuildWithAnchor`（全量 clone + 发送）确保渲染线程完全同步。

**`parking_lot::RwLock`**：RenderCache 使用 `parking_lot::RwLock`（guard 是 `Send`），而非 `std::sync::RwLock`，确保在 async 上下文中跨 `.await` 持有 guard 不会编译失败。

## Tool Search 延迟加载

非核心工具（MCP 工具、Cron 工具等）从 LLM 工具列表中移除，通过 `SearchExtraTools` 按需发现、`ExecuteExtraTool` 代理执行。

**核心工具（12 个，始终发送给 LLM）**：Read/Write/Edit/Glob/Grep/folder_operations/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite

**架构**：`ReActAgent.with_tool_filter(is_deferred_tool)` 过滤 `tool_refs`（LLM 可见），`with_shared_tools(Arc<RwLock<HashMap>>)` 将所有工具写入共享注册表供 `ExecuteExtraTool` 代理执行。`ToolSearchMiddleware` 注册两个元工具，在 `before_agent` 时从 `shared_tools` 读取 deferred 工具自动构建 `ToolSearchIndex`，然后注入延迟工具列表到 system prompt。

**[TRAP]** `Box<dyn BaseTool>` 不能直接转为 `Arc<dyn BaseTool>`（Rust 不支持 unsized trait object 的所有权转移）。executor 使用 `box_to_arc()` 通过中间 `ToolWrapper` struct（持有 `ManuallyDrop<Box<dyn BaseTool>>`，实现 `BaseTool` trait 透传）安全地完成转换。**绝不能使用 `Box::into_raw` + `Arc::from_raw`**——`Box` 指向 `T`，`Arc` 指向 `ArcInner<T>`，布局不同会导致 UB。

**[TRAP]** `std::sync::RwLockReadGuard` 不是 `Send`，在 async 函数中不能跨 `.await` 持有。使用 `parking_lot::RwLock`（guard 是 `Send`）或在 await 前 clone Arc。

## HITL 审批

默认需审批工具：`Bash`、`folder_operations`、`Agent`、`Write`、`Edit`、`delete_*`、`rm_*`、`mcp__*`、`WebFetch`、`WebSearch`。

## Skills

搜索顺序：`~/.claude/skills/` → `skillsDir`（`~/.peri/settings.json`） → `./.claude/skills/` → 插件 skills，同名先到先得。

**插件 skills 集成**：`SkillsMiddleware.with_extra_dirs()` 是插件 skills 路径注入入口，修改 SkillsMiddleware 搜索逻辑时必须保留此扩展点。

每个 skill 是子目录，内含 `SKILL.md`（YAML frontmatter: `name`, `description`）。输入 `/` 前缀触发 Skills 浮层，Tab 导航，Enter 补全为 `/skill-name`。

**Frontmatter 解析**：skill 和插件命令的 Markdown 文件使用 `gray_matter` crate（YAML engine）解析 frontmatter。新增命令解析时必须复用 `Matter::<YAML>::new()` 模式，不手动解析 `---` 分隔符。

## Fork 模式

子 agent 继承父 agent 的完整消息历史 + system prompt + 工具集，通过 fork directive 规则约束防递归。

## Background Agent 模式

后台 agent 通过独立事件通道 + 通知通道完成（不共享父 event_handler），最大 3 个并发。父 agent Done 后若有后台任务，自动保持通道存活并在最后一个完成时触发 continuation。

## 中间件链执行顺序

```
1. AgentDefineMiddleware      ← 解析 agent 定义，设置 model/maxTurns 等覆盖
2. AgentsMdMiddleware         ← 读 CLAUDE.md/AGENTS.md 注入 system
3. SkillsMiddleware           ← Skills 摘要注入 system
4. SkillPreloadMiddleware     ← #skill-name 全文注入（fake tool 序列）
5. PluginMiddleware           ← 插件命令注入 system
6. HookMiddleware             ← 插件 hooks 事件拦截
7. FilesystemMiddleware       ← 6 个文件系统工具（Read/Write/Edit/Glob/Grep/folder_operations）
8. TerminalMiddleware         ← Bash 工具
9. WebMiddleware             ← WebFetch/WebSearch 工具
10. TodoMiddleware             ← after_tool 解析 TodoWrite
11. CronMiddleware             ← Cron 调度工具
12. LspMiddleware              ← LSP 工具 + after_tool 文件变更同步
13. HumanInTheLoopMiddleware   ← before_tool 拦截敏感工具
14. SubAgentMiddleware        ← Agent 工具
15. McpMiddleware             ← MCP 工具和资源注入（仅 pool 初始化成功时注册）
[ReActAgent.with_system_prompt()] ← system prompt prepend
```

手动注册工具（`register_tool`）优先级最高，覆盖同名中间件工具。

## 上下文压缩

Token 累积达到上下文窗口阈值（默认 85%）时自动触发：

1. **Micro-compact**：零 API 调用，清除可压缩工具结果/图片/文档
2. 如仍超限 → **Full Compact**：LLM 生成 9 段结构化摘要替换历史
3. **Re-inject**：重新注入最近文件 + Skills

## MCP 中间件

通过 `McpMiddleware` 将外部 MCP 服务器提供的工具和资源注入 ReAct 循环。基于 `rmcp` crate 实现。

**配置加载**：`McpConfig::load_merged_config(cwd)` 合并两层配置：

| 来源 | 路径 | 说明 |
|------|------|------|
| 全局 | `~/.peri/settings.json` 的 `config.mcpServers` 或 `mcpServers` | 所有项目共享 |
| 项目级 | `{cwd}/.mcp.json` 的 `mcpServers` | 项目特定，同名覆盖全局 |

**服务器配置**（`McpServerConfig`）：

| 字段 | 说明 |
|------|------|
| `command` + `args` + `env` | stdio 传输：启动子进程 |
| `url` + `headers` | Streamable HTTP 传输：连接远程服务器 |
| `oauth` | OAuth 2.0 认证配置（`authorizationUrl`/`tokenUrl`/`clientId`/`clientSecret`/`scopes`） |
| `disabled` | 设为 `true` 禁用该服务器（TUI 面板可切换） |
| `${VAR}` 占位符 | 所有字符串字段自动展开环境变量 |

**工具命名**：`mcp__{server_name}__{tool_name}`，HITL 对 `mcp__` 前缀的工具默认需审批。

**插件 MCP 命名空间**：插件定义的 MCP 服务器使用 `{plugin_name}__{server_name}` 前缀命名空间，`ConfigSource::Plugin` 标记来源，合并后工具名为 `mcp__{plugin_name}__{server_name}__{tool_name}`。新增 MCP 配置合并逻辑时必须遵循此前缀规则。

**插件 MCP Env 展开**：必须在合并之前执行（per-plugin 独立上下文），避免不同插件的同名 env var 交叉污染。`expand_server_config_with_context` 在 Step 2 立即调用，Plugin 来源的 server config 在合并到 `merged.mcp_servers` 之前已完成 env 变量替换。

**ClaudeSettings 反序列化陷阱 [TRAP]**：`extraKnownMarketplaces` 字段在 Claude Code 中可能是对象格式 `{"name": {source, ...}}`。`deserialize_known_marketplaces` 自定义反序列化器需同时支持对象和数组两种格式，否则整个 `ClaudeSettings` 解析失败会导致 `load_enabled_plugins` 静默失败、插件 MCP 服务器全部丢失。同样，`enabledPlugins` 也有对象/数组两种格式（已有 `deserialize_enabled_plugins` 处理）。**`enabledPlugins` 写入必须用对象格式** `{"id": true}`，数组格式会导致 Claude Code 报 `Expected record, but received array`。

**Plugin Sources 旁路表**：`load_merged_config_full` 返回 `(McpConfigFile, HashMap<String, String>)`，其中 `HashMap` 的 key 格式为 `"plugin:{name}:{server}"`（与合并后 config 中的 server name 一致），value 为 `"name@marketplace"`。`LoadedPlugin` 当前没有 `marketplace` 字段，marketplace 信息只能从 `InstalledPlugin`（`installed_plugins.json`）中获取。`load_installed_plugins` 读取路径需从 `claude_home` 参数推导（`claude_home/plugins/installed_plugins.json`），不能使用 `None`（会读 `~/.claude/plugins/installed_plugins.json`）。

**资源读取**：`mcp__read_resource` 工具，参数 `server_name` + `uri`，120 秒超时。

**连接池**（`McpClientPool`）：

- 首次 agent 启动时惰性初始化（`agent_ops.rs`），后续复用
- stdio 连接超时 10 秒，HTTP 连接超时 30 秒
- 连接失败的 server 记录为 `Failed` 状态，不影响其他 server
- App 退出时调用 `pool.shutdown()` 优雅关闭所有连接

**代码结构**（`rust-agent-middlewares/src/mcp/`）：

| 文件 | 职责 |
|------|------|
| `config.rs` | 配置加载、合并、`${VAR}` 展开 |
| `transport.rs` | 传输层工厂（stdio / StreamableHTTP） |
| `client.rs` | 连接池管理、HTTP headers 注入 |
| `tool_bridge.rs` | MCP 工具 → `BaseTool` 桥接 |
| `resource_tool.rs` | MCP 资源读取工具 |
| `middleware.rs` | `Middleware` trait 实现，`collect_tools` 注入 |

## 插件系统

插件机制兼容 Claude Code 插件生态，支持命令、hooks、MCP 服务器、LSP 服务器、skills 和 agents 的注入。

**核心模块**（`rust-agent-middlewares/src/plugin/`）：

| 文件 | 职责 |
|------|------|
| `config.rs` | Claude Code 配置加载（settings.json/installed_plugins.json/known_marketplaces.json） |
| `types.rs` | 类型定义：`PluginManifest`、`InstalledPlugin`、`MarketplacePlugin`、`McpServerEntry`（支持内联 Config 和 FilePath 两种格式） |
| `loader.rs` | 加载已启用插件：`load_enabled_plugins()`、`LoadPlugin` 包含 commands/skills_dirs/agents_dirs/mcp_servers/hooks_config |
| `installer.rs` | 插件安装/卸载/更新：支持 GitHub/git/url/file/directory/NPM 来源 |
| `marketplace.rs` | Marketplace 管理：`MarketplaceManager` 管理多个 marketplace 入口 |
| `install_counts.rs` | 安装次数缓存 |
| `middleware.rs` | `PluginMiddleware`——into system prompt（commands 列表） |

**配置来源**：

| 来源 | 路径 | 说明 |
|------|------|------|
| 全局 | `~/.peri/settings.json` | 全局 settings，ClaudeCode 格式相同字段 |
| 插件 | 安装在 `~/.claude/plugins/cache/` | `plugin.json` manifest 驱动 |

**插件清单字段**（`PluginManifest`）：`commands`、`agents`、`skills`、`hooks`、`mcpServers`、`lspServers`、`channels`、`options`、`settings`。

**`McpServerEntry`**：插件 MCP 服务器支持内联配置直接写入 manifest，或 `.mcp.json` 文件路径引用。枚举 `Config(McpServerConfig)` / `FilePath(String)`。

**Hooks（`rust-agent-middlewares/src/hooks/`）**

对齐 Claude Code hooks 系统——4 种执行类型：

| 类型 | 说明 |
|------|------|
| `Command` | Shell 命令（bash/powershell），同步/异步，通过 exit code 控制流程 |
| `Prompt` | LLM 提示词评估 |
| `Http` | HTTP POST |
| `Agent` | 子 agent 完整循环 |

**支持的事件**（`HookEvent`）：`PreToolUse`、`PostToolUse`、`PostToolUseFailure`、`PermissionRequest`、`UserPromptSubmit`、`SessionStart`、`SessionEnd`、`Stop`、`StopFailure`、`SubagentStart`、`SubagentStop`、`PreCompact`、`PostCompact`、`Notification`。

**Hook 流程控制**：通过 exit code（Command）或 JSON response 控制：
- exit 0 / `continue: true` → Allow
- exit 1 → Allow with warning
- exit 2 / `continue: false` → Block
- stdout JSON 的 `hook_specific_output.hookEventName.PreToolUse.updatedInput` → ModifyInput

**PermissionRequest 门控**：仅对敏感工具触发（同 HITL 列表），不查 permission_mode——YOLO 模式也触发以便日志/观察。

**SSRF 防护**（`hooks/ssrf_guard.rs`）：阻止对内网私有地址（10.0.0.0/8、172.16.0.0/12、192.168.0.0/16、100.64.0.0/10、169.254.0.0/16）的 HTTP hook 请求，回环地址（127.0.0.1、::1）允许（本地开发）。

**变量替换**（`hooks/variables.rs`）：`${CLAUDE_PROJECT_DIR}`、`${CLAUDE_PLUGIN_ROOT}`、`${CLAUDE_PLUGIN_DATA}`、`${ARGUMENTS}` 等占位符展开。

## LSP 中间件

LSP 支持通过 `LspMiddleware` + `LspTool` + `perihelion-lsp` 客户端库实现。

**`perihelion-lsp` crate**（`perihelion-lsp/src/`）：

| 文件 | 职责 |
|------|------|
| `client.rs` | `LspClient`——LSP 协议客户端（initialize/didOpen/didChange/didSave/shutdown）和通用 `request`/`notification` |
| `pool.rs` | `LspServerPool`——按文件扩展名映射 LSP 服务器，按需自动初始化 |
| `config.rs` | 配置加载：`LspConfigFile`（`{cwd}/.peri/lsp.json`）和 `LspServerConfig` |
| `diagnostics.rs` | 诊断缓存（`DiagnosticsCache`）——按 URI 存储最近诊断结果 |
| `error.rs` | `LspClientError` |
| `jsonrpc/` | JSON-RPC 2.0 协议编解码器（`codec.rs`）、消息类型（`message.rs`）、transport（`transport.rs`——Line-based TCP transport） |
| `protocol/` | LSP 协议通知/请求类型封装（复用 `lsp-types`） |

**`LspMiddleware`**（`rust-agent-middlewares/src/lsp/`）：提供 `LSP` 工具（goToDefinition/findReferences/hover/documentSymbol/workspaceSymbol/diagnostics 等 10 种操作），并在 `after_tool` 中自动同步 Write/Edit 后的文件变更到 LSP 服务器（`didChange` + `didSave`）。

**`LspTool` 操作**：goToDefinition、findReferences、hover、documentSymbol、workspaceSymbol、goToImplementation、prepareCallHierarchy、incomingCalls、outgoingCalls、diagnostics。

## peri-cli（Node.js CLI）

Node.js CLI 工具（`peri-cli/`），通过 GitHub Releases 分发项目二进制。使用 `commander` 框架：

```bash
peri install [package]   # 安装指定包（agent 或完整标签 agent-v1.17）
peri list                # 列出 GitHub 可用版本（top 5）
peri update              # 升级到最新版本
peri add-env             # 将 peri 二进制添加到 PATH
peri uninstall           # 卸载并清理
peri clean               # 清理旧版本，每个包保留最新 2 个
```

位于 `peri-cli/`，非 workspace 成员，独立管理。

## SubAgents（子 Agent 委派）

`Agent` 工具允许 LLM 将子任务委派给 `.claude/agents/{agent_id}/agent.md` 定义的专门 agent 执行。插件 agent 通过 `scan_agents_with_extra_dirs` 追加搜索路径，同名 agent_id 去重（项目级优先），系统提示词中的 agent 列表由 `prompt.rs` 的 `format_available_agents` 生成。

**工具过滤规则**：

- `tools` 字段为空 → 子 agent 继承所有父工具（排除 `Agent` 自身，防递归）
- `tools` 字段有值 → 仅保留允许列表中的工具
- `disallowedTools` 字段 → 额外排除指定工具

**返回值格式**：

```
[子 agent 执行了 N 个工具调用: tool1, tool2, tool3]

Final response text here
```

## TUI 命令

输入 `/` 前缀触发统一浮层，Tab 导航，Enter 补全，支持前缀唯一匹配。

| 命令 | 说明 |
|------|------|
| `/login` | 管理 Provider 配置 |
| `/model` | 模型选择面板 |
| `/model <alias>` | 直接切换（opus/sonnet/haiku） |
| `/history` | 历史对话浏览 |
| `/agents` | SubAgent 定义管理 |
| `/compact` | 上下文压缩 |
| `/clear` | 清空消息列表 |
| `/config` | 查看/编辑运行时配置 |
| `/cost` | 查看 token 用量和成本 |
| `/context` | 查看上下文窗口使用情况 |
| `/memory` | 管理持久化记忆 |
| `/help` | 命令列表 |

## TUI Headless 测试模式

`rust-agent-tui` 支持无真实终端的 headless 集成测试。

```rust
#[tokio::test]
async fn test_example() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 必须在发送事件前注册监听
    let notified = handle.render_notify.notified();

    app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    notified.await;  // 等待渲染线程处理完成

    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
    assert!(handle.contains("Hello"));
}
```

**注意事项：**

- `notified()` 必须在 `process_pending_events()` **之前**调用
- `AssistantChunk` 事件会发送 2 个 `RenderEvent`
- CJK 字符在 `TestBackend` 中有宽字符填充，断言应使用 ASCII 内容
- 测试位于 `rust-agent-tui/src/ui/headless.rs`

## 关键模式

```rust
// 组装 agent（系统提示词通过 with_system_prompt() 注入）
ReActAgent::new(BaseModelReactLLM::new(model))
    .max_iterations(50)
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .register_tool(Box::new(AskUserTool::new(invoker)))
    .with_event_handler(Arc::new(FnEventHandler(move |ev| { tx.try_send(ev); })))
    .execute(AgentInput::text(input), &mut AgentState::new(cwd))
```

**SubAgent 委派：**

```rust
let parent_tools: Arc<Vec<Arc<dyn BaseTool>>> = Arc::new(
    FilesystemMiddleware::new().tools(cwd)
        .into_iter()
        .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
        .collect()
);
let llm_factory = Arc::new(move || {
    Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
});
let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
    build_system_prompt(overrides, cwd, PromptFeatures::detect())
});
ReActAgent::new(llm)
    .add_middleware(Box::new(
        SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
            .with_system_builder(system_builder)
    ))
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE=true` | 默认行为，跳过 HITL 审批（不影响 AskUserQuestion） |
| `YOLO_MODE=false` | 启用 HITL 审批 |
| `RUST_LOG` | 日志级别（默认 `info`） |
| `RUST_LOG_FILE` | 日志文件路径 |
| `RUST_LOG_FORMAT=json` | 使用 JSON 格式输出日志 |
| `LANGFUSE_*` | Langfuse 追踪配置 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry OTLP 导出端点 |

配置通过 `~/.peri/settings.json` 的 `env` 字段注入环境变量（已替代 .env 文件）。

## CLI 参数

| 参数 | 说明 |
|------|------|
| `-a, --approve` | 启用 HITL 审批（设置 `YOLO_MODE=false`） |

运行时 `Shift+Tab` 循环切换 5 级权限模式，`Alt+M` 循环切换模型（opus→sonnet→haiku）。

**多 session 分屏**：支持多个 agent session 并列分屏显示，外层彩色边框指示当前聚焦 session。

**Ctrl+C 中断恢复**：中断 agent 执行时，已输入的用户文本自动恢复到输入框。

## 编码规范

- Rust 2021 edition，tokio async/await + async-trait
- 库 crate 用 `thiserror`，应用层用 `anyhow::Result`
- 日志用 `tracing` 宏，禁止 `println!`/`eprintln!`
- 单元测试：测试代码与源码分离为同目录 `_test.rs` 文件（见「测试规范」）
- bin crate 集成测试在 `src/` 内（不支持 `tests/` 目录）
- 文件组织：每模块一目录，`mod.rs` 入口
- Workspace resolver = "2"，禁止下层 crate 依赖上层
- 禁止使用 `ℹ`（U+2139）符号和 `[i]` 前缀，系统消息无需额外前缀标记
- **字符串截断必须用字符级操作**：`&s[..N]` 按字节切片，CJK 字符占 3 字节，N 值落在多字节字符内部会 panic。应使用 `s.chars().take(N).collect::<String>()` 或 `s.char_indices().nth(N)` 做字符边界安全的截断。`s.len()` 返回字节数，`s.chars().count()` 返回字符数，截断长度判断也必须用字符数。

## 测试规范

### 文件分离

**规则**：测试代码独立为同目录 `{source}_test.rs` 文件，源文件末尾保留最小引用声明。

**命名**：`foo.rs` → `foo_test.rs`，`mod.rs` → `mod_test.rs`。

**源文件末尾声明**（3 行固定格式）：
```rust
#[cfg(test)]
#[path = "foo_test.rs"]
mod tests;
```

**测试文件开头**（固定导入）：
```rust
use super::*;
```

**阈值**：测试代码 ≥ 30 行时必须分离。少于 30 行允许 inline `mod tests {}`。

### 现有已分离文件（无需迁移）

| 源文件 | 测试文件 |
|--------|----------|
| `rust-create-agent/src/llm/openai.rs` | `openai_test.rs` |
| `rust-create-agent/src/agent/executor/mod.rs` | `mod_test.rs` |
| `rust-agent-middlewares/src/subagent/fork.rs` | `fork_test.rs` |
| `rust-agent-middlewares/src/subagent/tool.rs` | `tool_test.rs` |
| `rust-agent-middlewares/src/plugin/marketplace.rs` | `marketplace_test.rs` |
| `rust-agent-middlewares/src/plugin/loader.rs` | `loader_test.rs` |
| `rust-agent-middlewares/src/plugin/installer.rs` | `installer_test.rs` |
| `rust-agent-middlewares/src/mcp/config.rs` | `config_test.rs` |
| `rust-agent-middlewares/src/hooks/middleware.rs` | `middleware_test.rs` |
| `rust-agent-tui/src/app/message_pipeline.rs` | `message_pipeline_test.rs` |
| `rust-agent-tui/src/app/setup_wizard.rs` | `setup_wizard_test.rs` |
| `langfuse-client/src/types.rs` | `types_test.rs` |
| `rust-agent-tui/src/ui/headless.rs` | `headless_test.rs` |

### 待迁移文件（测试 ≥ 100 行，按优先级排序）

| 测试行数 | 源文件 | 目标文件 |
|----------|--------|----------|
| 400 | `rust-agent-tui/src/config/types.rs` | `config/types_test.rs` |
| 398 | `rust-create-agent/src/agent/compact/full.rs` | `compact/full_test.rs` |
| 388 | `rust-create-agent/src/agent/token.rs` | `token_test.rs` |
| 375 | `rust-agent-middlewares/src/hitl/mod.rs` | `hitl/mod_test.rs` |
| 357 | `rust-create-agent/src/agent/compact/re_inject.rs` | `compact/re_inject_test.rs` |
| 333 | `rust-create-agent/src/llm/anthropic.rs` | `llm/anthropic_test.rs` |
| 327 | `rust-agent-middlewares/src/hooks/executor.rs` | `hooks/executor_test.rs` |
| 319 | `rust-create-agent/src/middleware/chain.rs` | `middleware/chain_test.rs` |
| 313 | `rust-create-agent/src/agent/compact/micro.rs` | `compact/micro_test.rs` |
| 310 | `langfuse-client/src/batcher.rs` | `batcher_test.rs` |
| 304 | `perihelion-widgets/src/markdown/mod.rs` | `markdown/mod_test.rs` |
| 298 | `rust-agent-middlewares/src/plugin/types.rs` | `plugin/types_test.rs` |
| 286 | `rust-agent-tui/src/command/mod.rs` | `command/mod_test.rs` |
| 276 | `rust-create-agent/src/agent/compact/invariant.rs` | `compact/invariant_test.rs` |
| 268 | `rust-agent-middlewares/src/subagent/mod.rs` | `subagent/mod_test.rs` |
| 263 | `rust-agent-middlewares/src/plugin/config.rs` | `plugin/config_test.rs` |
| 255 | `rust-create-agent/src/messages/content.rs` | `messages/content_test.rs` |

### 测试编写风格

**语言**：注释、文档注释、断言消息统一使用中文。

**命名**：`test_<被测对象>_<场景>`，snake_case。
```rust
#[test]
fn test_content_block_text_序列化往返() { ... }

#[tokio::test]
async fn test_tool_search_超时返回空结果() { ... }
```

**结构**：Arrange-Act-Assert，无空行分隔三段。
```rust
#[test]
fn test_parse_config_缺少必要字段返回错误() {
    // Arrange
    let input = r#"{"command": "ls"}"#;

    // Act
    let result = parse_config(input);

    // Assert
    assert!(result.is_err(), "缺少 args 字段应返回错误");
}
```

**断言**：
- 优先 `assert_eq!` / `assert!`
- 模式匹配用 `assert!(matches!(value, Pattern))`
- 错误消息用 `assert!` 的第二个参数（中文描述），不用 `.expect()`
- `.unwrap()` 用于构造测试数据（测试前置条件），不用于被测代码的结果断言

**Mock/构造器**：测试文件内定义私有的 helper 函数和 Mock struct，不跨文件共享。命名以 `make_` 或 `mock_` 前缀。
```rust
fn make_tool_call(id: &str, name: &str) -> ToolCallRequest {
    ToolCallRequest::new(id, name, json!({}))
}
```

**异步测试**：使用 `#[tokio::test]`，单线程足够时用 `#[tokio::test(flavor = "multi_thread")]` 显式标注。

**测试隔离**：
- 文件系统操作用 `tempfile::NamedTempFile`
- 环境变量用唯一名称（加测试名前缀）避免并行冲突
- 不使用 `serial_test`——通过命名空间隔离

**最小依赖**：仅使用 `assert!`/`assert_eq!`/`matches!` + `tempfile` + `tokio-test`，不引入 `insta`/`rstest`/`pretty_assertions`/`proptest` 等额外测试库。

**集成测试**：`tests/` 目录仅用于跨 crate 的端到端测试，单个 crate 内部测试用 `_test.rs`。

## 开发注意事项

- **BaseMessage 与 MessageViewModel 维度混淆 [TRAP]**：`MessagePipeline.completed_len_at_round_start` 追踪的是 `BaseMessage` 数组长度，但 `RebuildAll { prefix_len }` 中的 `prefix_len` 是 `MessageViewModel` 数组索引。由于 BaseMessage → MessageViewModel 不是 1:1 映射（相邻只读工具聚合为 `ToolCallGroup`、SubAgent 表示为单个 `SubAgentGroup`），两者长度可能不同。
  - **错误模式**：使用 `completed_len_at_round_start` 作为 `prefix_len` 或用于切片 `&self.completed[completed_len_at_round_start..]`
  - **正确做法**：
    - `prefix_len` 应使用 `SessionMessages.round_start_vm_idx`（VM 维度）
    - `MessagePipeline` 内部切片前必须用 `.min(self.completed.len())` 保护
    - `MessagePipeline` 无法访问 `round_start_vm_idx` 时，使用 `prefix_len=0` 作为安全回退
    - `apply_pipeline_action` 的 `drain(prefix_len..)` 必须用 `prefix_len.min(view_messages.len())` 钳位，越界时记录 `tracing::error!` 而非 panic
  - **禁止从 Pipeline 内部返回 `RebuildAll`**：`MessagePipeline.handle_event()` 不得返回 `PipelineAction::RebuildAll`——Pipeline 不拥有 `round_start_vm_idx`，无法生成正确的 `prefix_len`。之前 `ToolStart` 在 `throttle_armed` 时返回 `RebuildAll { prefix_len: 0 }`，导致 `view_messages` 被全部替换后，`agent_ops` 的 `request_rebuild()` 用旧的 `round_start_vm_idx` 做 `drain` 时越界 panic。正确的做法是仅解除 `throttle_armed`，由 `request_rebuild()` 以正确的 `prefix_len` 触发重建。
  - **历史教训**：此问题已出现三次——第一次在 `build_tail_vms()` 切片越界，第二次在 `ToolStart` 事件处理器返回 `RebuildAll { prefix_len: 0 }` 导致 `apply_pipeline_action` 的 `drain` 越界 panic，第三次同样是 `prefix_len` 越界但已通过钳位+error 日志降级处理。

- **新增弹窗面板**：`Event::Paste` 独立于 key event 链，必须在该分支单独拦截；`Ctrl+V` 需在 `handle_xxx_panel` 内单独处理。
- **EditField 导航**：`next()/prev()` 链必须与表单实际渲染字段一致。
- **快捷键设计**：禁止使用 `Shift + 字母`（A-Z）组合。编辑状态下 `Shift+字母` 等同于输入大写字母，二者不可区分。全局操作用 `Ctrl + 字母` 或功能键，面板操作用 `↑/↓`、`Space`、`Enter`、`Esc`。
- **字符串显示宽度**：终端列宽计算使用 `unicode-width` crate（`UnicodeWidthStr::width()` / `UnicodeWidthChar::width()`），CJK 等全角字符占 2 列。面板列表项截断需基于显示宽度而非 `char` 数量。
- **测试隔离——禁止写入全局配置**：`config::save()` 默认写入 `~/.peri/settings.json`。Headless 测试（`new_headless`）通过 `App.config_path_override` 将保存路径重定向到临时目录。新增面板操作方法若需持久化配置，必须调用 `App::save_config(cfg, self.config_path_override.as_deref())` 而非直接调用 `crate::config::save(cfg)`，否则测试会覆盖用户的真实 Provider/API Key 配置。
- **`CommandRegistry::dispatch` 借用限制 [TRAP]**：`dispatch(&self, app: &mut App)` 同时需要 `&self`（registry）和 `&mut App`，由于 registry 通过 session 嵌套在 App 内，Rust 借用检查器无法通过字段投影拆分。当前通过 `std::mem::take` + put-back workaround 解决。若要消除此 workaround，需重构 `dispatch` 签名（如改为传入 `&CommandRegistry` + 独立的 `&mut` 参数而非整个 `&mut App`）。

## 面板系统

`PanelManager` + `PanelComponent` trait 组件化架构（`app/panel_manager.rs` / `app/panel_component.rs`）。新增面板只需定义 `PanelState` 变体 + 实现 trait，无需修改 `event.rs` / `status_bar.rs` / `main_ui.rs`。

- **双作用域**：`session_panels`（Session-scoped）和 `global_panels`（Global-scoped），`App::open_panel()` 自动处理跨作用域互斥
- **统一入口**：`open_panel(PanelKind)` / `close_all_panels()`，业务逻辑在 `panel_ops.rs`
- **特殊面板**（不纳入 PanelManager）：Setup Wizard、OAuth Prompt、Interaction Prompts——全屏覆盖或 agent/MCP 触发，在 `event.rs` 中优先处理
- **快捷键提示**：面板内部禁止渲染提示行，统一由状态栏通过 `status_bar_hints()` 自描述，新增面板实现该方法即可
