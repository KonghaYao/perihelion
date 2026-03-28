# Agent 领域

## 领域综述

Agent 领域是整个框架的核心，负责 ReAct 推理循环的执行、消息系统管理、工具抽象与执行、LLM 适配以及线程会话的持久化。

核心职责包括：
- ReAct 执行器：管理推理循环、工具分发、事件发射
- 消息类型系统：`BaseMessage`（Human/Ai/System/Tool，每条含 UUID v7 MessageId）、`ContentBlock` 完整变体、`MessageContent`
- LLM 适配层：OpenAI / Anthropic 双适配，`MessageAdapter` trait 支持双向格式转换，序列化时跳过 id 字段
- Middleware Chain：横切关注点（Skills、HITL、SubAgent、SkillPreload 等）通过标准 trait 解耦
- 线程持久化：SQLite WAL 模式，`parking_lot::Mutex` 串行写，`StateSnapshot` 事件驱动增量写入，message_id 为主键
- 声明式子 Agent：`.claude/agents/*.md` 定义 Explorer/WebResearcher 等专用 Agent，frontmatter 声明工具白名单和 skills 预加载
- System Prompt：ReActAgent.with_system_prompt() 固定在 run_before_agent 后 prepend，消除 PrependSystemMiddleware 顺序约束
- 工具接口：ask_user_question（对齐 Claude AskUserQuestion 规范），questions 数组 + header + options.description

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
| 持久化 | SQLite WAL，`parking_lot::Mutex<Connection>` 串行写，`append_messages` 事务，message_id 为主键 |
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
- 新建 rust-create-agent/src/interaction/mod.rs：UserInteractionBroker trait + InteractionContext（Approval/Questions）
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

---

## 相关 Feature
- → [relay-server.md#feature_20260326_F009_relay-message-id-propagation](./relay-server.md) — message_id 透传到 Web 前端
- → [langfuse.md#feature_20260325_F003_langfuse-observation-types](./langfuse.md#feature_20260325_F003_langfuse-observation-types) — Langfuse 观测依赖 AgentEvent LlmCallStart/End 钩子
