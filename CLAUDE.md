# CLAUDE.md

## 项目概述

Rust Agent 框架，6 个 Workspace Crate + 1 个独立 Node.js CLI（`peri-cli/`）。

| Crate | 职责 |
|-------|------|
| `rust-create-agent` | 核心：ReAct 循环、Middleware trait、LLM 适配器、工具系统、持久化（SQLite）、遥测 |
| `rust-agent-middlewares` | 中间件：文件系统、终端、HITL、SubAgent、Skills、Todo、Cron、MCP、Hooks、Plugin、LSP |
| `perihelion-widgets` | Widget 组件库（14 组件），仅依赖 ratatui + pulldown-cmark |
| `rust-agent-tui` | TUI 应用，依赖 widgets + middlewares |
| `langfuse-client` | Langfuse 遥测客户端（独立） |
| `perihelion-lsp` | LSP 客户端库（独立，被 middlewares 使用） |

`rmcp` crate（v1.6.0）通过 `[patch.crates-io]` 指向本地 `rust-mcp-patch/`，上游修复后删除补丁目录即可。

**其他目录**：`peri-cli/`（Node.js CLI，版本管理/安装工具）、`scripts/`（启动脚本）、`peri-control/`、`peri-workflow-engine/`、`side-projects/`（实验性/空壳，未纳入 workspace）。

## 依赖关系

