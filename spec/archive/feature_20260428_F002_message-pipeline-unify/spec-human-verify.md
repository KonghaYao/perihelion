# 消息显示管线统一 人工验收清单

**生成时间:** 2026-04-28
**关联计划:** spec/feature_20260428_F002_message-pipeline-unify/spec-plan.md
**关联设计:** spec/feature_20260428_F002_message-pipeline-unify/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链: `rustc --version`
- [ ] [AUTO] 编译全量项目: `cargo build`

---

## 验收项目

### 场景 1：AgentEvent 拆分——ToolStart + ToolEnd 替代 ToolCall

#### - [x] 1.1 Headless 测试验证事件拆分
- **来源:** spec-plan.md Task 1 步骤 12
- **目的:** 确认 ToolCall 拆分后所有 headless 测试通过
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- headless` → 期望包含: `test result: ok`

---

### 场景 2：AppCore 持有 MessagePipeline

#### - [x] 2.1 Pipeline 字段初始化测试
- **来源:** spec-plan.md Task 2 步骤 8
- **目的:** 确认 AppCore 正确持有并初始化 Pipeline
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_appcore_pipeline_initialized` → 期望包含: `test test_appcore_pipeline_initialized ... ok`

#### - [x] 2.2 AppCore::new 签名变更无回归
- **来源:** spec-plan.md Task 2 步骤 9
- **目的:** 确认所有调用点（mod.rs / panel_ops.rs）正确适配
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib` → 期望包含: `test result: ok`

---

### 场景 3：MessagePipeline handle_event 统一入口

#### - [x] 3.1 编译通过
- **来源:** spec-plan.md Task 3 检查步骤 1
- **目的:** 确认 handle_event 方法编译无错误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望包含: `Finished` 或 `Compiling`（无 `error`）

#### - [x] 3.2 Pipeline 单元测试全部通过
- **来源:** spec-plan.md Task 3 检查步骤 2
- **目的:** 确认 handle_event 路由逻辑正确（流式文本、空 chunk、工具生命周期、Done reconcile、StateSnapshot）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- message_pipeline` → 期望包含: `test result: ok`

---

### 场景 4：全量构建与回归

#### - [x] 4.1 全 workspace 编译无错
- **来源:** spec-plan.md 全局 / spec-design.md §风险与缓解
- **目的:** 确认所有 crate 编译通过，无类型不匹配
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 4.2 全量测试通过
- **来源:** spec-plan.md 全局 / spec-design.md §风险与缓解（测试覆盖不足）
- **目的:** 确认无功能回归，涵盖流式 vs 恢复一致性
- **操作步骤:**
  1. [A] `cargo test` → 期望包含: `test result: ok`

---

### 场景 5：边界与回归

#### - [x] 5.1 AgentEvent 无遗留 ToolCall 引用
- **来源:** spec-design.md §6 AgentEvent 调整 / spec-plan.md Task 1
- **目的:** 确认 ToolCall 变体已完全替换
- **操作步骤:**
  1. [A] `grep -rn "ToolCall" rust-agent-tui/src/` → 期望精确: （无输出，即无残留引用）

#### - [x] 5.2 PipelineAction 变体覆盖完整
- **来源:** spec-design.md §3 PipelineAction → RenderEvent 映射
- **目的:** 确认所有 PipelineAction 变体均被 handle_event 使用
- **操作步骤:**
  1. [A] `grep -c "PipelineAction::" rust-agent-tui/src/app/message_pipeline.rs` → 期望包含: 数字 >= 7（AddMessage/AppendChunk/UpdateLast/RemoveLast/RemoveLastN/RebuildAll/StreamingDone/None）

#### - [x] 5.3 subagent_group_idx 已移除
- **来源:** spec-design.md §1 AppCore 持有 MessagePipeline（subagent_group_idx 移除）
- **目的:** 确认 subagent_group_idx 被 pipeline.in_subagent() 替代（⚠ 注意: plan 仅 Task 2 提及保留，实际移除在 Task 4——此步骤验证移除完成）
- **操作步骤:**
  1. [A] `grep -rn "subagent_group_idx" rust-agent-tui/src/` → 期望精确: （无输出，即已完全移除）

#### - [x] 5.4 agent_ops 无直接 view_messages 操作
- **来源:** spec-design.md §目标 / spec-plan.md Task 4（重构 agent_ops）
- **目的:** 确认 agent_ops 通过 PipelineAction 间接操作 view_messages
- **操作步骤:**
  1. [A] `grep -n "view_messages\\.push\\|view_messages\\.pop\\|view_messages = " rust-agent-tui/src/app/agent_ops.rs` → 期望精确: （无输出，即无直接操作）

---

## 验收后清理

（本次无后台服务，无需清理）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Headless 测试验证事件拆分 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | Pipeline 字段初始化测试 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | AppCore::new 签名变更无回归 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | 编译通过 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | Pipeline 单元测试全部通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | 全 workspace 编译无错 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | 全量测试通过 | 1 | 0 | ⬜ |
| 场景 5 | 5.1 | 无遗留 ToolCall 引用 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | PipelineAction 变体覆盖完整 | 1 | 0 | ⬜ |
| 场景 5 | 5.3 | subagent_group_idx 已移除 | 1 | 0 | ⬜ |
| 场景 5 | 5.4 | agent_ops 无直接 view_messages 操作 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
