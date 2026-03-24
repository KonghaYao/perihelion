# Agent 领域

## 领域综述

Agent 领域是整个框架的核心，负责 ReAct 推理循环的执行、消息系统管理、工具抽象与执行、LLM 适配以及线程会话的持久化。

核心职责包括：
- ReAct 执行器：管理推理循环、工具分发、事件发射
- 消息类型系统：`BaseMessage`（Human/Ai/System/Tool）、`ContentBlock` 完整变体、`MessageContent`
- LLM 适配层：OpenAI / Anthropic 双适配，`MessageAdapter` trait 支持双向格式转换
- Middleware Chain：横切关注点（Skills、HITL、SubAgent 等）通过标准 trait 解耦
- 线程持久化：SQLite WAL 模式，`parking_lot::Mutex` 串行写，`StateSnapshot` 事件驱动增量写入

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
| 持久化 | SQLite WAL，`parking_lot::Mutex<Connection>` 串行写，`append_messages` 事务 |
| LLM 适配 | OpenAI（streaming SSE）+ Anthropic（Prompt Cache / Extended Thinking） |
| 消息格式 | `BaseMessage` ↔ `MessageAdapter` trait，`OpenAiAdapter` / `AnthropicAdapter` |
| Middleware | `Middleware<S>` trait（5 个钩子），`MiddlewareChain` 顺序执行 |
| 工具系统 | `BaseTool` trait，`ToolProvider` trait 动态提供，`register_tool` 优先级最高 |
| 错误处理 | LLM 层 `anyhow::Result`，工具层结构化错误信息（`is_error: true`） |
| 测试 | `MockLLM::tool_then_answer()` 脚本回放，无需真实 API |

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

---

## 相关 Feature
