# legacy-cleanup 执行计划

**目标:** 删除已被 UserInteractionBroker 替代的旧交互 trait（HitlHandler、AskUserInvoker）及其重导出

**技术栈:** Rust

**设计文档:** spec-design.md

---

### Task 1: 删除 HitlHandler trait

**涉及文件:**
- 修改: `rust-create-agent/src/hitl/mod.rs`

**执行步骤:**
- [x] 删除 `HitlHandler` trait 定义（第 27-53 行），包括 `// ─── HitlHandler ──` 注释、`#[async_trait]`、trait 块
  - 保留 `HitlDecision` 枚举（第 3-16 行）和 `BatchItem` 结构体（第 18-25 行）
  - 删除顶部的 `use async_trait::async_trait;`——该模块中不再有 async_trait 消费者

**检查步骤:**
- [x] 验证 HitlHandler 定义已删除
  - `grep -n "HitlHandler" rust-create-agent/src/hitl/mod.rs`
  - 预期: 返回 0 结果
- [x] 验证 HitlDecision 和 BatchItem 仍存在
  - `grep -n "HitlDecision\|BatchItem" rust-create-agent/src/hitl/mod.rs`
  - 预期: 各返回匹配行

---

### Task 2: 删除 AskUserInvoker trait 及 core prelude 清理

**涉及文件:**
- 修改: `rust-create-agent/src/ask_user/mod.rs`
- 修改: `rust-create-agent/src/lib.rs`

**执行步骤:**
- [x] 删除 `AskUserInvoker` trait 定义（第 41-53 行），包括 `// ─── AskUserInvoker ──` 注释、`#[async_trait]`、trait 块
  - 保留 `AskUserOption`、`AskUserQuestionData`、`AskUserBatchRequest` 数据类型
  - 删除顶部的 `use async_trait::async_trait;`——该模块中不再有 async_trait 消费者
- [x] 在 `rust-create-agent/src/lib.rs` prelude 中移除 `HitlHandler` 和 `AskUserInvoker`
  - 第 27 行: `AskUserBatchRequest, AskUserInvoker, AskUserOption, AskUserQuestionData` → 移除 `AskUserInvoker`
  - 第 30 行: `BatchItem, HitlDecision, HitlHandler` → 移除 `HitlHandler`

**检查步骤:**
- [x] 验证 AskUserInvoker 定义已删除
  - `grep -n "AskUserInvoker" rust-create-agent/src/ask_user/mod.rs`
  - 预期: 返回 0 结果
- [x] 验证 core prelude 不再导出旧 trait
  - `grep -n "HitlHandler\|AskUserInvoker" rust-create-agent/src/lib.rs`
  - 预期: 返回 0 结果
- [x] 验证数据类型仍存在
  - `grep -n "AskUserOption\|AskUserQuestionData\|AskUserBatchRequest" rust-create-agent/src/ask_user/mod.rs`
  - 预期: 各返回匹配行
- [x] 编译验证
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 编译成功，无 error

---

### Task 3: 清理 middlewares 重导出

**涉及文件:**
- 修改: `rust-agent-middlewares/src/hitl/mod.rs`
- 修改: `rust-agent-middlewares/src/ask_user/mod.rs`
- 修改: `rust-agent-middlewares/src/lib.rs`

**执行步骤:**
- [x] 在 `rust-agent-middlewares/src/hitl/mod.rs` 中移除旧类型重导出
  - 删除第 12-14 行（注释 + `#[allow(deprecated)]` + `pub use ... HitlHandler`）
  - 保留 `BatchItem, HitlDecision`：改为 `pub use rust_create_agent::hitl::{BatchItem, HitlDecision};`（无 deprecated）
- [x] 在 `rust-agent-middlewares/src/ask_user/mod.rs` 中移除 AskUserInvoker/AskUserHandler
  - 第 5-7 行: 移除 `AskUserInvoker`，只保留数据类型：`pub use rust_create_agent::ask_user::{AskUserBatchRequest, AskUserOption, AskUserQuestionData};`
  - 删除第 9-12 行（注释 + `pub use ... AskUserInvoker as AskUserHandler;`）
- [x] 在 `rust-agent-middlewares/src/lib.rs` 中清理重导出
  - 第 33-36 行: 移除 `HitlHandler`，改为 `pub use hitl::{default_requires_approval, is_yolo_mode, BatchItem, HitlDecision, HumanInTheLoopMiddleware};`
  - 删除第 37-39 行（注释 + `AskUserInvoker` + `AskUserHandler` 的重导出）
  - 第 49 行 prelude: 移除 `AskUserHandler`，改为 `ask_user_tool_definition, parse_ask_user, AskUserBatchRequest, AskUserOption, AskUserQuestionData,`
  - 第 52-55 行 prelude: 移除 `HitlHandler`，改为 `default_requires_approval, is_yolo_mode, BatchItem, HitlDecision, HumanInTheLoopMiddleware,`

**检查步骤:**
- [x] 验证 middlewares 不再引用旧 trait
  - `grep -rn "HitlHandler\|AskUserInvoker\|AskUserHandler" --include="*.rs" rust-agent-middlewares/src/`
  - 预期: 返回 0 结果
- [x] 编译验证无 warning
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -i "warn\|error" | head -10`
  - 预期: 无 warning 和 error

---

### Task 4: legacy-cleanup Acceptance

**Prerequisites:**
- Start command: `cargo build --workspace`
- 无需额外测试数据

**End-to-end verification:**

1. 全 workspace 编译通过
   - `cargo build --workspace 2>&1 | tail -5`
   - Expected: 编译成功，无 error ✓

2. 全量测试通过
   - `cargo test --workspace 2>&1 | tail -20`
   - Expected: 所有测试通过，无 failure ✓

3. HitlHandler 完全清除
   - `grep -rn "HitlHandler" --include="*.rs" .`
   - Expected: 仅剩 3 处 TUI 注释历史说明 ✓

4. AskUserInvoker 完全清除
   - `grep -rn "AskUserInvoker" --include="*.rs" .`
   - Expected: 返回 0 结果 ✓

5. AskUserHandler 完全清除
   - `grep -rn "AskUserHandler" --include="*.rs" .`
   - Expected: 仅剩 TUI 注释历史说明 ✓

6. 数据类型保留验证
   - `grep -rn "HitlDecision\|BatchItem\|AskUserOption\|AskUserQuestionData\|AskUserBatchRequest" --include="*.rs" . | wc -l`
   - Expected: 大于 0 → 73 处引用 ✓

7. 编译无 deprecated warning
   - `cargo build --workspace 2>&1 | grep -i "deprecated" | head -5`
   - Expected: 无 deprecated warning ✓
