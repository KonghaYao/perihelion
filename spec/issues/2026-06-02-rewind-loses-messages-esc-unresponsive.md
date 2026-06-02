# Rewind 回退后前文消息全部丢失 + 双击 ESC 偶发无响应

**状态**：Fixed
**优先级**：高
**创建日期**：2026-06-02
**修复日期**：2026-06-02

## 问题描述

Rewind 功能存在两个严重问题：（1）回退消息后，界面上只显示 rewind 的系统通知（如"已回滚 N 条消息"），保留的消息内容全部消失，用户只能看到空白对话 + 一条 rewind 通知；（2）在 Agent 空闲状态下双击 ESC 偶发无法触发 rewind 选择器弹窗。

## 症状详情

### 现象 1：Rewind 后前文全部消失

| 维度 | 描述 |
|------|------|
| 操作 | 选择回退到某个用户消息节点（如回退 1 条消息） |
| 期望 | 回退目标之后的消息被移除，目标之前的消息正常显示 |
| 实际 | 界面只剩一条 rewind 系统通知（"已回滚 N 条消息"），所有保留的消息内容不可见 |
| 严重性 | 即使只回退 1 条消息，前面 10 条也全部不可见 |

### 现象 2：双击 ESC 偶发无响应

| 维度 | 描述 |
|------|------|
| 操作 | Agent 空闲时双击 ESC 触发 rewind 选择器 |
| 期望 | 弹出 rewind 选择器弹窗，显示可回退的用户消息列表 |
| 实际 | 偶发双击 ESC 完全无反应，不弹出选择器 |
| 场景 | Agent 已完成回答、输入框空闲时出现 |

## 复现条件

- **复现频率**：偶发（两个问题都是）
- **触发步骤（现象 1）**：
  1. 进行多轮对话（如 10+ 轮）
  2. Agent 空闲时双击 ESC 打开 rewind 选择器
  3. 选择回退到某个用户消息
  4. 确认回退
  5. 观察界面：只剩 rewind 通知，前文消息消失
- **触发步骤（现象 2）**：
  1. Agent 完成回答后处于空闲状态
  2. 双击 ESC（间隔 < 2 秒）
  3. 无响应，不弹出 rewind 选择器

## 涉及文件

- `peri-tui/src/app/agent_ops/rewind.rs` —— Rewind 弹窗 UI 逻辑（打开/确认/取消）
- `peri-tui/src/app/agent_compact.rs:104-135` —— `handle_rewind_completed`：Rewind 完成后更新消息历史和 pipeline
- `peri-acp/src/session/command/rewind.rs` —— `/rewind` 命令执行：截断 history、提取文件变更、逆向恢复
- `peri-tui/src/event/keyboard/normal_keys.rs:48-61` —— 双击 ESC 触发 rewind 的按键处理逻辑
- `peri-tui/src/app/rewind_prompt.rs` —— Rewind 弹窗数据结构

## 根因分析

### Bug 1 根因：`handle_rewind_completed` 未渲染保留消息

`handle_rewind_completed`（`agent_compact.rs:104`）执行流程：
1. `pipeline.clear()` + `pipeline.restore_completed(messages)` — 保留消息存入 `pipeline.completed`，但 `has_snapshot_this_round = false`
2. `RebuildAll { prefix_len: 0, tail_vms: [rewind通知] }` — `drain(0..)` 清空所有 view_messages，只插入一条 rewind 通知

**问题**：保留的消息只存在 `pipeline.completed` 里，但没有被转为 `MessageViewModel` 渲染。`has_snapshot_this_round = false` 意味着后续的 `build_tail_vms()` 会跳过 completed → VM 转换。

对比 compact：compact 后 agent 会 resubmit，`StateSnapshot` 触发 `set_completed()` 设置 `has_snapshot = true`，后续 rebuild 能正常渲染。rewind 没有后续 agent 执行，所以消息永远不会被渲染。

**修复**：在 `handle_rewind_completed` 中，先调用 `messages_to_view_models()` 将保留消息转为 VMs，再把 rewind 通知追加到末尾，一起放入 `tail_vms`。

### Bug 2 根因：兜底分支重置 `rewind_pending_since`

`normal_keys.rs:302` 的 `_ =>` 兜底分支无差别重置 `rewind_pending_since = None`。两次 ESC 之间如果终端产生了任何中间事件（如 focus event、未知 key sequence），双击序列被中断，第二次 ESC 被当作"第一次"。

**修复**：不在兜底分支重置 `rewind_pending_since`。用户有意中断 rewind 双击的方式（输入实际字符）已被 `input if input.key != Key::Enter` 分支自然处理。

## 修复变更

- `peri-tui/src/app/agent_compact.rs` — `handle_rewind_completed` 渲染保留消息
- `peri-tui/src/event/keyboard/normal_keys.rs` — 移除兜底分支中 `rewind_pending_since` 重置
