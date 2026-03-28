# M3: 消除 PrependSystemMiddleware 排序约束 人工验收清单

**生成时间:** 2026-03-28 10:00
**关联计划:** spec/feature_20260327_M3_system-prompt/spec-plan.md
**关联设计:** spec/feature_20260327_M3_system-prompt/spec-design.md

> 所有验收项均可自动化验证，无需人类参与。本清单用于 sdd-start-human-verify 自动执行。

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [x] [AUTO] 全量构建（确认无编译错误）: `cargo build 2>&1 | grep -E "^error"` → 期望: 无输出

> **执行备注（2026-03-28）:** 准备阶段发现并修复了预存在 bug：`rust-create-agent/src/agent/state.rs` 引用私有模块路径（`crate::thread::store::ThreadStore`、`crate::thread::types::ThreadId`），已改为公开 re-export 路径（`crate::thread::ThreadStore`、`crate::thread::ThreadId`）。

### 测试数据准备

无需额外测试数据，所有验证通过代码检查和单元测试完成。

---

## 验收项目

### 场景 1：核心框架修改验证（rust-create-agent executor.rs）

#### - [x] 1.1 with_system_prompt API 已正确实现

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "system_prompt" rust-create-agent/src/agent/executor.rs` → 期望: 找到至少 1 处字段定义（含 `system_prompt: Option<String>`）
  2. [A] `grep -n "with_system_prompt" rust-create-agent/src/agent/executor.rs` → 期望: 找到至少 2 处（字段赋值 + builder 方法定义）
  3. [A] `grep -n "prepend_message" rust-create-agent/src/agent/executor.rs` → 期望: 找到 1 处（execute() 中的 prepend 逻辑，位于 run_before_agent 调用之后）
- **异常排查:**
  - 如果 grep 无输出: 检查 Task 1 是否已完成，确认修改目标文件为 `rust-create-agent/src/agent/executor.rs`

#### - [x] 1.2 核心库编译无报错

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-create-agent 2>&1 | grep -E "^error"` → 期望: 无输出（无编译错误）
- **异常排查:**
  - 如果出现编译错误: 查看完整错误信息 `cargo build -p rust-create-agent`，根据错误位置检查 executor.rs 的修改

#### - [x] 1.3 核心库单元测试全部通过

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib 2>&1 | tail -5` → 期望: 末尾出现 `test result: ok. N passed; 0 failed`
- **异常排查:**
  - 如果测试失败: 运行 `cargo test -p rust-create-agent --lib 2>&1 | grep FAILED` 查看具体失败用例

---

### 场景 2：TUI 迁移验证（rust-agent-tui agent.rs）

#### - [x] 2.1 旧 PrependSystemMiddleware 调用已从 agent.rs 删除

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "PrependSystemMiddleware" rust-agent-tui/src/app/agent.rs` → 期望: 无输出（已完全移除）
- **异常排查:**
  - 如果仍有输出: 手动删除 `rust-agent-tui/src/app/agent.rs` 中对应的 `.add_middleware(Box::new(PrependSystemMiddleware::new(system_prompt)))` 行及相关 import

#### - [x] 2.2 新 with_system_prompt 调用已存在于 agent.rs

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "with_system_prompt" rust-agent-tui/src/app/agent.rs` → 期望: 找到 1 处
- **异常排查:**
  - 如果无输出: 在 `rust-agent-tui/src/app/agent.rs` 的 ReActAgent builder 链中添加 `.with_system_prompt(system_prompt)`

#### - [x] 2.3 TUI 编译无报错

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"` → 期望: 无输出
- **异常排查:**
  - 如果出现编译错误: 查看完整输出 `cargo build -p rust-agent-tui`，通常原因为 import 未清理或 builder 方法名拼写错误

---

### 场景 3：SubAgent 迁移验证（rust-agent-middlewares subagent/tool.rs）

#### - [x] 3.1 PrependSystemMiddleware 已从 subagent/tool.rs 移除

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -n "PrependSystemMiddleware" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 无输出（import 和调用均已删除）
  2. [A] `grep -n "with_system_prompt" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 找到至少 1 处（新的调用方式）
- **异常排查:**
  - 如果 PrependSystemMiddleware 仍存在: 检查 Task 3 执行步骤，确认替换了 invoke 方法中的相关代码块并删除了顶部 import

#### - [x] 3.2 中间件库编译无报错

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"` → 期望: 无输出
- **异常排查:**
  - 如果编译失败: 查看 `cargo build -p rust-agent-middlewares` 完整输出定位错误

#### - [x] 3.3 SubAgent 相关测试全部通过

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib subagent 2>&1 | tail -10` → 期望: 末尾出现 `test result: ok. N passed; 0 failed`（含 `test_system_builder_injects_system_message`）
- **异常排查:**
  - 如果 `test_system_builder_injects_system_message` 失败: 检查 Task 3，确认 `with_system_prompt` 调用逻辑与原 `PrependSystemMiddleware` 语义等价

---

### 场景 4：PrependSystemMiddleware 废弃标注验证

#### - [x] 4.1 deprecated 属性已添加到 PrependSystemMiddleware 结构体

- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -n "deprecated" rust-agent-middlewares/src/middleware/prepend_system.rs` → 期望: 找到包含 `#[deprecated` 的行
- **异常排查:**
  - 如果无输出: 在 `pub struct PrependSystemMiddleware` 定义上方添加 `#[deprecated(since = "0.2.0", note = "改用 ReActAgent::with_system_prompt()")]`

