# Feature: 20260329_F005 - legacy-cleanup

## 需求背景

架构审查发现 `HitlHandler` trait 和 `AskUserInvoker` trait 是旧的交互接口，已被 `UserInteractionBroker` 统一替代，但仍保留在代码中：

- `rust-create-agent/src/hitl/mod.rs` 导出 `HitlHandler` trait + `BatchItem` + `HitlDecision` 类型
- `rust-create-agent/src/ask_user/mod.rs` 导出 `AskUserInvoker` trait
- `rust-agent-middlewares/src/lib.rs` 重导出这些类型"保留向后兼容"
- `rust-agent-middlewares/src/hitl/mod.rs` 标注 `#[allow(deprecated)]` 重导出旧类型

实际搜索后确认：所有业务代码已使用 `UserInteractionBroker`，这些旧类型零引用（仅测试代码中有少数使用）。

## 目标

- 删除 `HitlHandler` trait（`rust-create-agent/src/hitl/mod.rs` 中的 trait 定义）
- 删除 `AskUserInvoker` trait（`rust-create-agent/src/ask_user/mod.rs` 中的 trait 定义）
- 清理 middlewares 的重导出（`lib.rs` 中的 `#[allow(deprecated)]` 行）
- 保留数据类型：`HitlDecision`、`BatchItem`、`AskUserOption`、`AskUserQuestionData`、`AskUserBatchRequest` 仍被使用，不删除
- 修复引用这些旧 trait 的测试代码

## 方案设计

### 要删除的类型

| 类型 | 位置 | 替代 |
|------|------|------|
| `HitlHandler` trait | `core::hitl::HitlHandler` | `UserInteractionBroker` |
| `AskUserInvoker` trait | `core::ask_user::AskUserInvoker` | `UserInteractionBroker` |
| `AskUserHandler` 别名 | `middlewares::lib` 重导出 | 删除 |

### 要保留的类型（仍被使用）

- `HitlDecision`（枚举，HITL 中间件映射决策）
- `BatchItem`（数据结构，interaction 模块使用）
- `AskUserOption` / `AskUserQuestionData` / `AskUserBatchRequest`（数据结构，AskUserTool 使用）

### 改动文件清单

1. **`rust-create-agent/src/hitl/mod.rs`**
   - 删除 `HitlHandler` trait 定义（第 31-53 行）
   - 保留 `HitlDecision`、`BatchItem` 数据类型

2. **`rust-create-agent/src/ask_user/mod.rs`**
   - 删除 `AskUserInvoker` trait 定义（第 49-53 行）
   - 保留 `AskUserOption`、`AskUserQuestionData`、`AskUserBatchRequest`

3. **`rust-create-agent/src/lib.rs`**
   - 从 prelude 中移除 `HitlHandler`

4. **`rust-agent-middlewares/src/hitl/mod.rs`**
   - 移除 `#[allow(deprecated)]` 行（第 13-14 行）
   - 移除 `use rust_create_agent::hitl::{BatchItem, HitlDecision, HitlHandler};` 中的 `HitlHandler`

5. **`rust-agent-middlewares/src/lib.rs`**
   - 移除 `pub use rust_create_agent::ask_user::AskUserInvoker;`（第 38 行）
   - 移除 `pub use rust_create_agent::ask_user::AskUserInvoker as AskUserHandler;`（第 39 行）
   - 移除 `#[allow(deprecated)]` 标记

6. **`rust-agent-middlewares/src/lib.rs` prelude**
   - 移除 `AskUserHandler` 重导出

7. **测试代码修复**
   - 搜索所有使用 `HitlHandler` 和 `AskUserInvoker` 的测试，改为使用 `UserInteractionBroker` 的 mock 实现

## 实现要点

- 改动纯删除性质，无新增逻辑
- 数据类型（`HitlDecision` 等）保留在 `interaction/mod.rs` 的 `ApprovalDecision` / `ApprovalItem` 已能替代，但 `HitlDecision` 在 middlewares 的 `apply_decision()` 中仍被直接使用。两阶段清理：本次只删 trait，数据类型统一可留到后续

## 约束一致性

- 与 `constraints.md` 无冲突（不改变任何接口契约）
- 与 `architecture.md` 一致：简化了 hitl/ask_user 模块

## 验收标准

- [ ] `HitlHandler` trait 从代码库完全删除
- [ ] `AskUserInvoker` trait 从代码库完全删除
- [ ] `AskUserHandler` 别名从 middlewares 删除
- [ ] `HitlDecision`、`BatchItem`、`AskUserOption` 等数据类型保留不变
- [ ] `cargo test` 全量通过
- [ ] `cargo build --workspace` 无 warning
- [ ] `grep -r "HitlHandler" --include="*.rs"` 返回 0 结果
- [ ] `grep -r "AskUserInvoker" --include="*.rs"` 返回 0 结果
