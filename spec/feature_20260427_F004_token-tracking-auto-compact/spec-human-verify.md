# Token 用量追踪与 Auto-Compact 机制 人工验收清单

**生成时间:** 2026-04-27
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链: `rustc --version`
- [x] [AUTO] 编译核心 crate: `cargo build -p rust-create-agent 2>&1 | tail -5`
- [x] [AUTO] 编译全 workspace: `cargo build 2>&1 | tail -5`

### 测试数据准备
- [x] 确保至少配置一个 LLM Provider（Anthropic 或 OpenAI 兼容）用于 TUI 端到端验证

---

## 验收项目

### 场景 1：编译与基础环境

#### - [x] 1.1 核心 crate 编译通过
- **来源:** spec-plan.md Task 0
- **目的:** 确认构建环境可用
- **操作步骤:**
  1. [A] `cargo build -p rust-create-agent 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 1.2 基础测试可执行
- **来源:** spec-plan.md Task 0
- **目的:** 确认测试工具链正常
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_agent_state_new 2>&1 | tail -5` → 期望包含: `ok`

---

### 场景 2：TokenTracker 核心数据模型

#### - [x] 2.1 TokenTracker 与 ContextBudget 已导出至 prelude
- **来源:** spec-plan.md Task 1
- **目的:** 确认公共 API 可达
- **操作步骤:**
  1. [A] `grep -n "TokenTracker\|ContextBudget" rust-create-agent/src/lib.rs` → 期望包含: `token::{ContextBudget, TokenTracker}`

#### - [x] 2.2 token 模块已注册
- **来源:** spec-plan.md Task 1
- **目的:** 确认模块声明完整
- **操作步骤:**
  1. [A] `grep -n "pub mod token" rust-create-agent/src/agent/mod.rs` → 期望包含: `pub mod token`

#### - [x] 2.3 TokenTracker + ContextBudget 单元测试全部通过
- **来源:** spec-plan.md Task 1
- **目的:** 确认累积、估算、阈值判断逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- agent::token::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

---

### 场景 3：AgentState 集成与 executor 自动累积

#### - [x] 3.1 State trait 包含 token_tracker 访问器
- **来源:** spec-plan.md Task 2
- **目的:** 确认 trait 扩展完整
- **操作步骤:**
  1. [A] `grep -n "token_tracker" rust-create-agent/src/agent/state.rs` → 期望包含: `fn token_tracker(&self)` 和 `fn token_tracker_mut(&mut self)`

#### - [x] 3.2 executor 中有 accumulate 调用
- **来源:** spec-plan.md Task 2
- **目的:** 确认每轮 LLM 调用自动累积 token
- **操作步骤:**
  1. [A] `grep -n "token_tracker_mut" rust-create-agent/src/agent/executor.rs` → 期望包含: `token_tracker_mut().accumulate`

#### - [x] 3.3 AgentState token_tracker 单元测试通过
- **来源:** spec-plan.md Task 2
- **目的:** 确认字段初始化和累积正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- agent::state::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 4：ReactLLM context_window 模型映射

#### - [x] 4.1 ReactLLM trait 包含 context_window 方法
- **来源:** spec-plan.md Task 3
- **目的:** 确认 trait 扩展和 blanket impl 转发
- **操作步骤:**
  1. [A] `grep -n "context_window" rust-create-agent/src/agent/react.rs` → 期望包含: `fn context_window(&self) -> u32`

#### - [x] 4.2 BaseModelReactLLM 实现模型→窗口映射
- **来源:** spec-plan.md Task 3 / spec-design.md §2.6
- **目的:** 确认 Claude/DeepSeek/GPT-4o 等模型映射正确
- **操作步骤:**
  1. [A] `grep -n "context_window" rust-create-agent/src/llm/react_adapter.rs` → 期望包含: `fn context_window`

#### - [x] 4.3 context_window 映射单元测试通过
- **来源:** spec-plan.md Task 3
- **目的:** 验证各模型 context_window 返回值正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- llm::react_adapter::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 5：TUI Token 数据流与状态栏展示

#### - [x] 5.1 核心 AgentEvent 包含 ContextWarning 变体
- **来源:** spec-plan.md Task 4
- **目的:** 确认事件类型已扩展
- **操作步骤:**
  1. [A] `grep -n "ContextWarning" rust-create-agent/src/agent/events.rs` → 期望包含: `ContextWarning`

#### - [x] 5.2 TUI AgentEvent 包含 TokenUsageUpdate 变体
- **来源:** spec-plan.md Task 4
- **目的:** 确认 TUI 层事件扩展
- **操作步骤:**
  1. [A] `grep -n "TokenUsageUpdate" rust-agent-tui/src/app/events.rs` → 期望包含: `TokenUsageUpdate`

#### - [x] 5.3 map_executor_event 转发 LlmCallEnd 为 TokenUsageUpdate
- **来源:** spec-plan.md Task 4
- **目的:** 确认数据流贯通（原 LlmCallEnd => None 已改为转发）
- **操作步骤:**
  1. [A] `grep -n "TokenUsageUpdate" rust-agent-tui/src/app/agent.rs` → 期望包含: `AgentEvent::TokenUsageUpdate`

