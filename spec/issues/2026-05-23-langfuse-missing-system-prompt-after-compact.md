# Compact 后 Langfuse 遥测丢失系统提示词

**状态**：Fixed
**优先级**：高（影响 compact 后所有 LLM 调用的系统提示词，不仅是遥测）
**创建日期**：2026-05-23

## 问题描述

在 Langfuse 遥测中观察到，compact 后的 LLM 调用（Generation observation）��� `input.messages` 数组中**没有系统提示词（system role 消息）**。这导致在 Langfuse UI 中无法看到发给 LLM 的完整上下文，影响调试和成本分析。

**实际影响远超遥测**：经验证，`BaseModelReactLLM.system` 在 `build_agent()` 中从未设置（`builder.rs:141-147`），系统提示词**仅通过 `state.messages()` 中的 System 消息传递**。因此 compact 后的 LLM 调用本身也缺少系统提示词——模型在无系统指令的情况下继续工作。

## 症状详情

### Langfuse 观察数据

- **Trace ID**: `019e5261130a7c839935ca6468d090e6`
- **Observation ID**: `019e52636e117512b91f8149814306a0`（GENERATION 类型，name=`ChatAnthropic`）
- **模型**: `glm-5.1`
- **Session ID**: `019e5036-4a42-7102-a115-9b2f2c730f7a`

### 实际现象

该 Generation observation 的 `input.messages` 数组以 `user` 角色消息开头（内容为 compact 后的摘要："此会话从之前的对话延续..."），**不包含任何 `system` 角色消息**。正常情况下，`with_system_prompt()` 预置的系统提示词应作为 `messages[0]` 出现（role=`system`）。

### 时序背景

1. 该 trace 的用户输入是 compact 后的续接对话："会损失掉 reasoning_effort 这个吧, 这个是不对的"
2. `messages` 数组第一条是 compact 摘要 Human 消息，后面跟着 assistant/tool 交替消息
3. 在 compact 之前的同一 session 的其他 trace 中，系统提示词正常存在

## 复现条件

- **复现频率**：必现（compact 后的 LLM 调用均缺失系统提示词）
- **触发步骤**：
  1. 进行多轮对话直到触发 auto compact（上下文 >85%）
  2. compact 完成后，ReAct 循环继续调用 LLM
  3. 在 Langfuse 中查看 compact 后的 Generation observation
- **环境**：所有使用 Langfuse 遥测的环境

## 涉及文件

- `peri-acp/src/langfuse/tracer.rs`（L206-219 `on_llm_start`，L222-292 `on_llm_end`）—— Langfuse 遥测记录 LLM 调用的 input
- `peri-agent/src/agent/executor/llm_step.rs`（L22-27）—— 发出 `LlmCallStart` 事件时传递 `state.messages()`
- `peri-agent/src/agent/executor/mod.rs`（L240-241）—— `with_system_prompt` 通过 `prepend_message` 注入系统提示词
- `peri-middlewares/src/compact_middleware.rs`（L251-253）—— compact 替换全部 messages 后系统提示词丢失

## 根因分析

`execute()` 在 ReAct 循环前只执行一次 `prepend_message(BaseMessage::system(prompt))`（`mod.rs:241`）。`CompactMiddleware::do_full_compact()` 用 `*state.messages_mut() = new_messages` 整体替换消息，丢弃了头部的所有 System 消息（包括系统提示词、CLAUDE.md、agent 定义、skills 摘要）。

数据流：
1. `execute()` → `prepend_message(System(prompt))` — 注入一次
2. ReAct 循环 → `before_model()` → `do_full_compact()` → `*state.messages_mut() = [Human(summary), System(re_inject)]` — 全部替换
3. 循环继续 → `call_llm()` 使用 `state.messages()` — 无系统提示词

## 修复

在 `do_full_compact()` 替换 messages 前，提取头部 System 消息前缀（`take_while(|m| m.is_system())`），替换后重新前置到 `new_messages` 头部。保留原始 `MessageId`，确保 `cleanup_prepended` 仍能正确清理。

修改文件：`peri-middlewares/src/compact_middleware.rs:228-248`

测试：`compact_middleware_test.rs` 新增 3 个测试覆盖 System 前缀保留、无前缀退化、ID 保留。
