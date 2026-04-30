# Done/Interrupted 事件 Reconcile 修复 人工验收清单

**生成时间:** 2026-04-30 23:30
**关联计划:** spec/feature_20260430_F002_reconcile-on-done-interrupted/spec-plan.md
**关联设计:** spec/feature_20260430_F002_reconcile-on-done-interrupted/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 编译项目: `cargo build -p rust-agent-tui 2>&1 | tail -5`
- [x] [AUTO] 确认测试框架可用: `cargo test -p rust-agent-tui --lib -- --list 2>&1 | tail -3`

---

## 验收项目

### 场景 1：数据模型变更（PipelineAction + reconcile_tail + round_start_vm_idx）

#### - [x] 1.1 PipelineAction::RebuildAll 使用结构体形式
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §1
- **目的:** 确认 RebuildAll 区分前缀与尾部
- **操作步骤:**
  1. [A] `grep -A 3 "RebuildAll" rust-agent-tui/src/app/message_pipeline.rs | head -4` → 期望包含: `prefix_len: usize` 和 `tail_vms: Vec<MessageViewModel>`

#### - [x] 1.2 StreamingDone 从 PipelineAction 完全移除
- **来源:** spec-plan.md Task 1+4 / spec-design.md §5
- **目的:** 确认废弃变体已清除
- **操作步骤:**
  1. [A] `grep -rn "PipelineAction::StreamingDone" rust-agent-tui/src/` → 期望精确: (空输出，exit code 1)

#### - [x] 1.3 reconcile_tail() 方法存在且签名正确
- **来源:** spec-plan.md Task 1 / spec-design.md §2+§6
- **目的:** 确认核心方法实现
- **操作步骤:**
  1. [A] `grep "pub fn reconcile_tail" rust-agent-tui/src/app/message_pipeline.rs` → 期望包含: `round_start_vm_idx: usize`

#### - [x] 1.4 AppCore 包含 round_start_vm_idx 字段并初始化
- **来源:** spec-plan.md Task 1 / spec-design.md §6
- **目的:** 确认轮次索引记录机制
- **操作步骤:**
  1. [A] `grep -c "round_start_vm_idx" rust-agent-tui/src/app/core.rs` → 期望精确: `2`

---

### 场景 2：事件处理适配（Done/Interrupted/submit_message/apply）

#### - [x] 2.1 Done 事件不再返回 StreamingDone
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 pipeline 层 Done 行为变更
- **操作步骤:**
  1. [A] `grep -A 2 "AgentEvent::Done =>" rust-agent-tui/src/app/message_pipeline.rs` → 期望包含: `PipelineAction::None`

#### - [x] 2.2 Done 事件处理调用 reconcile_tail
- **来源:** spec-plan.md Task 2 / spec-design.md §3
- **目的:** 确认流式结束时触发尾部重建
- **操作步骤:**
  1. [A] `grep -A 15 "AgentEvent::Done =>" rust-agent-tui/src/app/agent_ops.rs | grep "reconcile_tail"` → 期望包含: `reconcile_tail`

#### - [x] 2.3 Interrupted 事件处理调用 reconcile_tail
- **来源:** spec-plan.md Task 2 / spec-design.md §3
- **目的:** 确认中断时同样触发尾部重建
- **操作步骤:**
  1. [A] `grep -A 15 "AgentEvent::Interrupted =>" rust-agent-tui/src/app/agent_ops.rs | grep "reconcile_tail"` → 期望包含: `reconcile_tail`

#### - [x] 2.4 submit_message 在 push Human VM 前记录 round_start_vm_idx
- **来源:** spec-plan.md Task 2 / spec-design.md §6
- **目的:** 确认轮次索引记录时机正确
- **操作步骤:**
  1. [A] `grep -B 1 "MessageViewModel::user" rust-agent-tui/src/app/agent_ops.rs | grep "round_start_vm_idx"` → 期望包含: `round_start_vm_idx = self.core.view_messages.len()`

