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

## Issue 经验附录

### issue_2026-05-12-deferred-tool-list-nondeterministic-order
**摘要:** 多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** HashMap 顺序, Prompt Cache, 缓存前缀, ToolSearchIndex
**问题本质:** HashMap 迭代顺序不确定（Rust 默认 RandomState），跨进程重启时 API 请求前缀变化，导致 Prompt Cache 失效。涉及 ToolSearchIndex 的 deferred tools 列表注入和 executor 的 tool_refs 两个独立位置
**通用模式:** 所有需要跨进程复用的序列化内容（system prompt、tools 数组）必须保证顺序稳定。任何参与缓存前缀的数据结构，其迭代顺序必须是确定性的
**架构影响:** ToolSearchIndex 从 submit 级局部变量提升到 session 级共享 Arc，减少重复构建的同时保证缓存一致性
**技术决策:** 工具列表按名称排序；ToolSearchIndex 会话级缓存；每轮注入缓存提示词而非重新构建
**涉及文件:** peri-middlewares/src/tool_search/tool_index.rs, peri-agent/src/agent/executor/mod.rs, peri-tui/src/app/agent_comm.rs, peri-tui/src/app/agent.rs, peri-tui/src/app/agent_submit.rs
**CLAUDE.md 链接:** true

### issue_2026-05-12-skill-preload-invalidates-prompt-cache
**摘要:** Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** Prompt Cache, prepend_message, add_message, cache_control
**问题本质:** SkillPreloadMiddleware 用 prepend_message 将合成消息插入 index 0，改变了第一条 user 消息的位置，Anthropic 的 cache_control 标记落在不稳定的合成消息上，导致首轮 cache miss
**通用模式:** 向消息数组头部插入内容会改变缓存边界，应优先使用尾部追加（add_message）。缓存控制标记（cache_control）的位置决定了缓存前缀的稳定性
**技术决策:** prepend_message 改为 add_message，使 preload 工具调用追加在用户消息之后
**涉及文件:** peri-middlewares/src/subagent/skill_preload.rs, peri-agent/src/agent/compact/re_inject.rs, peri-agent/src/llm/anthropic.rs
**CLAUDE.md 链接:** true

### issue_2026-05-12-systemnote-position-drift-on-rebuild
**摘要:** SystemNote 在 RebuildAll 后堆积到消息列表末尾
**状态:** Fixed
**归档日期:** 2026-05-13
**关键词:** SystemNote, RebuildAll, ephemeral_notes, 锚点机制
**问题本质:** AddMessage 直接 push 到 view_messages 末尾，RebuildAll 的 saved_notes 机制保存后追加到末尾，导致 SystemNote 永远漂移到消息流最后
**通用模式:** 纯 UI 层的临时 VM（不在 BaseMessage 中）需要独立的锚点机制来维持位置。RebuildAll 的 drain+重建会破坏所有尾部追加内容的位置
**架构影响:** 引入 ephemeral_notes 字段记录 (锚点, VM) 对，RebuildAll 时按锚点位置重新插入。这是 message-pipeline 处理纯 UI VM 生命周期的通用模式
**技术决策:** VM 索引锚点方案——记录创建时 view_messages.len() 作为锚点，RebuildAll 时根据锚点与 prefix_len 的关系决定保留/丢弃/重插入
**涉及文件:** peri-tui/src/app/agent_render.rs, peri-tui/src/app/message_pipeline.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-cache-warning-discarded-by-rebuild
**摘要:** CacheWarning 消息被 RebuildAll 立即丢弃，用户无法看到
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** CacheWarning, RebuildAll, saved_notes, ephemeral VM
**问题本质:** RebuildAll 的 saved_notes 过滤器只保留 SystemNote 变体，不保留 CacheWarning，导致缓存警告一闪而过
**通用模式:** 新增 ephemeral VM 变体时，必须同步更新 RebuildAll 的 saved_notes 过滤逻辑，否则会被 drain 丢弃
**涉及文件:** peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent_render.rs, peri-tui/src/ui/message_view.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-compact-ephemeral-notes-not-cleared
**摘要:** Compact 完成后残留 "正在压缩上下文…" 系统通知
**状态:** Closed
**归档日期:** 2026-05-13
**关键词:** ephemeral_notes, compact, RebuildAll, prefix_len: 0
**问题本质:** Compact 完成后的 RebuildAll { prefix_len: 0 } 保留所有锚点 >= 0 的 ephemeral_notes，包括 compact 前的旧通知
**通用模式:** 全量重建（prefix_len: 0）时，应先清理过期的 ephemeral_notes，否则所有历史临时通知都会被保留
**涉及文件:** peri-tui/src/app/agent_compact.rs, peri-tui/src/app/thread_ops.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent_render.rs
**CLAUDE.md 链接:** false

### issue_2026-05-14-cache-breakpoint-structural-inefficiency
**摘要:** 82% system 未缓存 + message 断点在 tool_result-only 消息上静默失效
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** Prompt Cache, 断点回退, 缓存驱逐, system缓存, cache_control
**问题本质:** (1) system prompt 中 middleware 注入内容（CLAUDE.md、Skills）永远无 cache_control，82.6% 无法缓存；(2) second-to-last user message 在多轮工具调用中可能是 tool_result-only 消息，断点跳过；(3) ZhipuAI 等 Provider 的 token 报告格式与 Anthropic 原生不同（cache_read 可超过 input_tokens，cache_creation 始终为 0）。
**通用模式:** cache_control 断点策略需要回退搜索机制——目标消息不含 text block 时向前搜索最近的含 text 消息。断点覆盖范围之外的完整前缀缓存由 Provider 端管理，不受客户端控制。Provider 端的缓存驱逐是随机事件，客户端只能通过增加断点密度来提高小粒度缓存条目的存活概率。
**架构影响:** 新增 system[last] cache_control 覆盖整个 system prompt 区域，移除被 msg[first] 隐式覆盖的 tools cache_control 冗余断点
**技术决策:** apply_cache_to_messages 断点回退搜索；system 序列化时对最后一个 block 标记 cache_control
**涉及文件:** peri-agent/src/llm/anthropic.rs
**CLAUDE.md 链接:** true

