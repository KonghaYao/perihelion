# Agent 领域

## 领域综述

Agent 领域是整个框架的核心，负责 ReAct 推理循环的执行、消息系统管理、工具抽象与执行、LLM 适配以及线程会话的持久化。

核心职责包括：

- ReAct 执行器：管理推理循环、工具分发、事件发射
- 消息类型系统：`BaseMessage`（Human/Ai/System/Tool，每条含 UUID v7 MessageId）、`ContentBlock` 完整变体、`MessageContent`
- LLM 适配层：OpenAI / Anthropic 双适配，`MessageAdapter` trait 支持双向格式转换，序列化时跳过 id 字段；LLM Factory 签名 `Fn(Option<&str>)` 支持子 Agent 独立模型
- Middleware Chain：横切关注点（Skills、HITL、SubAgent、SkillPreload、Cron 等）通过标准 trait 解耦
- 线程持久化：SQLite WAL 模式，sqlx SqlitePool 连接池（max=5）原生 async，`StateSnapshot` 事件驱动增量写入，message_id 为主键
- 声明式子 Agent：`.claude/agents/*.md` 定义 Explorer/WebResearcher 等专用 Agent，frontmatter 声明工具白名单、skills 预加载和独立模型；Fork 路径（fork: true）继承父 agent 完整上下文
- System Prompt：ReActAgent.with_system_prompt() 固定在 run_before_agent 后 prepend，消除 PrependSystemMiddleware 顺序约束
- 工具接口：ask_user_question（对齐 Claude AskUserQuestion 规范），questions 数组 + header + options.description
- 定时任务：CronMiddleware 提供 cron_register/cron_list/cron_remove 工具，croner 2 解析 cron 表达式
- 后台执行：Agent 工具 `run_in_background` 参数，BackgroundTaskRegistry 最多 3 并发，mpsc unbounded 通道通知，完成后 Human 消息注入，Done 后自动 continuation

## 核心流程

### ReAct 推理循环

```
AgentInput → add_message(Human)
  → chain.collect_tools(cwd)     ← ToolProvider 合并，手动注册优先级最高
  → chain.before_agent(state)    ← AgentsMd → Skills（prepend System）
  → loop(max_iterations=50):
      llm.generate_reasoning(messages, tools)
        stop_reason==ToolUse  → 工具调用分支
        stop_reason==EndTurn  → 最终回答
      state.add_message(Ai{tool_calls})
      for each tool_call:
        chain.before_tool()   ← HITL 在此拦截
        tool.invoke(input)    ← AskUser 在此挂起
        chain.after_tool()    ← Todo 解析结果
        emit(ToolStart/ToolEnd)
        state.add_message(Tool{result})
      emit(TextChunk)
  → chain.after_agent(state, output)
  → AgentOutput
```

### 消息持久化流程

```
StateSnapshot 事件触发
  → 过滤 System 消息（不持久化）
  → append_messages 事务写入 SQLite
  → WAL 模式保证 crash-safe
  → 下次 Agent 执行时 load_messages 恢复
```

### SubAgent 委派流程

