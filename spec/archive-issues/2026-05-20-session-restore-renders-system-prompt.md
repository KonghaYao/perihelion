> 归档于 2026-05-20，原路径 spec/issues/2026-05-20-session-restore-renders-system-prompt

# Session 恢复后 System Prompt 和 Compact Summary 被渲染为可见消息

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-20
**修复日期**：2026-05-20

## 问题描述

从持久化存储恢复历史会话时，消息列表顶部渲染了不该显示的内部消息：完整的 system prompt 全文和 compact 生成的 summary 消息。这两类消息是内部状态（`BaseMessage::System` 变体），不应出现在用户可见的消息流中。

## 症状详情

| 阶段 | 现象 |
|------|------|
| 正常会话中 | system prompt 和 compact summary 不可见，符合预期 |
| Session 恢复后 | 消息列表**顶部**出现完整 system prompt 全文 + compact summary 文本 |
| 期望行为 | 恢复后的消息列表与正常会话一致，system/summary 消息被过滤 |

出现的内容：
1. **完整的 system prompt 全文** — 包含所有段落（静态 + 动态）的系统提示词文本
2. **compact 后的 summary 消息** — `BaseMessage::system(summary)` 被渲染为可见消息

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 进行一次包含 compact 的会话
  2. 退出 TUI
  3. 重新启动 TUI，恢复历史会话
  4. 消息列表顶部出现 system prompt + compact summary
- **环境**：所有环境

## 根因分析

### 缺陷路径：`open_thread()` session 恢复

```
thread_ops.rs:158  store.load_messages(&tid)
                              ↓ 加载全部消息（含 System）
thread_ops.rs:173  agent_state_messages = base_msgs
                              ↓ 未过滤
thread_ops.rs:176  messages_to_view_models(&base_msgs, ...)
                              ↓ 逐条转换，未跳过 System
message_view/mod.rs:547  BaseMessage::System → MessageViewModel::SystemNote
                              ↓ 渲染为可见卡片
```

`messages_to_view_models()`（`transform.rs:53`）遍历所有消息时没有跳过 `BaseMessage::System` 变体，导致 System 消息被转为 `SystemNote` VM 渲染出来。

### 对比：compact 恢复路径（正确）

`handle_compact_completed`（`agent_compact.rs:34`）把消息放进 `pipeline.restore_completed(messages)`，然后只显示一条 compact 通知 VM（`RebuildAll { prefix_len: 0, tail_vms: vec![compact_notification] }`）。后续 `build_tail_vms` 中 `last_human_offset`（`reconcile.rs:86`）从最后一条 Human 消息开始，跳过开头的 System 消息。

### 修复方向

在 `messages_to_view_models()` 内部跳过 `BaseMessage::System` 变体，或在 `open_thread()` 调用前过滤。两处选一处修即可——推荐在 `messages_to_view_models` 内部统一过滤，因为该函数是所有恢复路径的共享入口。

### 实际修复

在 `messages_to_view_models()` 循环开头添加 `BaseMessage::System` 过滤（`transform.rs:58-61`），跳过所有 System 变体。选择在此处修复是因为该函数是所有恢复路径的共享入口（包括 `reconcile()`），统一过滤最安全。37 个 message_pipeline 测试全部通过，无回归。

## 涉及文件

- `peri-tui/src/app/thread_ops.rs:155-184` — `open_thread()` 恢复路径，未过滤 System 消息
- `peri-tui/src/app/message_pipeline/transform.rs:53-84` — `messages_to_view_models()` 未跳过 System 变体
- `peri-tui/src/ui/message_view/mod.rs:547` — `from_base_message_with_cwd` 将 System 转为 SystemNote
- `peri-tui/src/app/agent_compact.rs:34-75` — compact 恢复路径（正确，仅作对比参考）
- `peri-tui/src/app/message_pipeline/reconcile.rs:78-91` — `build_tail_vms` 通过 `last_human_offset` 跳过 System（正确，仅作对比参考）