#### - [x] 5.4 AgentComm 包含 token 追踪状态字段
- **来源:** spec-plan.md Task 4
- **目的:** 确认 session_tracker/context_window/needs_auto_compact/auto_compact_failures 字段存在
- **操作步骤:**
  1. [A] `grep -n "session_token_tracker\|needs_auto_compact\|auto_compact_failures" rust-agent-tui/src/app/agent_comm.rs` → 期望包含: `session_token_tracker` 和 `needs_auto_compact` 和 `auto_compact_failures`

#### - [x] 5.5 状态栏包含上下文百分比展示逻辑
- **来源:** spec-plan.md Task 4 / spec-design.md §2.7
- **目的:** 确认 ctx 百分比渲染和颜色分级（绿/黄/红）
- **操作步骤:**
  1. [A] `grep -n "context_usage_percent" rust-agent-tui/src/ui/main_ui/status_bar.rs` → 期望包含: `context_usage_percent`

#### - [x] 5.6 compact 后重置 tracker
- **来源:** spec-plan.md Task 4
- **目的:** 确认 start_compact 时重置 session_token_tracker
- **操作步骤:**
  1. [A] `grep -n "session_token_tracker.reset\|session_token_tracker" rust-agent-tui/src/app/thread_ops.rs` → 期望包含: `reset()`

#### - [x] 5.7 CompactDone 重置失败计数 / CompactError 递增失败计数
- **来源:** spec-plan.md Task 4 / spec-design.md §五（circuit breaker）
- **目的:** 确认 circuit breaker 计数逻辑正确
- **操作步骤:**
  1. [A] `grep -n "auto_compact_failures" rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `auto_compact_failures = 0` 和 `auto_compact_failures += 1`

#### - [x] 5.8 TUI 编译通过
- **来源:** spec-plan.md Task 4
- **目的:** 确认跨 crate 依赖无类型错误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望包含: `Finished`

---

### 场景 6：Micro-Compact 与 Auto-Compact 触发

#### - [x] 6.1 micro_compact 函数已定义
- **来源:** spec-plan.md Task 5
- **目的:** 确认纯函数清除旧工具结果实现存在
- **操作步骤:**
  1. [A] `grep -n "pub fn micro_compact" rust-create-agent/src/agent/token.rs` → 期望包含: `pub fn micro_compact`

#### - [x] 6.2 start_micro_compact 方法已定义
- **来源:** spec-plan.md Task 5
- **目的:** 确认 TUI 层 micro-compact 入口存在
- **操作步骤:**
  1. [A] `grep -n "fn start_micro_compact" rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `fn start_micro_compact`

#### - [x] 6.3 micro_compact 单元测试通过
- **来源:** spec-plan.md Task 5
- **目的:** 验证清除逻辑（长内容替换、短内容保留、keep_recent 边界）
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- agent::token::tests::test_micro_compact 2>&1 | tail -5` → 期望包含: `test result: ok`

#### - [x] 6.4 Auto-compact 两阶段触发逻辑
- **来源:** spec-plan.md Task 4（Done 分支） / spec-design.md §2.5
- **目的:** 确认 Done 事件中集成 auto-compact（>=85% full compact，70-85% micro-compact）
- **操作步骤:**
  1. [A] `grep -n "needs_auto_compact\|start_micro_compact\|start_compact" rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `needs_auto_compact` 和 `start_compact` 和 `start_micro_compact`

---

### 场景 7：全量回归与集成验证

#### - [x] 7.1 全 workspace 测试通过
- **来源:** spec-plan.md Task 6
- **目的:** 确认无回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 7.2 全 workspace 编译通过
- **来源:** spec-plan.md Task 6
- **目的:** 确认跨 crate 依赖完整
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -3` → 期望包含: `Finished`

#### - [x] 7.3 ContextWarning 序列化测试通过
- **来源:** spec-plan.md Task 4
- **目的:** 确认事件可序列化/反序列化
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- agent::events::tests::test_context_warning 2>&1 | tail -5` → 期望包含: `test result: ok`

---

## 验收后清理

无需清理（无后台服务启动）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 核心 crate 编译 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | 基础测试可执行 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | prelude 导出 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | token 模块注册 | 1 | 0 | ✅ |
| 场景 2 | 2.3 | 数据模型单元测试 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | State trait 访问器 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | executor accumulate | 1 | 0 | ✅ |
| 场景 3 | 3.3 | AgentState 测试 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | ReactLLM trait 扩展 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | 模型映射实现 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | 映射测试通过 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | ContextWarning 变体 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | TokenUsageUpdate 事件 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | LlmCallEnd 转发 | 1 | 0 | ✅ |
| 场景 5 | 5.4 | AgentComm 追踪字段 | 1 | 0 | ✅ |
| 场景 5 | 5.5 | 状态栏展示逻辑 | 1 | 0 | ✅ |
| 场景 5 | 5.6 | compact 后重置 | 1 | 0 | ✅ |
| 场景 5 | 5.7 | circuit breaker 计数 | 1 | 0 | ✅ |
| 场景 5 | 5.8 | TUI 编译通过 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | micro_compact 函数 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | start_micro_compact | 1 | 0 | ✅ |
| 场景 6 | 6.3 | micro_compact 测试 | 1 | 0 | ✅ |
| 场景 6 | 6.4 | 两阶段触发逻辑 | 1 | 0 | ✅ |
| 场景 7 | 7.1 | 全 workspace 测试 | 1 | 0 | ✅ |
| 场景 7 | 7.2 | 全 workspace 编译 | 1 | 0 | ✅ |
| 场景 7 | 7.3 | ContextWarning 序列化 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