```
launch_agent 工具调用
  → 查找 .claude/agents/{id}.md
  → 解析 frontmatter（system_prompt/tools/disallowedTools/maxTurns）
  → 过滤父工具集（无 tools → 全部继承；tools → 白名单；disallowed → 排除）
  → 创建子 ReActAgent（共享事件处理器）
  → 执行 → 返回工具调用摘要 + 最终回答
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 持久化 | SQLite WAL，sqlx SqlitePool(max=5) 原生 async，`append_messages` 事务，message_id 为主键 |
| 消息 ID | UUID v7（时间有序，`uuid = "1"` features: v7 + serde），MessageId 封装，构造器自动填充 |
| LLM 适配 | OpenAI（streaming SSE）+ Anthropic（Prompt Cache / Extended Thinking）；序列化时跳过 message id 字段 |
| 消息格式 | `BaseMessage` ↔ `MessageAdapter` trait，`OpenAiAdapter` / `AnthropicAdapter` |
| Middleware | `Middleware<S>` trait（5 个钩子），`MiddlewareChain` 顺序执行 |
| 工具系统 | `BaseTool` trait，`ToolProvider` trait 动态提供，`register_tool` 优先级最高 |
| 错误处理 | LLM 层 `anyhow::Result`，工具层结构化错误信息（`is_error: true`） |
| 测试 | `MockLLM::tool_then_answer()` 脚本回放，无需真实 API |
| 子 Agent 中间件 | AgentsMdMiddleware → SkillsMiddleware → SkillPreloadMiddleware → TodoMiddleware → PrependSystemMiddleware |
| skill 预加载 | SkillPreloadMiddleware：fake read_file 工具调用+ToolResult 消息对注入，frontmatter.skills 声明 |
| System Prompt | ReActAgent.with_system_prompt()：内置字段，execute() 在 run_before_agent 之后固定 prepend；PrependSystemMiddleware 保留用于子 agent 动态 system builder |
| ask_user_question | 工具名对齐 Claude；questions 数组（1-4 个）；header 短标签；options.description；始终允许自定义输入 |
| 事件携带 message_id | TextChunk/ToolStart/ToolEnd 均携带 message_id，Web 前端可 update-in-place |
| LLM Factory | `Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM>>`，支持 Option<&str> 参数传递子 Agent 独立模型标识 |
| 子 Agent 模型 | agent.md frontmatter model 字段（sonnet/opus/haiku/inherit），alias 解析在 TUI 层完成 |
| 定时任务 | CronMiddleware 提供 cron_register/cron_list/cron_remove 三个工具；croner 2 解析表达式；内存任务表上限 20 |
| 后台执行 | BackgroundTaskRegistry(max=3) + mpsc unbounded 通道；invoke_background() 不 await；结果注入为 Human 消息；Done 后保持通道存活 + 自动 continuation |
| 工具延迟加载 | 核心工具（12 个）直接加载，非核心工具通过 SearchExtraTools 按需发现、ExecuteExtraTool 代理执行；Prompt 缓存会话级 |
| Web 工具 | WebMiddleware 注入 WebFetch（HTML→Markdown）和 WebSearch（Tavily API），支持域名过滤和实时抓取 |
| trait 清理 | ReactLLM trait 移除废弃方法（generate_reasoning），统一为 generate()；废弃 trait 标记 #[deprecated] |

## Feature 附录

### 20260321_F001_subagents-execution

**摘要:** launch_agent 工具支持子 Agent 委派，防递归，工具过滤
**关键决策:**

- 工具过滤: tools 空→继承全部（除自身）；tools 有值→白名单；disallowedTools→黑名单
- 防递归: launch_agent 始终从子 agent 工具集中排除
- LLM 工厂: `Arc<dyn Fn() -> Box<dyn ReactLLM>>`，每次创建独立实例
- 事件透传: 子 agent 与父共享 `Arc<dyn AgentEventHandler>`
**归档:** [链接](../../archive/feature_20260321_F001_subagents-execution/)
**归档日期:** 2026-03-24

### 20260322_F001_agent-storage-refactor

**摘要:** SQLite WAL 持久化替代 JSONL，MessageAdapter 双向转换
**关键决策:**

- SQLite WAL 模式: journal_mode=WAL, synchronous=NORMAL，并发读写安全
- 串行写: `parking_lot::Mutex<Connection>` 持锁执行所有写操作
- 幂等追加: INSERT OR IGNORE，基于 seq 唯一约束，重复不报错
- MessageAdapter: OpenAI / Anthropic 双实现，BaseMessage ↔ Provider 原生 JSON
**归档:** [链接](../../archive/feature_20260322_F001_agent-storage-refactor/)
**归档日期:** 2026-03-24

### feature_20260325_F001_subagent-middleware-injection

**摘要:** 子 Agent 补全三个缺失中间件使上下文与父 Agent 一致
**关键决策:**

- 注入顺序：AgentsMdMiddleware → SkillsMiddleware → TodoMiddleware → PrependSystemMiddleware
- TodoMiddleware 的 todo_rx 立即丢弃，send 失败静默忽略（子 Agent 不通知 TUI）
- 有意省略：HitlMiddleware（子 Agent 自动执行）、SubAgentMiddleware（防递归）、AskUserTool
**归档:** [链接](../../archive/feature_20260325_F001_subagent-middleware-injection/)
**归档日期:** 2026-03-27

### feature_20260326_F001_specialized-agents

**摘要:** 预置 Explorer 和 WebResearcher 两个声明式专用 Agent
**关键决策:**

- 纯配置文件实现，无 Rust 代码改动
- explorer：只读工具（read_file/glob_files/search_files_rg/bash），disallowedTools 覆盖所有写操作
- web-researcher：bash + write_file + read_file，中间结果落盘 /tmp/research_*.md
- Agent 定义文件：.claude/agents/{id}.md，frontmatter 声明 tools/disallowedTools/maxTurns
**归档:** [链接](../../archive/feature_20260326_F001_specialized-agents/)
**归档日期:** 2026-03-27

### feature_20260326_F005_subagent-skill-preload

**摘要:** Agent 定义 frontmatter 声明 skills，子 Agent 启动时自动全文预加载
**关键决策:**

- AgentFrontmatter.skills: Vec<String>，默认空
- SkillPreloadMiddleware：before_agent 注入 fake read_file ToolUse + ToolResult 消息对
- fake ID 格式：skill_preload_{index}，不依赖 UUID
- 找不到的 skill 静默跳过；不经过 HitlMiddleware（预注入非真实调用）
**归档:** [链接](../../archive/feature_20260326_F005_subagent-skill-preload/)
**归档日期:** 2026-03-27

### feature_20260326_F006_message-uuid-v7

**摘要:** BaseMessage 四变体增加 UUID v7 全局唯一 ID
**关键决策:**

- MessageId(uuid::Uuid)，Default::default() 自动生成新 ID
- 所有构造器（human/ai/system/tool_result 等）自动填充 id
- Provider 适配层序列化时跳过 id 字段（LLM 不需要）
- SQLite Schema 重建：message_id TEXT PRIMARY KEY，移除 seq 列
**归档:** [链接](../../archive/feature_20260326_F006_message-uuid-v7/)
**归档日期:** 2026-03-27

### feature_20260328_F001_ask-user-question-align

**摘要:** ask_user 工具全面对齐 Claude AskUserQuestion 接口规范
**关键决策:**

- 工具名: ask_user → ask_user_question
- 顶层结构改为 questions 数组（1-4 个）；新增 header 字段（≤12字短标签）
- QuestionOption 新增 description 字段；移除 allow_custom_input/placeholder，始终允许自定义输入
- TUI 弹窗 Tab 使用 header；选项下方展示 description；前端 AskUserDialog.js 同步更新
**归档:** [链接](../../archive/feature_20260328_F001_ask-user-question-align/)
**归档日期:** 2026-03-28

### feature_20260327_M3_system-prompt

**摘要:** with_system_prompt() 方法消除 PrependSystemMiddleware 的注册顺序约束
**关键决策:**

- ReActAgent 新增 system_prompt: Option<String> 字段和 with_system_prompt() builder
- execute() 在 run_before_agent() 之后固定 prepend，不受中间件注册顺序影响
- 主 Agent 调用方改用 with_system_prompt()；PrependSystemMiddleware 保留（子 agent 动态场景仍可用）
**归档:** [链接](../../archive/feature_20260327_M3_system-prompt/)
**归档日期:** 2026-03-28

### feature_20260327_H3_interaction-unify

**摘要:** 提取 UserInteractionBroker trait 统一 HITL 和 AskUser 交互机制
**关键决策:**

- 新建 peri-agent/src/interaction/mod.rs：UserInteractionBroker trait + InteractionContext（Approval/Questions）
- HITL 和 AskUser 中间件均通过 broker.request() 等待响应，单 channel 替代两套
- TUI TuiInteractionBroker 实现；relay 协议从 4 条消息合并为 2 条（InteractionRequest/InteractionResponse）
- 两阶段迁移：先新增 broker，再删旧实现（此 feature 归档时尚未完全完成）
**归档:** [链接](../../archive/feature_20260327_H3_interaction-unify/)
**归档日期:** 2026-03-28

### feature_20260326_F009_relay-message-id-propagation

**摘要:** TextChunk/ToolStart/ToolEnd 事件携带 message_id 支持 Web 前端 update-in-place
**关键决策:**

- ExecutorEvent::TextChunk 改为结构体变体 { message_id, chunk }
- ExecutorEvent::ToolStart/ToolEnd 新增 message_id 字段
- TUI agent.rs 用 `..` 解构忽略 message_id，TUI AgentEvent 枚举不变
- Relay Server 无需修改（JSON 自动透传新字段）
**归档:** [链接](../../archive/feature_20260326_F009_relay-message-id-propagation/)
**归档日期:** 2026-03-27

### feature_20260330_F003_cron-loop-command

**摘要:** /loop /cron 定时任务系统，cron 表达式注册管理
**关键决策:**

- CronMiddleware 提供 cron_register/cron_list/cron_remove 三个工具供 AI 使用
- croner 2 crate 解析 cron 表达式并计算下次触发时间
- 内存任务表上限 20 条，TUI 重启后清空
- TUI /loop 命令注册定时任务，/cron 面板管理（导航/删除/切换启用）
- 定时触发时将 prompt 作为 AgentInput 提交到 agent task
**归档:** [链接](../../archive/feature_20260330_F003_cron-loop-command/)
**归档日期:** 2026-04-27

### feature_20260329_F005_legacy-cleanup

**摘要:** Agent trait 层级清理与废弃 API 移除
**关键决策:**

- ReactLLM trait 移除废弃方法（generate_reasoning），统一为 generate() 接口
- 废弃 trait 和方法标记 #[deprecated]，保留编译兼容
- 清理未使用的泛型参数和冗余类型别名
- 同步更新所有 impl 块和测试代码
**归档:** [链接](../../archive/feature_20260329_F005_legacy-cleanup/)
**归档日期:** 2026-04-27

### feature_20260329_F002_subagent-model-switch

**摘要:** 子 Agent 支持独立模型配置，LLM Factory 签名升级
**关键决策:**

- LLM Factory 签名升级为 `Fn(Option<&str>)`，参数传递子 Agent 模型标识
- agent.md frontmatter 新增 model 字段（sonnet/opus/haiku/inherit）
- inherit（默认）继承父 Agent 模型，其他值使用指定别名
- alias 解析在 TUI 层完成（ModelAliasMap），不侵入 core 层
- SkillFrontmatter 同步增加 model 文档字段
**归档:** [链接](../../archive/feature_20260329_F002_subagent-model-switch/)
**归档日期:** 2026-04-27

### feature_20260503_F003_background-agent

**摘要:** Agent 工具支持后台执行，主 agent 不阻塞，完成后通知注入
**关键决策:**

- run_in_background 参数触发 invoke_background()，不 await 子 agent
- BackgroundTaskRegistry 管理最多 3 并发后台任务
- mpsc::unbounded_channel 通知，ReAct 循环末尾 try_recv 消费
- 后台结果注入为 Human 消息（非 ToolResult），原始调用早已返回
- 主 agent Done 后保持通道存活，最后一个后台任务完成时自动 continuation
- BackgroundTaskResult 定义在核心层（保持依赖方向正确）
- 显示样式: ToolBlock 格式，bg:{agent_name} 工具名，超长截断+折叠
**归档:** [链接](../../archive/feature_20260503_F003_background-agent/)
**归档日期:** 2026-05-04

### feature_20260503_F002_multi-agent-design

**摘要:** Fork 路径继承父 agent 上下文 + Agent prompt 指导扩写
**关键决策:**

- fork: true 触发 Fork 路径，子 agent 继承父 agent 完整消息历史 + system prompt + 工具集
- Fork 不读 agent 定义文件（subagent_type 参数被忽略）
- 继承实现：parent_messages.read().clone() 传递给子 agent state
- 结构化输出：子 agent 返回 Scope/Result/Key files/Files changed 四段格式
- Agent 工具 prompt 指导从 21 行扩写为完整指导（使用时机、prompt 写作、示例）
**归档:** [链接](../../archive/feature_20260503_F002_multi-agent-design/)
**归档日期:** 2026-05-04

### feature_20260430_F001_align-claude-code-tools

**摘要:** 10 个工具名称和参数结构完全对齐 Claude Code
**关键决策:**

- 工具名完全对齐：read_file→Read、write_file→Write、edit_file→Edit、glob_files→Glob、search_files_rg→Grep、launch_agent→Agent
- Grep 重大重构：args 数组→结构化字段（pattern/path/glob/type/output_mode/-i/-C/-n 等）
- Agent 对齐：prompt/description 模式，subagent_type/fork/run_in_background 参数
- folder_operations 保留为 Peri 扩展工具
- Read 新增 pages 参数（PDF 页范围）
**归档:** [链接](../../archive/feature_20260430_F001_align-claude-code-tools/)
**归档日期:** 2026-05-04

### feature_20260509_F001_tool-search

**摘要:** 工具延迟加载机制，核心工具直接加载，非核心工具按需发现和代理执行
**关键决策:**

- 核心工具（12 个）始终加载：Read/Write/Edit/Glob/Grep/folder_operations/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite
- 非核心工具通过 SearchExtraTools 按需发现，ExecuteExtraTool 代理执行
- ToolProvider trait 动态提供工具集合，支持运行时扩展
- Prompt 缓存优化：deferred tools 提示词会话级缓存，保证 Anthropic cache 前缀稳定
**归档:** [链接](../../archive/feature_20260509_F001_tool-search/)
**归档日期:** 2026-05-13

### feature_20260505_F001_web-tools

**摘要:** WebFetch 和 WebSearch 工具集成，支持网络请求和搜索功能
**关键决策:**

- WebMiddleware 注入 WebFetch（HTML→Markdown）和 WebSearch（Tavily API）两个工具
- WebFetch 使用 web_reader MCP 工具抓取网页并转换为 Markdown
- WebSearch 使用 Tavily API 进行网络搜索，返回结构化结果
- 支持搜索结果过滤（允许/阻止域名）和实时抓取模式
**归档:** [链接](../../archive/feature_20260505_F001_web-tools/)
**归档日期:** 2026-05-13

---

## Issue 经验附录

### issue_2026-05-11-streaming-text-invisible-with-tools
**摘要:** 流式过程中 AI 文本不可见（工具调用场景）
**状态:** Fixed（待用户验证）
**归档日期:** 2026-05-13
**关键词:** AiReasoning, TextChunk, 事件类型语义, 流式渲染
**问题本质:** 工具前文本通过 AiReasoning 事件发射而非 TextChunk，TUI pipeline 将 AiReasoning 映射为 "Thought for N chars" 推理提示，不显示实际文本
**通用模式:** 核心框架的事件类型决定了 TUI pipeline 的处理路径。新增事件或修改事件语义时，必须同步检查 TUI 侧的事件映射层（agent.rs 的事件映射表）
**技术决策:** 工具前文本改用 TextChunk 发射，与最终回答走同一路径
**涉及文件:** peri-agent/src/agent/executor/tool_dispatch.rs, peri-agent/src/agent/executor/final_answer.rs, peri-tui/src/app/agent.rs, peri-tui/src/app/message_pipeline.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-background-agent-display-and-continuation-bugs
**摘要:** Background Agent 三个 Bug：显示消失、subagent_type 限制、continuation 不触发
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** frozen_subagent_vms, continuation 竞态, fork+background, pending_bg_continuation
**问题本质:** 三个独立根因：(1) fork 检测优先于 background 导致走错路径且 background_task_count 泄漏；(2) frozen_subagent_vms 跨轮次膨胀导致错位替换；(3) pending_bg_continuation.take() 在 loading=true 时丢失
**通用模式:** 多语义叠加（fork+background）时需要明确的优先级和独立处理路径。跨轮次累积的数据结构（frozen_vms）必须有清理/去重机制。异步 take() + 条件检查应先检查条件再 take()，避免消费后丢弃
**架构影响:** frozen_subagent_vms 的 drain_subagent_stack() 方法规范了异常残留清理；pending_bg_continuation 的修复模式（先检查条件再 take）可作为异步状态消费的通用范式
**涉及文件:** peri-middlewares/src/subagent/tool.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/message_pipeline.rs
**CLAUDE.md 链接:** false

### issue_2026-05-11-background-agent-missing-tools
**摘要:** Background Agent 工具继承缺失——子 agent 仅能使用 TodoWrite
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** SubAgent, 工具继承, register_tool, Arc 共享
**问题本质:** Background agent 的工具完全依赖 parent_tools 通过 register_tool 传递，tokio::spawn 闭包的 Arc 引用在 move 后可能失效
**通用模式:** Background agent 的 middleware 配置与 Normal 路径一致但工具来源不同（register_tool vs middleware 内部构建）。跨 async 边界的工具传递需要确保 Arc 引用的生命周期
**涉及文件:** peri-tui/src/app/agent.rs, peri-middlewares/src/subagent/tool.rs, peri-agent/src/agent/executor/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-glm-reasoning-field-not-parsed
**摘要:** GLM 模型 reasoning 字段未被解析，thinking 内容跨轮次丢失
**状态:** Fixed
**归档日期:** 2026-05-13
**关键词:** reasoning, reasoning_content, GLM, OpenAI 兼容
**问题本质:** GLM 系列模型使用 `reasoning` 顶层字段而非 `reasoning_content`，代码只检查了后者。附带发现 invariant check 对并行 tool_calls 的合法消息序列产生误报
**通用模式:** OpenAI 兼容 API 的字段名存在 provider 差异。解析时应同时检查多个可能的字段名（or_else 链式），序列化时应同时回传多个字段以保持兼容。invariant check 应基于消息块而非逐条检查
**技术决策:** 解析侧 reasoning_content.or(reasoning) 双字段尝试；序列化侧同时设置两个字段；invariant check 改为连续块检查
**涉及文件:** peri-agent/src/llm/openai.rs, peri-agent/src/messages/adapters/openai.rs
**CLAUDE.md 链接:** true

### issue_2026-05-14-deepseek-anthropic-thinking-block-dropped
**摘要:** SkillPreloadMiddleware 注入的伪 assistant 消息不含 thinking block，DeepSeek API 400
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** thinking block, redacted_thinking, SkillPreload, DeepSeek
**问题本质:** DeepSeek 要求 thinking 模式下所有 assistant 消息都必须回传 thinking block（含 signature），但 SkillPreloadMiddleware 构造的伪造消息天然不含 thinking。本地构造的 assistant 消息与 provider 的 thinking 回传约束不兼容。
**通用模式:** 手动构造的 assistant 消息必须考虑 provider 的 thinking 回传约束。在序列化层自动检测并注入 redacted_thinking 比在每个构造点修补更健壮——集中处理比分散修补更可靠。
**架构影响:** 使用 Anthropic 的 redacted_thinking 类型（opaque data 字段）作为"无 thinking 原文但有占位"的通用解决方案
**技术决策:** messages_to_anthropic() 中检测不含 thinking/redacted_thinking 的 assistant 消息自动注入 redacted_thinking
**涉及文件:** peri-middlewares/src/subagent/skill_preload.rs, peri-agent/src/llm/anthropic.rs
**CLAUDE.md 链接:** true

### issue_2026-05-14-orphaned-tool-use-without-tool-result
**摘要:** 并发工具执行中部分路径提前返回导致 tool_result 缺失，Anthropic API 400
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** tool_result闭合, 并发工具, deferred_error, 孤儿tool_use
**问题本质:** 并发工具执行的结果处理循环中，P3（run_on_error 错误传播）和 P4（run_after_tool 返回 Err）路径通过 `?` 提前跳出循环，导致后续工具的 tool_result 未写入 state。Anthropic API 要求所有 tool_use 必须在下一条消息中有对应 tool_result（闭合跟随规则），缺失即 400。
**通用模式:** 多工具并发的结果处理必须"尽最大努力收集所有结果，延迟错误传播"（collect-all-before-error 模式）。在循环中收集 deferred_error，循环结束后统一判断是否报错。所有 tool_result 必须始终写入（包括 error tool_result）。
**架构影响:** deferred_error 模式——将所有错误收集到 Option<AgentError>，循环结束后统一返回。这适用于所有需要"处理所有元素后再决定成败"的并发场景
**技术决策:** run_on_error/run_after_tool 失败改为 let _ = 吞掉并收集到 deferred_error；state.add_message(tool_msg) 始终执行
**涉及文件:** peri-agent/src/agent/executor/tool_dispatch.rs
**CLAUDE.md 链接:** true

### issue_2026-05-14-grep-tool-capability-gap
**摘要:** Grep 工具声明参数未实现 + 标准 grep 能力缺失
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** Grep工具, 参数声明, 接口契约, 工具标准能力
**问题本质:** 工具暴露给 LLM 的 JSON schema 参数（multiline/-n/whole_word）声称可用但实际未实现，导致 LLM 写出正确的跨行正则却得到错误结果。本质是接口契约不匹配——声明与实现不同步。
**通用模式:** 所有暴露给 LLM 的工具参数必须经过实现验证。工具 schema 是对 LLM 的接口契约，任何声称支持的参数必须有对应的代码路径。新增参数时必须同步实现。
**技术决策:** output_mode 从必填改为默认 "content"，减少 LLM 调用负担
**涉及文件:** peri-middlewares/src/tools/filesystem/grep.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-thinking-reasoning-dataflow-issues
**摘要:** Thinking/Reasoning数据流：占位thinking缺signature + AiReasoning死代码
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** thinking block, reasoning_content, AiReasoning, 死代码清理
**问题本质:** Anthropic extended thinking的占位thinking block缺少signature字段可能导致API拒绝；AiReasoning事件链路是为流式API预留的未使用代码，非流式下reasoning完全依赖StateSnapshot路径
**通用模式:** LLM适配器中多模型兼容字段需按provider条件注入，不可凭空伪造；预留接口若长期未使用应清理或显式标记
**架构影响:** 非流式API下reasoning通过source_message保留而非流式事件，流式路径的预留在当前架构下不必要
**技术决策:** 删除占位thinking注入逻辑；保留AiReasoning事件定义但明确标记为预留
**涉及文件:** peri-agent/src/llm/anthropic.rs, peri-agent/src/llm/openai.rs, peri-agent/src/agent/executor/tool_dispatch.rs, peri-agent/src/agent/executor/final_answer.rs, peri-agent/src/agent/events.rs, peri-tui/src/app/message_pipeline.rs, peri-tui/src/ui/message_view.rs
**CLAUDE.md 链接:** true

### issue_2026-05-14-deepseek-multi-turn-tool-result-duplication
**摘要:** DeepSeek多轮对话中agent_state_messages消息重复导致API 400错误
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** prepend_message, StateSnapshot, last_message_count, 消息重复
**问题本质:** prepend_message的insert(0)使last_message_count索引失效，StateSnapshot捕获范围扩大，旧消息被重复extend到agent_state_messages
**通用模式:** 任何插入操作后必须补偿依赖索引的偏移量；基于计数的索引不应在插入/删除操作后继续使用
**架构影响:** StateSnapshot的增量扩展机制对prepend敏感，长期应考虑基于消息ID的标记替代数组索引
**技术决策:** 在prepend_message后补偿last_message_count += 1
**涉及文件:** peri-agent/src/agent/executor/mod.rs, peri-agent/src/agent/state.rs, peri-tui/src/app/agent_ops.rs
**CLAUDE.md 链接:** true

### issue_2026-05-15-glm-anthropic-tool-result-id-attribute-error
**摘要:** GLM Anthropic兼容端口tool_result block缺少id属性导致500错误
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** tool_result id, GLM兼容性, Anthropic适配器, 第三方API
**问题本质:** 第三方Anthropic兼容端口对API规范实现不完整，GLM网关要求tool_result有id字段但Anthropic规范无此要求
**通用模式:** 第三方provider的Anthropic兼容端口可能存在属性缺失或额外要求，需客户端兼容策略
**架构影响:** Anthropic适配器需为不同provider准备兼容字段，不能假设所有provider严格遵循规范
**技术决策:** 为tool_result添加可选id字段，BaseMessage::Tool路径用MessageId，ContentBlock路径用UUID v7
**涉及文件:** peri-agent/src/messages/content.rs, peri-agent/src/llm/anthropic/invoke.rs, peri-agent/src/messages/adapters/anthropic.rs
**CLAUDE.md 链接:** false

### issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error
**摘要:** stop_reason与内容不一致导致孤儿tool_use触发Anthropic API 400
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** stop_reason, tool_use, 孤儿tool_use, 延迟写入
**问题本质:** 第三方provider的stop_reason与实际内容不一致（end_turn但含tool_use），仅依赖stop_reason路由导致工具调用误入最终回答路径
**通用模式:** 不能仅信任API元数据字段，必须同时检查实际内容；defense-in-depth通过内容自检兜底
**架构影响:** 延迟写入重构消除了tool_dispatch中的flush路径脆弱性（4次因此bug修复）
**技术决策:** generate_reasoning中增加has_tool_calls()内容检查作为stop_reason的兜底
**涉及文件:** peri-agent/src/llm/anthropic/invoke.rs, peri-agent/src/llm/react_adapter.rs, peri-agent/src/agent/executor/tool_dispatch.rs
**CLAUDE.md 链接:** true

### issue_2026-05-15-tool-execution-error-stops-agent
**摘要:** 工具调用参数错误导致Agent停止而非自动重试
**状态:** Fixed — 部分修复
**归档日期:** 2026-05-16
**关键词:** deferred_error, ToolExecutionFailed, after_tool错误, 工具错误处理
**问题本质:** 工具执行错误和中间件错误被统一设为deferred_error导致Agent停止，应区分错误来源：工具执行错误应反馈给LLM而非终止循环
**通用模式:** 工具执行层面的错误（参数缺失、执行失败）是正常流程的一部分，应创建error ToolResult让LLM自行修正；只有基础设施错误才应终止
**架构影响:** deferred_error机制需要细化分类，区分tool-level error（不终止）和middleware error（可能终止）
**技术决策:** ToolNotFound和ToolExecutionFailed不再设deferred_error；after_tool中间件错误仍设deferred_error（残留问题）
**涉及文件:** peri-agent/src/agent/executor/tool_dispatch.rs
**CLAUDE.md 链接:** true

### issue_2026-05-15-write-tool-missing-filepath-max-tokens
**摘要:** Write工具超长内容触发max_tokens截断导致file_path缺失
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** max_tokens, Write工具, JSON截断, file_path缺失
**问题本质:** max_tokens不足导致流式JSON参数截断，关键字段file_path可能因为字段顺序靠后而缺失
**通用模式:** 流式JSON生成中max_tokens截断导致字段缺失是不可恢复的错误；工具定义中关键字段需优先输出
**架构影响:** 工具Schema中字段顺序影响截断时的完整性；超长内容应考虑分块策略
**涉及文件:** peri-middlewares/src/tools/filesystem/write.rs, peri-agent/src/llm/anthropic/invoke.rs, peri-agent/src/llm/openai/invoke.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background

**摘要:** 并发 SubAgent 工具调用路由错误 + 死锁修复
**状态:** Fixed
**归档日期:** 2026-05-17
**关键词:** source_agent_id routing, SubAgent 并发, streaming cancellation, agent_id 匹配, 通道容量
**问题本质:** 并发 SubAgent 场景下的四个系统性缺陷：(1) `subagent_stack.last_mut()` 位置路由将所有内部事件路由到最后一个 SubAgent；(2) LLM 流式期间不检查取消令牌导致 Ctrl+C 死锁；(3) `mpsc::channel(256)` 容量不足以承载 SubAgent 500+ 事件导致静默丢弃；(4) 同名 SubAgent 的 `find(|s| s.agent_id == target)` 永远命中第一个导致 `is_running` 不清零
**通用模式:** 事件路由必须使用唯一标识（agent_id）而非位置索引；流式循环必须通过 `tokio::select!` 竞争取消令牌和 stream.next()；事件通道容量应基于 SubAgent 速率而非主 Agent；同名实体匹配需加状态条件（如 `is_running`）
**架构影响:** `source_agent_id` 字段为所有事件类型添加了精确路由能力，从此 SubAgent 路由不再依赖位置堆栈；`deferred_error` 模式在 tool_dispatch 中成熟
**技术决策:** Agent 工具改为顺序执行消除并发争用（非 Agent 工具保持并发）；通道容量 256→4096；`SourceAgentIdHandler` 包装器注入子 Agent 事件标记
**涉及文件:** peri-agent/src/agent/events.rs, peri-agent/src/agent/executor/tool_dispatch.rs, peri-agent/src/agent/executor/llm_step.rs, peri-agent/src/llm/types.rs, peri-agent/src/llm/anthropic/stream.rs, peri-agent/src/llm/openai/stream.rs, peri-middlewares/src/subagent/tool.rs, peri-tui/src/app/agent.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent_submit.rs, peri-tui/src/app/message_pipeline.rs
**CLAUDE.md 链接:** true

### issue_2026-05-14-llm-adapter-modularization

**摘要:** LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行
**状态:** Fixed
**归档日期:** 2026-05-17
**关键词:** LLM 适配器, 模块化, 大文件拆分, anthropic, openai
**问题本质:** anthropic.rs（1983 行）和 openai.rs（1065 行）承载完整适配器实现：构造器、序列化、缓存策略、API invoke、流式处理、消息转换——职责过重，修改任一环节需阅读整个文件
**通用模式:** 按职责维度拆分大文件（构造器 + 缓存 + invoke + 流式），保留原文件路径 re-export 向后兼容
**技术决策:** 统一子模块结构：anthropic/{mod, cache, invoke, stream}、openai/{mod, invoke, stream}，上游直接 import 新路径
**涉及文件:** peri-agent/src/llm/anthropic.rs, peri-agent/src/llm/openai.rs, peri-agent/src/llm/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-13-background-task-completion-race-condition

**摘要:** Background task 完成后未触发 agent continuation（竞态条件）
**状态:** Fixed
**归档日期:** 2026-05-17
**关键词:** background task, 竞态条件, continuation, agent_done_pending_bg, 时序耦合
**问题本质:** BackgroundTaskCompleted 和 Done 通过同一 channel 传递，存在竞态：如果后台任务在 Done 之前完成，BackgroundTaskCompleted 先被消费，此时 agent_done_pending_bg 尚未设置，导致 continuation 永不触发
**通用模式:** 依赖时序耦合的双事件模式（先 A 后 B）必须处理乱序到达。解决方案：在"先到"事件中暂存结果，在"后到"事件中检查暂存区——用空间（pre_done_bg_completions 缓冲）换时间鲁棒性
**技术决策:** 新增 pre_done_bg_completions 字段缓存 Done 前完成的后台任务通知；Done/Error 处理时检查暂存区并设置 pending_bg_continuation
**涉及文件:** peri-tui/src/app/agent_comm.rs, peri-tui/src/app/agent_events_bg.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent_submit.rs, peri-tui/src/app/agent_compact.rs, peri-tui/src/ui/headless_test.rs
**CLAUDE.md 链接:** false

---

## 相关 Feature

- → [relay-server.md#feature_20260326_F009_relay-message-id-propagation](./relay-server.md) — message_id 透传到 Web 前端
- → [langfuse.md#feature_20260325_F003_langfuse-observation-types](./langfuse.md#feature_20260325_F003_langfuse-observation-types) — Langfuse 观测依赖 AgentEvent LlmCallStart/End 钩子
- → [tui.md#feature_20260328_F003_test-coverage-improvement](./tui.md#feature_20260328_F003_test-coverage-improvement) — ask_user_tool 10 个单元测试（MockBroker 参数解析和返回格式）
- → [hitl-permissions.md](./hitl-permissions.md) — 5 级权限模式 HITL middleware 集成
- → [llm-retry.md](./llm-retry.md) — RetryableLLM 装饰器包装 ReactLLM
- → [system-prompt.md](./system-prompt.md) — 系统提示词段落化 PromptFeatures 条件注入
- → [file-search.md](./file-search.md) — grep crate 进程内文件搜索
- → [token-tracking.md](./token-tracking.md) — TokenTracker Token 累积追踪
- → [compact.md](./compact.md) — Micro/Full Compact 核心层消息操作
- → [message-pipeline.md](./message-pipeline.md) — MessagePipeline 统一管线
- → [code-architecture.md](./code-architecture.md) — Relay Server 移除