#### - [x] 2.5 RebuildAll 处理使用截断+extend 模式
- **来源:** spec-plan.md Task 2 / spec-design.md §4
- **目的:** 确认 apply_pipeline_action 正确适配
- **操作步骤:**
  1. [A] `grep -A 5 "PipelineAction::RebuildAll {" rust-agent-tui/src/app/agent_ops.rs | head -6` → 期望包含: `truncate(prefix_len)` 和 `extend(tail_vms)`

#### - [x] 2.6 CompactDone 使用新 RebuildAll 形式（prefix_len: 0）
- **来源:** spec-plan.md Task 2 / spec-design.md §1
- **目的:** 确认全量重建场景兼容
- **操作步骤:**
  1. [A] `grep -A 2 "prefix_len: 0" rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `prefix_len: 0`

---

### 场景 3：单元测试与集成测试覆盖

#### - [x] 3.1 reconcile_tail 单元测试全部通过
- **来源:** spec-plan.md Task 1 执行步骤
- **目的:** 确认核心方法边界覆盖
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib reconcile_tail 2>&1 | grep "test result"` → 期望包含: `test result: ok`

#### - [x] 3.2 事件处理单元测试全部通过
- **来源:** spec-plan.md Task 2 执行步骤
- **目的:** 确认 Done/Interrupted/submit 逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib reconcile_event_handling 2>&1 | grep "test result"` → 期望包含: `test result: ok`

#### - [x] 3.3 集成测试覆盖尾部重建一致性
- **来源:** spec-plan.md Task 3
- **目的:** 确认 reconcile_tail 与全量转换结果一致
- **操作步骤:**
  1. [A] `grep -c "fn test_reconcile_tail" rust-agent-tui/src/app/message_pipeline.rs` → 期望精确: `6`

#### - [x] 3.4 全量测试套件无新增回归
- **来源:** spec-plan.md Task 4
- **目的:** 确认改动无副作用
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"` → 期望包含: `274 passed`

---

### 场景 4：渲染线程兼容性

#### - [x] 4.1 RenderEvent::StreamingDone 仍保留
- **来源:** spec-plan.md Task 4 / spec-design.md §5
- **目的:** 确认渲染线程内部使用未受影响
- **操作步骤:**
  1. [A] `grep -c "StreamingDone" rust-agent-tui/src/ui/render_thread.rs` → 期望包含: `2` (枚举定义 + 处理分支)

#### - [x] 4.2 RebuildAll 元组形式已完全替换
- **来源:** spec-plan.md Task 4
- **目的:** 确认无遗漏的旧调用形式
- **操作步骤:**
  1. [A] `grep -rn "RebuildAll(" rust-agent-tui/src/` → 期望精确: (空输出，exit code 1)

---

## 验收后清理

无需清理（无后台服务启动）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | RebuildAll 结构体形式 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | StreamingDone 完全移除 | 1 | 0 | ✅ |
| 场景 1 | 1.3 | reconcile_tail 方法签名 | 1 | 0 | ✅ |
| 场景 1 | 1.4 | round_start_vm_idx 字段 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | Done 不返回 StreamingDone | 1 | 0 | ✅ |
| 场景 2 | 2.2 | Done 调用 reconcile_tail | 1 | 0 | ✅ |
| 场景 2 | 2.3 | Interrupted 调用 reconcile_tail | 1 | 0 | ✅ |
| 场景 2 | 2.4 | submit_message 记录索引 | 1 | 0 | ✅ |
| 场景 2 | 2.5 | RebuildAll 截断+extend | 1 | 0 | ✅ |
| 场景 2 | 2.6 | CompactDone 兼容 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | reconcile_tail 单测通过 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | 事件处理单测通过 | 1 | 0 | ✅ |
| 场景 3 | 3.3 | 集成测试覆盖 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | 全量测试无回归 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | RenderEvent::StreamingDone 保留 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | RebuildAll 元组形式清除 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过
