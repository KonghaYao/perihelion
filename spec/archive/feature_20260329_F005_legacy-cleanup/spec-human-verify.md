# [legacy-cleanup] 人工验收清单

**生成时间:** 2026-03-29 22:00
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version && cargo --version`
- [ ] [AUTO] 检查工作区 Cargo.toml 存在: `test -f Cargo.toml`

### 测试数据准备
- 无需额外测试数据（纯代码删除任务）

---

## 验收项目

### 场景 1：旧 trait 完全删除验证

#### - [x] 1.1 HitlHandler trait 定义已从核心库删除
- **来源:** Task 1 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `grep -c "trait HitlHandler" rust-create-agent/src/hitl/mod.rs` → 期望: 输出 `0`
  2. [A] `grep -c "HitlHandler" rust-create-agent/src/hitl/mod.rs` → 期望: 输出 `0`
- **异常排查:**
  - 如果 grep 返回 >0: 检查该行是 trait 定义还是注释/文档引用，若为 trait 定义则未完成删除

#### - [x] 1.2 AskUserInvoker trait 定义已从核心库删除
- **来源:** Task 2 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `grep -c "trait AskUserInvoker" rust-create-agent/src/ask_user/mod.rs` → 期望: 输出 `0`
  2. [A] `grep -c "AskUserInvoker" rust-create-agent/src/ask_user/mod.rs` → 期望: 输出 `0`
- **异常排查:**
  - 如果 grep 返回 >0: 确认是否为 trait 定义（需删除）或注释引用（可保留）

#### - [x] 1.3 核心库 prelude 不再导出旧 trait
- **来源:** Task 2 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `grep -c "HitlHandler\|AskUserInvoker" rust-create-agent/src/lib.rs` → 期望: 输出 `0`
- **异常排查:**
  - 如果返回 >0: 检查 prelude `pub use` 语句中是否残留旧 trait 名称

#### - [x] 1.4 middlewares 不再重导出旧 trait
- **来源:** Task 3 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `grep -rn "HitlHandler\|AskUserInvoker\|AskUserHandler" --include="*.rs" rust-agent-middlewares/src/` → 期望: 返回 0 结果（空输出）
- **异常排查:**
  - 如果有输出: 检查对应文件和行号，确认是重导出（需删除）还是注释引用

#### - [x] 1.5 async_trait 导入已清理（核心库模块无残留消费者）
- **来源:** Task 1 & Task 2 执行步骤
- **操作步骤:**
  1. [A] `grep -c "use async_trait" rust-create-agent/src/hitl/mod.rs` → 期望: 输出 `0`
  2. [A] `grep -c "use async_trait" rust-create-agent/src/ask_user/mod.rs` → 期望: 输出 `0`
- **异常排查:**
  - 如果 >0: 该模块中可能还有其他 async_trait 消费者，检查后决定是否可安全删除

### 场景 2：数据类型保留验证

#### - [x] 2.1 HitlDecision 和 BatchItem 在核心库中保留
- **来源:** Task 1 检查步骤 / spec-design.md 保留类型清单
- **操作步骤:**
  1. [A] `grep -c "HitlDecision\|BatchItem" rust-create-agent/src/hitl/mod.rs` → 期望: 输出 >= `2`（两者各有定义行）
  2. [A] `grep -c "HitlDecision\|BatchItem" rust-create-agent/src/lib.rs` → 期望: 输出 >= `1`（prelude 重导出）
- **异常排查:**
  - 如果 =0: 数据类型被误删，需恢复

#### - [x] 2.2 AskUser 数据类型在核心库中保留
- **来源:** Task 2 检查步骤 / spec-design.md 保留类型清单
- **操作步骤:**
  1. [A] `grep -c "AskUserOption\|AskUserQuestionData\|AskUserBatchRequest" rust-create-agent/src/ask_user/mod.rs` → 期望: 输出 >= `3`（每个类型各有定义）
  2. [A] `grep -c "AskUserOption\|AskUserQuestionData\|AskUserBatchRequest" rust-create-agent/src/lib.rs` → 期望: 输出 >= `1`（prelude 重导出）
- **异常排查:**
  - 如果 =0: 数据类型被误删，需恢复

#### - [x] 2.3 数据类型在 middlewares 中正确重导出（无 deprecated 标记）
- **来源:** Task 3 执行步骤 / spec-design.md
- **操作步骤:**
  1. [A] `grep "pub use.*BatchItem\|pub use.*HitlDecision" rust-agent-middlewares/src/hitl/mod.rs` → 期望: 输出包含两个类型，且行首无 `#[allow(deprecated)]`
  2. [A] `grep "pub use.*AskUser" rust-agent-middlewares/src/ask_user/mod.rs` → 期望: 包含 `AskUserBatchRequest, AskUserOption, AskUserQuestionData`，不包含 `AskUserInvoker`
  3. [A] `grep -c "deprecated" rust-agent-middlewares/src/hitl/mod.rs` → 期望: 输出 `0`
