# LLM 重试机制 人工验收清单

**生成时间:** 2026-04-28
**关联计划:** spec/feature_20260428_F001_llm-retry/spec-plan.md
**关联设计:** spec/feature_20260428_F001_llm-retry/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译 workspace: `cargo build --workspace 2>&1 | tail -5`

---

## 验收项目

### 场景 1: 基础构建与编译

#### - [x] 1.1 Workspace 编译通过
- **来源:** spec-plan.md Task 0/6 / spec-design.md §验收标准
- **目的:** 确认全部改动编译无误
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | grep -E "error|Finished"` → 期望包含: `Finished`

#### - [x] 1.2 Executor 零改动验证
- **来源:** spec-plan.md Task 6 步骤 6 / spec-design.md §验收标准
- **目的:** 确认重试机制未侵入 executor
- **操作步骤:**
  1. [A] `git diff rust-create-agent/src/agent/executor.rs` → 期望精确: ``（空输出）

---

### 场景 2: 错误分类正确性

#### - [x] 2.1 is_retryable() 单元测试通过
- **来源:** spec-plan.md Task 1 / spec-design.md §错误类型改造
- **目的:** 确认 429/5xx/408 可重试，400/401/404 不可重试
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- error::tests 2>&1 | grep "test result"` → 期望包含: `test result: ok`

#### - [x] 2.2 OpenAI 适配器使用 LlmHttpError
- **来源:** spec-plan.md Task 3 / spec-design.md §LLM 实现层改造
- **目的:** 确认 HTTP 错误携带 status code
- **操作步骤:**
  1. [A] `grep -n "LlmHttpError" rust-create-agent/src/llm/openai.rs` → 期望包含: `status.as_u16()`

#### - [x] 2.3 Anthropic 适配器使用 LlmHttpError
- **来源:** spec-plan.md Task 3 / spec-design.md §LLM 实现层改造
- **目的:** 确认 HTTP 错误携带 status code
- **操作步骤:**
  1. [A] `grep -n "LlmHttpError" rust-create-agent/src/llm/anthropic.rs` → 期望包含: `status.as_u16()`

---

### 场景 3: 重试核心逻辑

#### - [x] 3.1 retry 模块正确导出
- **来源:** spec-plan.md Task 4
- **目的:** 确认 retry 模块和类型对外可见
- **操作步骤:**
  1. [A] `grep -n "pub mod retry\|pub use retry" rust-create-agent/src/llm/mod.rs` → 期望包含: `pub mod retry` → 期望包含: `pub use retry`

#### - [x] 3.2 主 Agent 和 SubAgent 组装点已包装 RetryableLLM
- **来源:** spec-plan.md Task 4 / spec-design.md §RetryableLLM 包装器
- **目的:** 确认组装点接入重试装饰器
- **操作步骤:**
  1. [A] `grep -c "RetryableLLM" rust-agent-tui/src/app/agent.rs` → 期望精确: `3`

#### - [x] 3.3 重试逻辑单元测试通过
- **来源:** spec-plan.md Task 4 / spec-design.md §验收标准
- **目的:** 确认可重试错误触发重试、不可重试立即返回、耗尽返回最后错误、退避延迟范围正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- llm::retry::tests 2>&1 | grep "test result"` → 期望包含: `test result: ok`

---

### 场景 4: 事件序列化与映射

#### - [x] 4.1 LlmRetrying 事件序列化测试通过
- **来源:** spec-plan.md Task 2 / spec-design.md §事件扩展
- **目的:** 确认新增事件正确序列化/反序列化
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- events::tests 2>&1 | grep "test result"` → 期望包含: `test result: ok`

#### - [x] 4.2 事件映射 grep 确认
- **来源:** spec-plan.md Task 2 / spec-design.md §事件扩展
- **目的:** 确认 Core→TUI 映射覆盖 LlmRetrying
- **操作步骤:**
  1. [A] `grep -n "LlmRetrying" rust-agent-tui/src/app/agent.rs` → 期望包含: `ExecutorEvent::LlmRetrying`

---

### 场景 5: TUI 集成显示

#### - [x] 5.1 RetryStatus 结构体与字段已添加
- **来源:** spec-plan.md Task 5 / spec-design.md §TUI 集成
- **目的:** 确认 App 状态持有重试信息
- **操作步骤:**
  1. [A] `grep -c "retry_status" rust-agent-tui/src/app/agent_comm.rs` → 期望精确: `3`（结构体定义 + 字段声明 + 初始化）

#### - [x] 5.2 handle_agent_event 包含 LlmRetrying 分支
- **来源:** spec-plan.md Task 5 / spec-design.md §TUI 集成
- **目的:** 确认 TUI 处理重试事件
- **操作步骤:**
  1. [A] `grep -n "LlmRetrying" rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `retry_status`

#### - [x] 5.3 Status bar 包含重试渲染逻辑
- **来源:** spec-plan.md Task 5 / spec-design.md §TUI 集成
- **目的:** 确认状态栏渲染重试状态
- **操作步骤:**
  1. [A] `grep -n "retry_status" rust-agent-tui/src/ui/main_ui/status_bar.rs` → 期望包含: `retry`

#### - [x] 5.4 Headless 测试验证重试状态显示
- **来源:** spec-plan.md Task 5 / spec-design.md §验收标准
- **目的:** 确认状态栏正确显示重试次数
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- headless::tests::test_retry_status 2>&1 | grep "test result"` → 期望包含: `test result: ok`

---

### 场景 6: 端到端回归

#### - [x] 6.1 全 workspace 测试套件通过
- **来源:** spec-plan.md Task 6 / spec-design.md §验收标准
- **目的:** 确认无回归
- **操作步骤:**
  1. [A] `cargo test --workspace 2>&1 | grep "test result"` → 期望包含: `test result: ok`（所有 crate 行均 ok）

---

## 验收后清理

无需清理（无后台服务启动）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Workspace 编译通过 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | Executor 零改动验证 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | is_retryable() 单元测试 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | OpenAI LlmHttpError | 1 | 0 | ✅ |
| 场景 2 | 2.3 | Anthropic LlmHttpError | 1 | 0 | ✅ |
| 场景 3 | 3.1 | retry 模块导出 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | 组装点包装 | 1 | 0 | ✅ |
| 场景 3 | 3.3 | 重试单元测试 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | 事件序列化测试 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | 事件映射确认 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | RetryStatus 结构体 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | 事件处理分支 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | Status bar 渲染 | 1 | 0 | ✅ |
| 场景 5 | 5.4 | Headless 测试 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | 全量回归测试 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过
