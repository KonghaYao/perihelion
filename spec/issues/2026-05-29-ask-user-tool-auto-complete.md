# AskUserQuestion 弹窗出现后工具调用自行结束，用户操作无效

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-29
**类型**：Bug

## 问题描述

Agent 调用 AskUserQuestion 工具时，TUI 弹窗正常显示但随即自行关闭。工具调用以某种结果（疑似空答案）结束，agent 继续后续执行。用户在弹窗上的操作（选择选项、确认）没有效果。**每次调用 AskUserQuestion 都必现**，不是偶发。

## 症状详情

| 现象 | 期望 | 实际 |
|------|------|------|
| 弹窗持续时间 | 一直显示直到用户操作 | 短暂显示后自动关闭 |
| 用户操作 | 点击选项/Enter 确认后提交给 LLM | 操作无效，弹窗已关闭 |
| 工具调用结果 | 等待用户回答后返回 | 工具自行结束（疑似空答案） |
| agent 后续行为 | 等待用户输入暂停执行 | 继续执行（拿到了某种默认返回） |

## 复现条件

- **复现频率**：必现（每次 AskUserQuestion 调用都出现）
- **触发步骤**：
  1. 启动 TUI，输入任意 prompt 让 agent 执行
  2. agent 在执行过程中调用 AskUserQuestion 工具
  3. 弹窗短暂显示后自动关闭，工具调用结束
- **与已有 issue 的区别**：`2026-05-28-ask-user-popup-auto-close.md` 是 cancel_sent_at 残留导致的偶发问题（需要先 Ctrl+C），本次是每次都必现、与 Ctrl+C 无关

## 可能相关的代码路径

### elicitation 请求-响应链路

1. `peri-acp/src/broker/transport_broker.rs:148-151` — `send_request("elicitation/create", params).await` 发送请求并等待响应
2. TUI 侧 `handle_acp_elicitation()` 创建弹窗，返回 `(true, true, false)` 暂停事件消费
3. 用户操作后 `ask_user_ops.rs:196-200` — `send_response(request_id, Ok(response)).await` 回传答案

### 疑似方向

- `send_request` 可能存在超时机制，超时后返回错误，`handle_questions` 的 `Err(e)` 分支（第 177-179 行）返回 `empty_answers`，agent 拿到空答案继续执行
- 弹窗的 `(true, true, false)` 暂停逻辑可能在事件泵循环中被绕过
- `send_response` 的 `request_id` 与 `send_request` 的对应关系可能有问题

## 涉及文件

- `peri-acp/src/broker/transport_broker.rs:104-182` — `handle_questions()` 发送 elicitation 并处理响应，Err 分支返回空答案
- `peri-tui/src/app/agent_ops_interaction.rs:56-156` — `handle_acp_elicitation()` 创建弹窗
- `peri-tui/src/app/ask_user_ops.rs:92-206` — `ask_user_confirm()` 用户确认后回传答案
- `peri-tui/src/app/agent_ops/acp_bridge.rs:38` — Elicitation 通知路由
- `peri-tui/src/app/agent_ops/polling.rs:79-97` — 事件泵循环，should_break 暂停逻辑

## 根因分析

**根因**：`MultiplexBroker` 竞速 TUI broker 和 Channel broker，而 `ChannelBroker` 对 `Questions` 交互立即返回空答案。

**完整链路**：
1. `builder.rs:230` — `AskUserTool::new(effective_broker)` 使用 MultiplexBroker
2. `builder.rs:211` — `MultiplexBroker` 包含 TUI broker + Channel broker
3. `channel_broker.rs:32-35` — `ChannelBroker` 对 `Questions` 立即返回 `Answers(vec![])`
4. `multiplex.rs:45` — 竞速中 Channel 先返回，空答案被采纳
5. 弹窗确实创建了（TUI broker 的 elicitation 请求已发送），但 agent 已拿到空答案继续执行

**触发条件**：`channel_state` 在 TUI 启动时始终为 `Some`（`app/mod.rs:261`），所以 MultiplexBroker 路径始终激活。HITL 审批不受影响是因为 Channel broker 会真正发送请求到 channel 并等待响应（5 分钟超时）。

## 修复

`peri-acp/src/agent/builder.rs:230` — `AskUserTool` 改用 `permission_broker`（原始 TUI broker）而非 `effective_broker`（MultiplexBroker）。Channel 不支持 Questions 交互，不应参与竞速。