- **异常排查:**
  - 如果仍有 deprecated 标记: 删除对应的 `#[allow(deprecated)]` 行

### 场景 3：全工作区编译与测试

#### - [x] 3.1 全工作区编译通过（无 error）
- **来源:** Task 4 端到端验证第 1 项
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | tail -5` → 期望: 输出包含 `Finished` 且无 `error`
- **异常排查:**
  - 如果有编译错误: 检查错误信息中引用的类型/模块，说明旧 trait 的某些使用点未被清理

#### - [x] 3.2 全量测试通过（无 failure）
- **来源:** Task 4 端到端验证第 2 项
- **操作步骤:**
  1. [A] `cargo test --workspace 2>&1 | tail -20` → 期望: 输出包含 `test result:` 且各 crate 显示 0 failures
- **异常排查:**
  - 如果有测试失败: 检查失败测试是否引用了旧 trait，需更新测试代码

#### - [x] 3.3 编译无 deprecated warning
- **来源:** Task 4 端到端验证第 7 项
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | grep -i "deprecated" | head -5` → 期望: 空输出（无 deprecated warning）
- **异常排查:**
  - 如果有 deprecated warning: 检查来源文件，确认是否有残留的 `#[deprecated]` 标记或旧类型使用

### 场景 4：全局残留扫描

#### - [x] 4.1 HitlHandler 在整个 Rust 代码库中零实现引用
- **来源:** Task 4 端到端验证第 3 项
- **操作步骤:**
  1. [A] `grep -rn "HitlHandler" --include="*.rs" . | grep -v "//.*HitlHandler"` → 期望: 空输出（非注释行无匹配）
  2. [A] `grep -rn "HitlHandler" --include="*.rs" . | grep "//.*HitlHandler"` → 期望: 仅 TUI 代码注释中的历史说明（如 "旧 HitlHandler"）
- **异常排查:**
  - 如果非注释行有匹配: 该位置仍有旧 trait 引用，需清理

#### - [x] 4.2 AskUserInvoker 在整个 Rust 代码库中零匹配
- **来源:** Task 4 端到端验证第 4 项
- **操作步骤:**
  1. [A] `grep -rn "AskUserInvoker" --include="*.rs" .` → 期望: 空输出（完全零匹配）
- **异常排查:**
  - 如果有输出: 检查并清理残留引用

#### - [x] 4.3 AskUserHandler 在整个 Rust 代码库中零实现引用
- **来源:** Task 4 端到端验证第 5 项
- **操作步骤:**
  1. [A] `grep -rn "AskUserHandler" --include="*.rs" . | grep -v "//.*AskUserHandler"` → 期望: 空输出（非注释行无匹配）
- **异常排查:**
  - 如果非注释行有匹配: 清理残留的别名引用或使用

#### - [x] 4.4 数据类型全局引用计数正常
- **来源:** Task 4 端到端验证第 6 项
- **操作步骤:**
  1. [A] `grep -rn "HitlDecision\|BatchItem\|AskUserOption\|AskUserQuestionData\|AskUserBatchRequest" --include="*.rs" . | wc -l` → 期望: 输出 > `0`（数据类型仍被广泛引用）
- **异常排查:**
  - 如果 =0: 数据类型被误删，需恢复所有定义和导出

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | HitlHandler trait 已删除 | 2 | 0 | ✅ | |
| 场景 1 | 1.2 | AskUserInvoker trait 已删除 | 2 | 0 | ✅ | |
| 场景 1 | 1.3 | 核心 prelude 不导出旧 trait | 1 | 0 | ✅ | |
| 场景 1 | 1.4 | middlewares 不重导出旧 trait | 1 | 0 | ✅ | |
| 场景 1 | 1.5 | async_trait 导入已清理 | 2 | 0 | ✅ | |
| 场景 2 | 2.1 | HitlDecision/BatchItem 保留 | 2 | 0 | ✅ | |
| 场景 2 | 2.2 | AskUser 数据类型保留 | 2 | 0 | ✅ | |
| 场景 2 | 2.3 | middlewares 重导出无 deprecated | 3 | 0 | ✅ | |
| 场景 3 | 3.1 | 全工作区编译通过 | 1 | 0 | ✅ | |
| 场景 3 | 3.2 | 全量测试通过 | 1 | 0 | ✅ | |
| 场景 3 | 3.3 | 无 deprecated warning | 1 | 0 | ✅ | |
| 场景 4 | 4.1 | HitlHandler 全局零引用 | 2 | 0 | ✅ | |
| 场景 4 | 4.2 | AskUserInvoker 全局零匹配 | 1 | 0 | ✅ | |
| 场景 4 | 4.3 | AskUserHandler 全局零引用 | 1 | 0 | ✅ | |
| 场景 4 | 4.4 | 数据类型全局引用计数正常 | 1 | 0 | ✅ | |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
