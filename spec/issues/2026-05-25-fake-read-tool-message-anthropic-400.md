# SkillPreload / AtMention 注入的 fake Read 工具消息导致 Anthropic API 400 错误

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-25

## 问题描述

当用户使用 `@path` 提及文件或触发 skill preload 时，`AtMentionMiddleware` / `SkillPreloadMiddleware` 会向 agent state 注入 `Ai[ToolUse{Read}] → Tool[ToolResult]` 消息序列。但这些消息经过 Anthropic 适配器（`messages_to_anthropic`）转换后，`Tool` 消息被转为 `{ role: "user", content: [tool_result] }`，如果此 user 消息成为 API messages 数组的第一条（`messages.0`），Anthropic API 报 400：

```
unexpected `messages.0.content.0: tool_use_id` found in `tool_result` blocks:
  call_45fcd0a4daa245a1b1edb85df995903a.
Each `tool_result` block must have a corresponding `tool_use` block in the previous message.
```

## 症状详情

### 消息注入结构

```text
[Human "看看 @test.rs"]         → { role: "user", content: "..." }
[Ai]    [ToolUse{Read, call_x}] → { role: "assistant", content: [{type: "tool_use", ...}] }
[Tool]  ToolResult{call_x}      → { role: "user", content: [{type: "tool_result", tool_use_id: "call_x", ...}] }
```

### Anthropic 适配器的 Tool 消息处理

`messages_to_anthropic`（`anthropic/invoke.rs:159-196`）处理 `BaseMessage::Tool` 时：

1. 尝试将 `tool_result` block 插入到最后一条 `user` 消息的 content 数组开头
2. 如果最后一条不是 `user`，新建 `{ role: "user", content: [tool_result_block] }`

### 错误原因

当 `tool_result` 消息出现在 messages 数组最前面（`messages.0`），前面没有包含 `tool_use` 的 assistant 消息，Anthropic API 拒绝接受。

### Anthropic vs OpenAI 消息格式差异

| 方面 | Anthropic | OpenAI |
|------|-----------|--------|
| Tool 消息角色 | 内嵌在 `user` 消息的 content 数组 | 独立 `tool` role 消息 |
| 约束 | `tool_result` 必须引用前一条 assistant 中的 `tool_use` | `tool` 消息必须跟在 assistant `tool_calls` 后 |
| `messages[0]` | 不能是纯 `tool_result` 的 user 消息 | 可以是 `tool` 消息（虽然少见） |

### 实际影响

- **AtMentionMiddleware**：用户输入 `@path` 并提交时触发，**第一轮就报错**
- **SkillPreloadMiddleware**：当前 `preload_skills` 为空（见 issue 2026-05-25-skill-preload-no-tool-calls-in-history），暂未触发，但一旦激活会有同样问题

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 Anthropic 模型（如 Claude Sonnet）
  2. 在输入框输入 `@某个存在的文件`
  3. 按 Enter 提交
  4. Agent 报错：`LLM HTTP 错误 (400): API 错误 400 Bad Request: unexpected messages.0.content.0: tool_use_id`
- **环境**：Anthropic API

## 涉及文件

- `peri-middlewares/src/at_mention/mod.rs` — AtMentionMiddleware，注入 Ai[ToolUse] + Tool[ToolResult] 消息
- `peri-middlewares/src/subagent/skill_preload.rs` — SkillPreloadMiddleware，同样的消息注入模式
- `peri-agent/src/llm/anthropic/invoke.rs:159-196` — Anthropic 适配器的 Tool 消息转换逻辑