#### - [x] 4.2 全量构建出现废弃警告且无新增测试失败

- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -i deprecated` → 期望: 出现 `deprecated` 相关警告（标注已生效）
  2. [A] `cargo test -p rust-create-agent -p rust-agent-middlewares 2>&1 | grep -E "FAILED|test result"` → 期望: 所有行均为 `test result: ok`，无 `FAILED`
- **异常排查:**
  - 如果无 deprecated 警告: 检查 Task 4，确认 `#[deprecated]` 属性位置正确（struct 定义上方）
  - 如果有 FAILED: 运行 `cargo test -p rust-create-agent -p rust-agent-middlewares 2>&1 | grep -B5 FAILED` 查看详情

> **执行备注（2026-03-28）:** step 1（deprecated 警告）未触发，原因是所有调用方已完全迁移到 `with_system_prompt()`，`PrependSystemMiddleware` 无任何使用处，Rust 只在有调用者时才生成 deprecated 警告。`#[deprecated]` 属性本身已存在（L21），迁移完全成功，此为预期行为。step 2 全量测试通过（134 个，0 失败）。

---

### 场景 5：端到端功能验证

#### - [x] 5.1 system prompt 位于消息列表最前（核心功能测试）

- **来源:** Task 5 端到端验证
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_system_prompt_is_first 2>&1 | tail -5` → 期望: `test result: ok. 1 passed; 0 failed`
- **异常排查:**
  - 如果测试不存在: 确认 `test_system_prompt_is_first` 已写入 `executor.rs` 的 `#[cfg(test)]` 块
  - 如果测试失败: 检查 Task 1，确认 prepend 位置在 `run_before_agent` **之后**

#### - [x] 5.2 中间件注册顺序无关性验证

- **来源:** Task 5 端到端验证
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- test_system_prompt_order_independent 2>&1 | tail -5` → 期望: `test result: ok. 1 passed; 0 failed`
- **异常排查:**
  - 如果测试失败: system prompt 可能仍受中间件顺序影响，重新检查 `execute()` 中 prepend 的插入位置

#### - [x] 5.3 SubAgent 系统提示词注入回归验证 + 全量测试无回归

- **来源:** Task 5 端到端验证
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_system_builder_injects_system_message 2>&1 | tail -5` → 期望: `test result: ok. 1 passed; 0 failed`
  2. [A] `cargo test -p rust-create-agent -p rust-agent-middlewares -p rust-agent-tui 2>&1 | grep -E "FAILED|test result"` → 期望: 所有行均为 `test result: ok`，无 `FAILED`
- **异常排查:**
  - 如果 SubAgent 测试失败: 检查 Task 3 中 `with_system_prompt` 替换逻辑
  - 如果全量测试有 FAILED: 根据失败的 crate 对应检查 Task 1-4

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | with_system_prompt API 已正确实现 | 3 | 0 | ✅ | |
| 场景 1 | 1.2 | 核心库编译无报错 | 1 | 0 | ✅ | |
| 场景 1 | 1.3 | 核心库单元测试全部通过 | 1 | 0 | ✅ | 50 个测试通过 |
| 场景 2 | 2.1 | 旧 PrependSystemMiddleware 调用已删除 | 1 | 0 | ✅ | |
| 场景 2 | 2.2 | 新 with_system_prompt 调用已存在 | 1 | 0 | ✅ | |
| 场景 2 | 2.3 | TUI 编译无报错 | 1 | 0 | ✅ | |
| 场景 3 | 3.1 | PrependSystemMiddleware 已替换为 with_system_prompt | 2 | 0 | ✅ | |
| 场景 3 | 3.2 | 中间件库编译无报错 | 1 | 0 | ✅ | |
| 场景 3 | 3.3 | SubAgent 相关测试全部通过 | 1 | 0 | ✅ | 含 test_system_builder_injects_system_message |
| 场景 4 | 4.1 | deprecated 属性已添加到结构体定义 | 1 | 0 | ✅ | |
| 场景 4 | 4.2 | 全量构建出现废弃警告且无新增失败 | 2 | 0 | ✅ | deprecated 警告因无调用者未触发，属预期行为 |
| 场景 5 | 5.1 | system prompt 位于消息列表最前 | 1 | 0 | ✅ | |
| 场景 5 | 5.2 | 中间件注册顺序无关性验证 | 1 | 0 | ✅ | |
| 场景 5 | 5.3 | SubAgent 回归 + 全量测试无回归 | 2 | 0 | ✅ | 211 个测试全部通过 |

**验收结论:** ✅ 全部通过