```
rust-create-agent → rust-agent-middlewares → rust-agent-tui
                   ↗ perihelion-lsp       ↗ perihelion-widgets
langfuse-client（独立）  peri-cli（独立，Node.js）
```

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p <crate>               # 构建指定 crate
cargo run -p rust-agent-tui          # 运行 TUI
cargo run -p rust-agent-tui -- -a    # HITL 审批模式
cargo test                           # 全量测试
cargo test -p <crate> --lib -- <test_name>  # 单个测试
lefthook install                     # 安装 git hooks
lefthook run pre-commit              # pre-commit（fmt/check/clippy）
scripts/start-tui.sh                 # 启动 TUI 并连接本地 Relay
scripts/start-relay.sh               # 启动 Relay Server（端口 8080）
```

**peri-cli**（Node.js）：`install`/`update`/`list`/`add-env`/`uninstall`/`clean`，用于版本管理和安装。

## 架构要点

**ReAct 循环**（`rust-create-agent`）：AgentInput → collect_tools → before_agent → loop(500) { LLM → [工具调用] before_tool → 并发执行 → after_tool → emit | [回答] → emit TextChunk + StateSnapshot → after_agent }。TUI 覆盖 `max_iterations(500)`（核心默认 10）。

**[TRAP]** `tool_dispatch.rs` 采用"先写 AI 消息（tool_use），后补全 tool_result"的两阶段写入模式。AI 消息在第 37 行写入 state，tool_result 在阶段三（第 253 行）才写入。中间隔着 before_tool 循环（含 P1 cancel / P3 ToolRejected / P4 其他错误）和并发执行两个阶段。**任何新增的错误提前退出路径都必须同时 flush `modified_calls`（已通过 before_tool 但未执行）和 `original_calls[i..]`（尚未处理），否则会产生孤儿 tool_use 导致 Anthropic API 400。** `flush_modified_tool_errors` 处理前者（只 emit ToolEnd，不重复 ToolStart），`flush_pending_tool_errors` 处理后者（emit ToolStart + ToolEnd）。此模块已因此 bug 修复 4 次（f138b21, 7f3ad00, 8d6bb1b 及更早），根本解决需实施延迟写入重构（将 AI 消息推迟到所有 tool_result 准备好后一起写入）。该函数有 8 条执行路径、4 处重复的 tool_result 写入代码（ToolRejected 内联 / flush_modified / flush_pending / Phase3 正常路径），修改一处必须检查所有其他处。取消路径与错误路径做相同的事（双 flush）但代码独立维护，修改必须同步。

**[TRAP]** 新增/修改事件类型语义（如工具前文本从 AiReasoning 改为 TextChunk）时，必须同步检查 TUI 侧事件映射层（`map_executor_event`）。新增 ExecutorEvent 变体时必须同步更新映射，事件丢弃会导致下游状态不一致。（详见 spec/global/domains/agent.md#issue_2026-05-11-streaming-text-invisible-with-tools，spec/global/domains/message-pipeline.md#issue_2026-05-13-streaming-text-tool-aggregation-visual-issues）

**消息类型**：`BaseMessage`（Human/Ai/System/Tool），`ContentBlock`（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）。

**LLM 适配层**：`BaseModel` trait（OpenAI/Anthropic）→ `BaseModelReactLLM` → `ReactLLM`。`RetryableLLM<L>` 指数退避重试。

**TUI 消息渲染**（`rust-agent-tui`）：所有消息更新通过统一 `RebuildAll` 路径触发（无增量更新）。`MessagePipeline`（`message_pipeline.rs`）维护规范状态，`build_tail_vms()` 构建尾部 VMs，`messages_to_view_models()` 是唯一转换入口。流式文本通过 100ms 节流触发 RebuildAll，非流式事件立即触发。独立 `RenderThread` 处理渲染，通过 `RenderCache(RwLock)` 与 UI 线程同步。

**[TRAP]** Ephemeral VM（SystemNote/CacheWarning）依赖锚点机制：`ephemeral_notes` 记录插入位置，RebuildAll 时根据锚点与 `prefix_len` 关系决定保留/重插入。新增 ephemeral VM 类型必须同步更新过滤逻辑。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild）

**系统提示词**：`build_system_prompt(overrides, cwd, features)` 合成，段落文件位于 `rust-agent-tui/prompts/sections/`（共 11 个：01-07 + 10-13）。`PromptFeatures` 控制条件段落注入。静态段落（01-06）与动态段落（07_env + feature-gated 10-13）通过 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔——标记前的内容被 Anthropic prompt cache 命中，标记后的内容变化不影响前缀缓存。`messages_to_anthropic()` 中 `split_system_blocks()` 负责拆分。

## Thinking/推理模式

`ThinkingConfig` 控制推理参数。Anthropic 用 `thinking + output_config.effort`，OpenAI 用 `reasoning_effort`。`budget_tokens` 最小 1024，`max_tokens` 必须 > `budget_tokens`。

**OpenAI Reasoning 回传**（`openai.rs`）：
- `reasoning_content` 顶层字段：所有模型无条件回传
- content 数组 `thinking` 类型：仅 deepseek-v4-pro（`supports_thinking_content` 标志控制）

**[TRAP]** DeepSeek `unknown variant 'thinking'`：不要把 `Reasoning` block 序列化为 `{"type":"thinking"}` 发给不支持的 provider。**[TRAP]** `reasoning_content must be passed back`：过滤 `Reasoning` 时必须同时作为顶层字段回传。两个陷阱互相关联。（详见 spec/global/domains/agent.md#issue_2026-05-12-glm-reasoning-field-not-parsed，spec/global/domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped）

## Tool Search 延迟加载

非核心工具通过 `SearchExtraTools` 按需发现、`ExecuteExtraTool` 代理执行。核心工具（12 个）：Read/Write/Edit/Glob/Grep/folder_operations/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite。

**[TRAP]** `Box<dyn BaseTool>` 不能直接转 `Arc<dyn BaseTool>`，用 `box_to_arc()` 通过 `ToolWrapper(ManuallyDrop<Box>)` 透传。**绝不能用 `Box::into_raw` + `Arc::from_raw`**——布局不同导致 UB。

**[TRAP]** Prompt Cache 前缀稳定性——通用原则：所有参与缓存前缀的数据（system prompt、tools 数组、消息顺序）必须保证跨请求稳定。具体规则：
- （a）system prompt 中用 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔静态/动态内容，标记前可缓存，标记后不缓存
- （b）优先用 `add_message`（尾部追加）而非 `prepend_message`（头部插入）
- （c）动态占位符（日期、cwd、环境变量）放在边界标记之后
- （d）middleware 注入的 System 消息天然在边界标记之后（非缓存块）

三个已踩坑的违反模式：（1）HashMap 迭代顺序不确定导致序列化内容跨进程变化；（2）`prepend_message` 向消息头部插入内容改变了 `cache_control` 标记的第一条 user 消息位置；（3）system prompt 内动态占位符（`{{date}}` 每日变化、`{{cwd}}` 跨项目变化）导致整个缓存段失效。（详见 spec/global/domains/message-pipeline.md，spec/global/domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency）

**[TRAP]** `prepend_message` 的 `insert(0)` 右移导致 StateSnapshot 快照范围扩大，泄露 System 消息到 `agent_state_messages`。StateSnapshot 应始终 `.filter(|m| !m.is_system())`，`agent_state_messages` 不应包含 System 变体。（详见 spec/global/domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls）

## 中间件链执行顺序

```
1.  AgentsMdMiddleware       ← CLAUDE.md/AGENTS.md 注入
2.  AgentDefineMiddleware    ← agent 定义，model/maxTurns 覆盖
3.  SkillsMiddleware         ← Skills 摘要注入（含插件 extra_dirs）
4.  SkillPreloadMiddleware   ← #skill-name 全文注入
5.  FilesystemMiddleware     ← 6 个文件系统工具
6.  TerminalMiddleware       ← Bash
7.  WebMiddleware            ← WebFetch/WebSearch
8.  TodoMiddleware           ← after_tool 解析 TodoWrite
9.  CronMiddleware           ← Cron 调度
10. HookMiddleware           ← hooks 事件拦截（多组实例）
11. HumanInTheLoopMiddleware ← before_tool 拦截
12. SubAgentMiddleware       ← Agent 工具
13. McpMiddleware            ← MCP 工具和资源（pool 成功时注册）
14. ToolSearchMiddleware     ← SearchExtraTools/ExecuteExtraTool 代理
15. LspMiddleware            ← LSP 工具 + after_tool 文件变更同步
[ReActAgent.with_system_prompt()] ← prepend
```

插件通过 `plugin_skill_dirs` → `SkillsMiddleware.with_extra_dirs()`、`plugin_hooks` → `HookMiddleware` 注入，无独立 PluginMiddleware。

## HITL 审批

默认需审批：`Bash`、`folder_operations`、`Agent`、`Write`、`Edit`、`delete_*`、`rm_*`、`mcp__*`、`WebFetch`、`WebSearch`。

## 上下文压缩

Token 达到上下文窗口 85% 时自动触发：Micro-compact（清除可压缩工具结果）→ Full Compact（LLM 生成 9 段摘要）→ Re-inject（最近文件 + Skills）。

触发阈值：0.70 micro-compact（清除 ≥5 步前的 Bash/Read/Glob/Grep/Write/Edit 结果），0.85 full compact。环境变量覆盖：`DISABLE_COMPACT`、`DISABLE_AUTO_COMPACT`、`COMPACT_THRESHOLD`（0.0-1.0）。

**[TRAP]** Compact 是跨异步操作：状态依赖（如 resubmit 所需的原始输入）应在操作开始时保存到独立字段。Full compact 后 `prefix_len: 0` 时必须清理过期 `ephemeral_notes`，否则历史通知残留。（详见 spec/global/domains/compact.md#issue_2026-05-11-auto-compact-no-resubmit）

## MCP 中间件

`McpMiddleware` 基于 `rmcp` crate。配置合并：全局 `~/.peri/settings.json` + 项目 `{cwd}/.mcp.json`（同名覆盖）。工具命名 `mcp__{server_name}__{tool_name}`。插件 MCP 使用 `{plugin_name}__{server_name}` 前缀命名空间。

**[TRAP]** `ClaudeSettings` 的 `extraKnownMarketplaces` 和 `enabledPlugins` 需同时支持对象和数组格式。**`enabledPlugins` 写入必须用对象格式** `{"id": true}`。

**Plugin Sources 旁路表**：`load_merged_config_full` 返回 `(McpConfigFile, HashMap<String, String>)`，key 格式 `"plugin:{name}:{server}"`，value `"name@marketplace"`。`load_installed_plugins` 路径需从 `claude_home` 参数推导。

## 插件系统

兼容 Claude Code 插件生态。配置：`~/.peri/settings.json`（全局）+ `~/.claude/plugins/cache/`（插件 manifest）。

**Hooks**（`rust-agent-middlewares/src/hooks/`）：4 种执行类型（Command/Prompt/Http/Agent），14 种事件。exit code 控制流程：0=Allow，1=Warn，2=Block。SSRF 防护阻止内网地址（`ssrf_guard.rs`），回环地址允许。

**Frontmatter 解析**：skill 和插件命令用 `gray_matter` crate（YAML engine），必须复用 `Matter::<YAML>::new()` 模式。

**Skills**：搜索顺序 `~/.claude/skills/` → `skillsDir` → `./.claude/skills/` → 插件 skills。`SkillsMiddleware.with_extra_dirs()` 是插件扩展点。

## SubAgents

`.claude/agents/{agent_id}/agent.md` 定义。`tools` 为空继承父工具（排除 Agent 防递归），有值仅保留允许列表，`disallowedTools` 额外排除。插件 agent 通过 `scan_agents_with_extra_dirs` 追加搜索路径。

**[TRAP]** Background agent 工具完全依赖 `register_tool` 传递，跨 async 边界需确保 Arc 引用生命周期。多语义叠加（fork+background）需明确优先级，跨轮次累积数据（frozen_vms）必须有清理机制。**[TRAP]** Normal/Fork 子 Agent 透传 event_handler 导致事件溢出，StateSnapshot/ContextWarning/LlmRetrying 缺少 in_subagent() 守卫——新增事件类型时必须同步检查所有事件处理路径的守卫。（详见 spec/global/domains/agent.md#issue_2026-05-12-background-agent-display-and-continuation-bugs，spec/global/domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent）

## LSP 中间件

`LspMiddleware` + `LspTool` + `perihelion-lsp` 客户端库。10 种操作（goToDefinition/findReferences/hover 等），`after_tool` 自动同步文件变更（`didChange` + `didSave`）。

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

i18n 模块位于 `rust-agent-tui/src/i18n/`，仅 TUI crate 使用，不向 `perihelion-widgets` 或 `rust-create-agent` 传递。`LcRegistry` 存储在 `ServiceRegistry.lc` 中，翻译资源通过 `include_str!` 编译时嵌入 `locales/{lang}/main.ftl`。

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
- **`Interrupted`/`Error` + `Done` 互斥 [TRAP]**：`Interrupted`/`Error` 先 `request_rebuild()` + 添加通知，设 `reconcile_already_done=true`，后续 `Done` 跳过 `request_rebuild()` 防止覆盖通知。
- **快捷键设计**：禁止 `Shift+字母`（编辑态等同大写输入）。全局用 `Ctrl+字母`，面板用方向键/Space/Enter/Esc。
- **面板系统**：`PanelManager` + `PanelComponent` trait（`panel_manager.rs`/`panel_component.rs`），新增面板只需定义变体 + 实现 trait。面板内禁止渲染提示行，由 `status_bar_hints()` 统一描述。
- **`Event::Paste`**：独立于 key event 链，必须单独拦截。
- **测试隔离**：禁止写入全局配置。用 `App::save_config(cfg, self.config_path_override.as_deref())`。
- **`std::sync::RwLockReadGuard` 不是 `Send`**，async 中不能跨 `.await` 持有，用 `parking_lot::RwLock`。（详见 spec/global/domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit）
- **`CommandRegistry::dispatch` 借用限制 [TRAP]**：`&self` + `&mut App` 冲突，当前用 `std::mem::take` + put-back 解决。
- **`ServiceRegistry` 与 `GlobalUiState`**：`App` 状态已拆分为 `ServiceRegistry`（跨会话共享服务：config/MCP/cron/provider）和 `GlobalUiState`（纯 UI 临时状态：高亮计时器/弹窗/鼠标检测）。面板 dispatch 宏封装了 `mem::take` 模式。
- **面板统一列表状态**：面板内列表组件使用统一的 `ListState` 管理（选中/滚动/过滤），支持鼠标交互（滚轮/点击/拖拽）。（详见 `rust-agent-tui/src/panels/`）
- **工具并发结果处理 [TRAP]**：多工具并发的结果处理循环中，P3/P4 错误路径提前返回会导致后续 tool_result 缺失。必须用 deferred_error 模式——先收集所有错误，循环结束后统一判断。所有 tool_result 必须始终写入 state。（详见 spec/global/domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result）
