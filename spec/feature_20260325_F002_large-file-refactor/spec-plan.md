# 大文件拆分 执行计划

**目标:** 将 `app/mod.rs`（1931行）和 `ui/main_ui.rs`（1305行）按职责域拆分为多个小文件，不改变任何外部 API

**技术栈:** Rust 2021, ratatui, tokio, cargo

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: app 类型定义迁移

**涉及文件:**
- 新建: `rust-agent-tui/src/app/hitl_prompt.rs`
- 新建: `rust-agent-tui/src/app/ask_user_prompt.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 新建 `hitl_prompt.rs`，将 `PendingAttachment`（原 mod.rs 69-78行）和 `HitlBatchPrompt`（原 mod.rs 80-152行）整块剪切过去
  - 文件顶部补充必要 use：`use tokio::sync::oneshot; use rust_agent_middlewares::prelude::BatchItem; use rust_create_agent::agent::react::AgentInput;` 等（根据实际编译错误按需补充）
  - 类型可见性保持 `pub struct`
- [x] 新建 `ask_user_prompt.rs`，将 `QuestionState`（153-235行）和 `AskUserBatchPrompt`（236-307行）整块剪切过去
  - 文件顶部补充 use：`use rust_agent_middlewares::ask_user::{AskUserBatchRequest, AskUserQuestionData}; use tokio::sync::oneshot;`
- [x] 在 `mod.rs` 顶部已有 mod 声明区域添加：
  ```rust
  mod hitl_prompt;
  mod ask_user_prompt;
  ```
- [x] 在 `mod.rs` 中原来的类型定义位置替换为重导出：
  ```rust
  pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
  pub use ask_user_prompt::{QuestionState, AskUserBatchPrompt};
  ```
- [x] 执行 `cargo build -p rust-agent-tui` 修复所有编译错误（通常是 use 路径缺失）

**检查步骤:**
- [x] 编译成功无错误
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 `error[`
- [x] `PendingAttachment` 仍可从 `crate::app` 路径访问（event.rs 已在用）
  - `grep -n "PendingAttachment" rust-agent-tui/src/event.rs`
  - 预期: 找到引用行，且构建通过证明路径有效
- [x] hitl_prompt.rs 行数在预期范围内
  - `wc -l rust-agent-tui/src/app/hitl_prompt.rs`
  - 预期: ≤ 120 行
- [x] ask_user_prompt.rs 行数在预期范围内
  - `wc -l rust-agent-tui/src/app/ask_user_prompt.rs`
  - 预期: ≤ 170 行

---

### Task 2: app HITL & AskUser 操作方法

**涉及文件:**
- 新建: `rust-agent-tui/src/app/hitl_ops.rs`
- 新建: `rust-agent-tui/src/app/ask_user_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 新建 `hitl_ops.rs`，文件头部写 `use super::*;`，然后创建 `impl App { }` 块
  - 将原 mod.rs 中 `hitl_move`（1373行）到 `hitl_confirm`（1433行）的 6 个方法整块剪切进去
  - 包含私有辅助方法 `send_hitl_resolved`
- [x] 新建 `ask_user_ops.rs`，同样以 `use super::*; impl App { }` 结构
  - 将 `ask_user_next_tab`（1435行）到 `ask_user_confirm`（1509行）的 7 个方法整块剪切进去
- [x] 在 `mod.rs` 添加模块声明：
  ```rust
  mod hitl_ops;
  mod ask_user_ops;
  ```
- [x] `cargo build -p rust-agent-tui` 验证编译

**检查步骤:**
- [x] 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error[`
- [x] hitl_ops.rs 包含预期方法数量
  - `grep -c "pub fn\|fn " rust-agent-tui/src/app/hitl_ops.rs`
  - 预期: ≥ 6（含私有方法）
- [x] ask_user_ops.rs 包含预期方法数量
  - `grep -c "pub fn" rust-agent-tui/src/app/ask_user_ops.rs`
  - 预期: ≥ 7

---

### Task 3: app 线程与面板管理

**涉及文件:**
- 新建: `rust-agent-tui/src/app/thread_ops.rs`
- 新建: `rust-agent-tui/src/app/panel_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 新建 `thread_ops.rs`（`use super::*; impl App { }`），迁移以下方法：
  - `ensure_thread_id`（680行，私有）
  - `scroll_up`、`scroll_down`、`toggle_collapsed_messages`（698-717行）
  - `add_pending_attachment`、`pop_pending_attachment`（1510-1521行）
  - `open_thread`、`new_thread`、`start_compact`、`open_thread_browser`（1522-1633行）
  - 这些方法涉及 `SqliteThreadStore`、`ThreadBrowser`，需在文件内补充 use（`use super::*` 通常已覆盖）
- [x] 新建 `panel_ops.rs`（`use super::*; impl App { }`），迁移以下方法：
  - `open_model_panel`、`close_model_panel`、`open_agent_panel`、`close_agent_panel`（1634-1651行）
  - `agent_panel_move_up/down/confirm/clear`（1657-1711行）
  - `model_panel_confirm_select/apply_edit/confirm_delete/activate_tab/save_alias`（1716-1843行）
  - `push_agent_event`、`process_pending_events`、`new_headless`（1845-1931行，含 cfg test）
- [x] 在 `mod.rs` 添加：
  ```rust
  mod thread_ops;
  mod panel_ops;
  ```
- [x] `cargo build -p rust-agent-tui` 验证

**检查步骤:**
- [x] 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error[`
- [x] thread_ops.rs 包含 start_compact 方法
  - `grep -n "fn start_compact" rust-agent-tui/src/app/thread_ops.rs`
  - 预期: 找到对应行
- [x] panel_ops.rs 包含 new_headless（headless 测试辅助）
  - `grep -n "fn new_headless" rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 找到对应行
- [x] 两个文件行数均在预期范围
  - `wc -l rust-agent-tui/src/app/thread_ops.rs rust-agent-tui/src/app/panel_ops.rs`
  - 预期: thread_ops.rs ≤ 220，panel_ops.rs ≤ 280

---

### Task 4: app 核心 Agent 事件处理

**涉及文件:**
- 新建: `rust-agent-tui/src/app/agent_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 新建 `agent_ops.rs`（`use super::*; impl App { }`），迁移最大的三个方法：
  - `submit_message`（818-968行，~151行）
  - `handle_agent_event`（969-1326行，~358行，私有）
  - `poll_agent`（1327-1372行，~46行）
- [x] 在 `mod.rs` 添加：
  ```rust
  mod agent_ops;
  ```
- [x] 此时 `mod.rs` 中 `impl App { }` 块应只剩 `new()`、`poll_relay`、`set_loading`、`hint_candidates_count`、`hint_complete`、`update_textarea_hint`、`set_agent_id`、`get_agent_id`、`interrupt`、`get_current_task_duration` 等基础方法
- [x] `cargo build -p rust-agent-tui` 验证

**检查步骤:**
- [x] 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error[`
- [x] agent_ops.rs 包含三个目标方法
  - `grep -n "fn submit_message\|fn handle_agent_event\|fn poll_agent" rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 找到 3 行
- [x] **app/mod.rs 行数满足目标**
  - `wc -l rust-agent-tui/src/app/mod.rs`
  - 预期: ≤ 450
- [x] agent_ops.rs 行数
  - `wc -l rust-agent-tui/src/app/agent_ops.rs`
  - 预期: ≤ 600

---

### Task 5: ui popups 子模块

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/popups/mod.rs`
- 新建: `rust-agent-tui/src/ui/popups/hitl.rs`
- 新建: `rust-agent-tui/src/ui/popups/ask_user.rs`
- 新建: `rust-agent-tui/src/ui/popups/hints.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 创建目录 `rust-agent-tui/src/ui/popups/`
- [x] 新建 `popups/mod.rs`：
  ```rust
  pub mod hitl;
  pub mod ask_user;
  pub mod hints;
  ```
- [x] 新建 `popups/hitl.rs`，将 `render_hitl_popup`（main_ui.rs 295-389行）剪切过去
  - 文件顶部补充 ratatui use 和 `use crate::app::App;`，参照原 main_ui.rs 的 use 块按需截取
  - 函数改为 `pub(crate) fn render_hitl_popup(...)`（注：Rust 子模块 pub(super) 只对直接父模块可见，改用 pub(crate)）
- [x] 新建 `popups/ask_user.rs`，将 `render_ask_user_popup`（390-518行）剪切过去
  - 函数改为 `pub(crate) fn render_ask_user_popup(...)`
- [x] 新建 `popups/hints.rs`，将 `render_command_hint`（519-581行）和 `render_skill_hint`（582-655行）剪切过去
  - 函数改为 `pub(crate) fn`
- [x] 在 `main_ui.rs` 顶部添加 `mod popups;`，并将原来直接调用 `render_hitl_popup(...)` 改为 `popups::hitl::render_hitl_popup(...)`，其余类似
- [x] `cargo build -p rust-agent-tui` 验证

**检查步骤:**
- [x] 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error[`
- [x] popups/ 目录结构完整
  - `ls rust-agent-tui/src/ui/main_ui/popups/`
  - 预期: 输出包含 `mod.rs hitl.rs ask_user.rs hints.rs`
- [x] 三个 popup 文件行数均在预期范围
  - `wc -l rust-agent-tui/src/ui/main_ui/popups/*.rs`
  - 预期: hitl.rs ≤ 140（含 format_input_preview），ask_user.rs ≤ 150，hints.rs ≤ 160

---

### Task 6: ui panels 子模块

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/panels/mod.rs`
- 新建: `rust-agent-tui/src/ui/panels/model.rs`
- 新建: `rust-agent-tui/src/ui/panels/thread_browser.rs`
- 新建: `rust-agent-tui/src/ui/panels/agent.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 创建目录 `rust-agent-tui/src/ui/panels/`
- [x] 新建 `panels/mod.rs`：
  ```rust
  pub mod model;
  pub mod thread_browser;
  pub mod agent;
  ```
- [x] 新建 `panels/model.rs`，将 `render_model_panel`（656-984行）和 `mask_api_key`（985-996行）剪切过去
  - `render_model_panel` 改为 `pub(crate) fn`；`mask_api_key` 保持私有 `fn`
  - 需要从 main_ui.rs 顶部的 use 中引入 `model_panel::{AliasEditField, AliasTab, EditField, ModelPanelMode, PROVIDER_TYPES}`
- [x] 新建 `panels/thread_browser.rs`，将 `render_thread_browser`（997-1070行）剪切过去
  - 函数改为 `pub(crate) fn`
- [x] 新建 `panels/agent.rs`，将 `render_agent_panel`（1071-1184行）和 `format_input_preview`（1185-1219行）剪切过去
  - `render_agent_panel` 改为 `pub(crate) fn`；`format_input_preview` 在此文件中未使用（仅 popups/hitl.rs 用），已删除
- [x] 在 `main_ui.rs` 添加 `mod panels;`，更新调用路径为 `panels::model::render_model_panel(...)`、`panels::thread_browser::render_thread_browser(...)`、`panels::agent::render_agent_panel(...)`
- [x] `cargo build -p rust-agent-tui` 验证

**检查步骤:**
- [x] 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error[`
- [x] panels/ 目录结构完整
  - `ls rust-agent-tui/src/ui/main_ui/panels/`
  - 预期: 输出包含 `mod.rs model.rs thread_browser.rs agent.rs`
- [x] panels/model.rs 包含最大的渲染函数
  - `grep -n "fn render_model_panel" rust-agent-tui/src/ui/main_ui/panels/model.rs`
  - 预期: 找到对应行
- [x] model.rs 行数
  - `wc -l rust-agent-tui/src/ui/main_ui/panels/model.rs`
  - 预期: ≤ 360

---

### Task 7: ui status_bar + main_ui.rs 精简

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/status_bar.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 新建 `status_bar.rs`，将 `render_status_bar`（164-294行）和 `format_duration`（151-163行）剪切过去
  - `render_status_bar` 改为 `pub(crate) fn`；`format_duration` 保持私有 `fn`
  - 补充 use：ratatui 相关类型、`use crate::app::App;`
- [x] 在 `main_ui.rs` 添加 `mod status_bar;`，将调用改为 `status_bar::render_status_bar(f, app, status_area);`
- [x] 确认 `main_ui.rs` 顶部的 `use crate::app::model_panel::...` 已移到 `panels/model.rs`，从 main_ui.rs 删除该行
- [x] `cargo build -p rust-agent-tui` 验证
- [x] 检查 main_ui.rs 行数，若仍超 300 行则检查是否有未迁移的函数

**检查步骤:**
- [x] 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error[`
- [x] status_bar.rs 包含目标函数
  - `grep -n "fn render_status_bar\|fn format_duration" rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 找到 2 行
- [x] **main_ui.rs 行数满足目标**
  - `wc -l rust-agent-tui/src/ui/main_ui.rs`
  - 预期: ≤ 300（实际 239 行）
- [x] status_bar.rs 行数
  - `wc -l rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: ≤ 155（实际 152 行）

---

### Task 8: 大文件拆分 Acceptance

**Prerequisites:**
- 所有 Task 1-7 已完成
- 启动命令: `cargo run -p rust-agent-tui`（验证可运行，无需完整交互）
- 测试运行: `cargo test -p rust-agent-tui`

**End-to-end verification:**

1. 全量编译无警告
   - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error|^warning" | grep -v "generated [0-9]"`
   - Expected: 无新增 warning（允许原有 allow 属性覆盖的 warning）
   - On failure: check Task 1-7 对应文件的 use 导入是否完整
   - ✅ 已验证：无输出（零 error/warning）

2. 所有测试通过（含 headless 测试）
   - `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - Expected: 输出包含 `test result: ok` 且无 `FAILED`
   - On failure: check Task 3 panel_ops.rs 中 `new_headless` 的迁移是否正确，以及 Task 4 中 `push_agent_event`/`process_pending_events` 是否已迁移
   - ✅ 已验证：54 passed; 0 failed

3. 核心文件行数均达标
   - `wc -l rust-agent-tui/src/app/mod.rs rust-agent-tui/src/ui/main_ui.rs`
   - Expected: app/mod.rs ≤ 450，ui/main_ui.rs ≤ 300
   - On failure: check Task 4（agent_ops.rs）或 Task 7（status_bar.rs）是否完整迁移
   - ✅ 已验证：app/mod.rs=433行，ui/main_ui.rs=239行

4. 外部调用路径未发生变化——event.rs 引用的类型全部可编译
   - `cargo check -p rust-agent-tui 2>&1 | grep "event.rs"`
   - Expected: 无 event.rs 相关错误
   - On failure: check Task 1 的 `pub use hitl_prompt::PendingAttachment;` 重导出是否已加入 mod.rs
   - ✅ 已验证：无输出

5. 新增文件结构完整性校验
   - `find rust-agent-tui/src/app -name "*.rs" | sort && find rust-agent-tui/src/ui/main_ui -name "*.rs" | sort`
   - Expected: 包含所有新建文件（实际新建 18 个，超过原计划 15 个，因额外提取了 relay_ops.rs 和 hint_ops.rs 以满足行数约束）
   - ✅ 已验证：所有文件存在
