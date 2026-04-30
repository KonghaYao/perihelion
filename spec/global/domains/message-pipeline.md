# 消息管线 领域

## 领域综述

消息管线领域负责统一 Agent 事件到 TUI 视图模型的消息转换，处理流式增量更新和历史恢复两条路径，确保最终一致性。

核心职责：
- MessagePipeline 成为消息状态管理唯一入口
- PipelineAction 枚举统一描述所有 UI 变更操作
- 流式 AppendChunk 优化 + Done 时 reconcile 尾部重建确保一致性
- SubAgent 路由逻辑集中管理

## 核心流程

### 消息处理管线

```
AgentEvent → MessagePipeline.handle_event()
  → 转换为 Vec<PipelineAction>
  → apply_pipeline_action():
      AddMessage → 追加 view_model
      AppendChunk → 增量更新最后一条 assistant
      RebuildAll{prefix_len, tail_vms} → 替换尾部
      RemoveLast / RemoveLastN → 删除最近消息
      StreamingDone → 最终一致性重建
  → Done/Interrupted → reconcile_tail() 尾部重建
```

### 尾部重建流程

```
reconcile_tail(round_start_vm_idx)
  → 找到最后一条 Human 消息
  → 从该位置开始重建 view_models
  → 返回 (prefix_len, tail_vms)
  → RebuildAll 只替换尾部，保留前缀
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 核心组件 | MessagePipeline 结构体，持有 view_messages 和 render_tx |
| 操作枚举 | PipelineAction: None/AddMessage/AppendChunk/UpdateLast/RemoveLast/RemoveLastN/RebuildAll |
| 事件拆分 | ToolCall 拆分为 ToolStart + ToolEnd 两个独立事件 |
| 流式优化 | AppendChunk 直接操作渲染层，finalize 边界 reconcile |
| 尾部重建 | reconcile_tail() + round_start_vm_idx 记录轮次起始位置 |
| SubAgent 路由 | 从 agent_ops 迁入 Pipeline，移除 subagent_group_idx |

## Feature 附录

### feature_20260428_F002_message-pipeline-unify
**摘要:** 统一流式与历史恢复的消息显示管线
**关键决策:**
- MessagePipeline 成为消息状态管理唯一入口，agent_ops 不再手动操作 view_messages
- PipelineAction 枚举统一描述所有 UI 变更操作
- AgentEvent::ToolCall 拆分为 ToolStart + ToolEnd 两个独立事件
- 流式 AppendChunk 优化保留，Done 时 reconcile 确保最终一致性
- SubAgent 路由逻辑从 agent_ops 迁入 Pipeline
**归档:** [链接](../../archive/feature_20260428_F002_message-pipeline-unify/)
**归档日期:** 2026-04-30

### feature_20260430_F002_reconcile-on-done-interrupted
**摘要:** Done/Interrupted 事件触发尾部重建确保流式与恢复路径一致
**关键决策:**
- RebuildAll 改为携带 prefix_len + tail_vms 的结构体形式，只替换尾部
- 新增 reconcile_tail() 方法，从最后一条 Human 消息开始重建 view_models
- 通过 round_start_vm_idx 记录本轮起始位置
- 移除 StreamingDone 变体，职责合并到 RebuildAll
- 保留全量 reconcile() 方法供 CompactDone 等其他场景使用
**归档:** [链接](../../archive/feature_20260430_F002_reconcile-on-done-interrupted/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 渲染依赖 MessagePipeline 输出的 view_models
- → [agent.md](./agent.md) — AgentEvent 事件定义在核心层