### issue_2026-05-15-thinking-tail-preview
**摘要:** 最后一条 AI 消息无正文时展示思考最后 1 行
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** Reasoning渲染, tail_lines, ContentBlockView, Hash设计
**问题本质:** ContentBlockView::Reasoning 只存储字符数，推理内容完全不可见。需要在不改变 Hash 等价性（char_count 决定 identity）的前提下携带展示用数据。
**通用模式:** ContentBlockView 的 Hash/PartialEq 设计中，语义身份字段（char_count）参与等价判断，展示辅助字段（text）不参与，触发重渲染的字段（tail_lines）选择性参与。这是一个"身份 ≠ 展示"的解耦模式——同一 semantic identity 可以有多种展示状态。
**架构影响:** 后处理模式——在 build_tail_vms() 末尾执行 add_thinking_tail_snapshot()，遵循"组装→后处理"的管线阶段分隔
**技术决策:** text 不参与 Hash（仅 char_count 决定等价性），tail_lines 参与 Hash（变化触发重渲染）
**涉及文件:** peri-tui/src/ui/message_view.rs, peri-tui/src/app/message_pipeline.rs, peri-tui/src/ui/message_render.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication

**摘要:** SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 跨轮次累积, frozen_vms, begin_round 清理, 位置匹配, agent_id 校验
**问题本质:** frozen_subagent_vms 仅在 clear() 清空，done()/begin_round() 均未清空，导致跨轮次累积。merge_frozen_subagents 按位置而非 agent_id 匹配，新轮次 SubAgentGroup 被旧轮次数据污染
**通用模式:** 轮次作用域的状态（frozen_vms, ephemeral_notes 等）必须在 begin_round 或 done 时显式清空。基于位置的匹配在跨轮次场景下不可靠——应使用唯一标识（如 agent_id）匹配
**架构影响:** Pipeline 的状态生命周期必须与轮次边界对齐，clear() 作为全量重置的终极手段不应是唯一清理路径
**涉及文件:** peri-tui/src/app/message_pipeline.rs, peri-tui/src/app/message_pipeline_test.rs
**CLAUDE.md 链接:** true

### issue_2026-05-17-pipeline-render-heavy-files

**摘要:** message_pipeline.rs（1067 行）和 message_view.rs（1061 行）拆分为子模块，Pipeline 按 transform/reconcile/view_model 分离，渲染层按 render/layout/scroll 分离
**状态:** Fixed
**归档日期:** 2026-05-18
**涉及文件:** peri-tui/src/app/message_pipeline.rs, peri-tui/src/ui/message_view.rs
**说明:** 纯代码组织优化（注意 TRAP 约束：Ephemeral VM 锚点机制、prefix_len vs round_start_vm_idx 维度区分、RebuildAll 只能在非 Pipeline 层触发），无领域认知提炼。

### issue_2026-05-18-subagent-duplicate-state-on-completion

**摘要:** SubAgent 完成后显示重复卡片：ToolStart 和 SubAgentStart 事件双重创建 SubAgentState
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** SubAgent 重复卡片, ToolStart/SubAgentStart 竞态, 双重创建
**问题本质:** ToolStart（name="Agent"）和 SubAgentStart 两个事件都调用 tool_start_internal() 创建 SubAgentState。SubAgentEnd 只冻结第一个匹配项，第二个残留为 is_running=true，直到 Done 才清理。
**通用模式:** 当同一语义实体有多个事件入口点时，必须明确单一创建职责。事件之间不是"累加"关系——应做"去重"判断。修复方案是让 ToolStart 仅注册 tool_call/pending_tool，由 SubAgentStart 独占 SubAgentState 创建职责。
**涉及文件:** peri-tui/src/app/message_pipeline/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-20-session-restore-renders-system-prompt

**摘要:** Session 恢复后 System Prompt 和 Compact Summary 被渲染为可见消息
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** session恢复, System消息过滤, messages_to_view_models, SystemNote泄漏
**问题本质:** `messages_to_view_models()` 是所有恢复路径（`open_thread()` + `reconcile()`）的共享入口，但未过滤 `BaseMessage::System` 变体，导致持久化存储中的 system prompt 和 compact summary 被转换为 `SystemNote` VM 渲染为可见卡片。
**通用模式:** 所有从持久化到渲染的消息转换路径必须在入口处统一过滤 System 变体，不依赖各调用点自行处理。
**架构影响:** 强化了 `messages_to_view_models()` 作为"唯一转换入口"的架构定位——内部消息（System）与用户可见消息（Human/Ai/Tool）的边界应在此函数内明确。
**涉及文件:** peri-tui/src/app/thread_ops.rs, peri-tui/src/app/message_pipeline/transform.rs, peri-tui/src/ui/message_view/mod.rs
**CLAUDE.md 链接:** true

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 渲染依赖 MessagePipeline 输出的 view_models
- → [agent.md](./agent.md) — AgentEvent 事件定义在核心层
