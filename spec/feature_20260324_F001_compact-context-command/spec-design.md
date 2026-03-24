# Feature: 20260324_F001 - compact-context-command

## 需求背景

随着多轮对话进行，`agent_state_messages`（LLM 上下文历史）会持续增长，导致以下问题：
- Token 消耗快速增加，API 成本上升
- 接近模型上下文窗口上限时性能下降甚至截断
- 长对话中 LLM 对早期重要信息的注意力下降

Claude Code 提供了 `/compact` 指令解决此问题。本 feature 为 `rust-agent-tui` 实现同等功能：通过调用 LLM 将历史对话智能压缩为结构化摘要，在不损失关键上下文的前提下大幅缩减 token 占用。

## 目标

- 实现 `/compact [instructions]` TUI 指令，触发 LLM 自动压缩当前对话历史
- 压缩后 `agent_state_messages` 仅保留一条结构化摘要，下一轮对话可无缝续接
- TUI 显示层保留最近 10 条消息，头部插入"已压缩"系统提示
- 支持用户通过可选参数指定摘要侧重点

## 方案设计

### 命令接口

新增 `rust-agent-tui/src/command/compact.rs`，实现 `CompactCommand`：

```rust
fn name(&self) -> &str { "compact" }
fn description(&self) -> &str { "压缩对话上下文（调用 LLM 生成摘要）" }
fn execute(&self, app: &mut App, args: &str) {
    app.start_compact(args.to_string());
}
```

注册到 `command/mod.rs` 的 `default_registry()`。支持前缀匹配：`/co`、`/com`、`/compact` 均可触发。

### 数据流设计

![/compact 数据流](./images/01-flow.png)

**执行路径：**

1. 用户输入 `/compact [instructions]`
2. `CompactCommand::execute` → `App::start_compact(instructions)`
3. `start_compact`：
   - 若 `agent_state_messages` 为空，显示"无可压缩的上下文"提示并返回
   - 克隆当前历史消息
   - 设置 `loading=true`（TUI 进入"压缩中…"状态）
   - `tokio::spawn` 启动独立压缩任务（非 ReAct 循环）
4. `compact_task`：
   - 构造压缩 prompt（见下节）
   - 直接调用 `BaseModel::invoke(LlmRequest{...})` 一次
   - 成功：`tx.send(AgentEvent::CompactDone(summary))`
   - 失败：`tx.send(AgentEvent::CompactError(err_msg))`
5. `App::handle_agent_event`：
   - `CompactDone(summary)` → 替换 agent_state_messages、更新 view_messages、set_loading(false)
   - `CompactError(msg)` → 显示错误提示、set_loading(false)、不修改历史

**新增 AgentEvent 变体（`app/mod.rs`）：**

```rust
CompactDone(String),   // 压缩成功，携带摘要文本
CompactError(String),  // 压缩失败，携带错误信息
```

### LLM 压缩 Prompt 设计

**系统 Prompt（固定）：**

```
你是一个对话上下文压缩工具。将以下对话历史压缩为一份结构化摘要，要求：
1. 保留用户的核心目标和意图
2. 记录已完成的关键操作（文件读写、命令执行结果等）
3. 记录发现的重要信息（文件路径、错误信息、代码结构等）
4. 保留对话中的重要决策和约束
5. 格式：Markdown，分"## 目标"、"## 已完成操作"、"## 关键发现"三个小节
6. 语言：中文
```

**用户消息（动态）：**

```
以下是需要压缩的对话历史：
<conversation>
{每条消息格式化为 "[角色] 内容"}
</conversation>

{若有 instructions: "压缩时请特别注意：{instructions}"}
```

**压缩结果存储：** `BaseMessage::system(summary)`，作为新 `agent_state_messages` 的唯一条目。System 消息在 Anthropic adapter 中被提取到顶层 `system` 字段，OpenAI adapter 中作为 System 角色消息 prepend，均能被 LLM 正确感知。

### TUI 显示更新

压缩成功后，`handle_agent_event(CompactDone)` 执行：

```rust
// 1. 替换 LLM 历史
self.agent_state_messages = vec![BaseMessage::system(summary.clone())];

// 2. 保留最近 10 条 view_messages
let keep_count = 10;
if self.view_messages.len() > keep_count {
    let tail = self.view_messages.split_off(self.view_messages.len() - keep_count);
    self.view_messages = tail;
}

// 3. 在头部插入压缩提示
self.view_messages.insert(0, MessageViewModel::system(
    format!("📦 上下文已压缩（保留最近 {} 条显示消息，LLM 历史已替换为摘要）", keep_count)
));
```

## 实现要点

- `compact_task` 异步函数复用 `LlmProvider::into_model()` 构造 `BaseModel`，直接调用 `invoke`，避免引入 ReAct 循环
- `agent_rx` 在 compact 期间仍使用同一 channel，`CompactDone`/`CompactError` 由 `poll_agent` 统一消费
- compact 期间若用户输入消息，走 `pending_messages` 缓冲（与 Agent 运行期间行为一致）
- 现有 `persisted_count` 机制：compact 后设为 0（view_messages 已重建），下次 agent 运行完成后会重新持久化
- 压缩摘要不持久化到 SQLite thread store（因为摘要已覆盖原始历史，后续正常消息会增量写入）
- 工具调用消息（`BaseMessage::Tool` 和含 `tool_calls` 的 `Ai`）格式化时做配对处理，避免发送不完整的 tool_use/tool_result 对给 LLM

## 约束一致性

（`spec/global/` 不存在，省略此节）

## 验收标准

- [ ] `/compact` 和 `/co` 均能触发命令（前缀匹配）
- [ ] 执行期间 TUI 显示 loading 状态（"压缩中…"），输入框禁用
- [ ] 压缩后 `agent_state_messages` 仅含一条 System 摘要消息
- [ ] view_messages 保留最近 10 条，头部新增"📦 上下文已压缩"系统提示
- [ ] LLM 调用失败时显示错误消息，恢复 loading=false，不修改 agent_state_messages
- [ ] 传入 instructions 参数时，摘要内容侧重该指令要求
- [ ] 历史消息为空时，显示"无可压缩的上下文"并不进入 loading 状态
- [ ] 压缩后继续发送消息，LLM 能基于摘要正常续接对话
