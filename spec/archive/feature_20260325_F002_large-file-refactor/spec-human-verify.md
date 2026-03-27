# 大文件拆分 人工验收清单

**生成时间:** 2026-03-25 14:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 确认在项目根目录: `test -f Cargo.toml && echo OK`
- [ ] [AUTO] 确认 Rust toolchain 可用: `cargo --version`
- [ ] [AUTO] 确认 rust-agent-tui crate 存在: `test -d rust-agent-tui && echo OK`

### 说明

本次拆分为纯代码结构重组（无逻辑变更），所有验收项均为自动化验证，无需人工界面交互。

---

## 验收项目

### 场景 1：编译与构建质量

#### - [x] 1.1 全量编译无错误、无新增警告

- **来源:** Task 8 End-to-end verification / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望: 输出包含 `Finished` 且无 `error[`
  2. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error|^warning" | grep -v "generated [0-9]"` → 期望: 无输出（表示零 error / 零新增 warning）
- **异常排查:**
  - 若出现 `error[E0603]: ... is private`：检查对应方法是否需要改为 `pub(crate)` 或 `pub(super)`
  - 若出现 `unused import`：检查对应文件是否有已迁移类型残留的 use 语句

#### - [x] 1.2 所有测试通过（含 headless 集成测试）

- **来源:** Task 8 End-to-end verification / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep "test result"` → 期望: 输出包含 `test result: ok` 且无 `FAILED`
  2. [A] `cargo test -p rust-agent-tui 2>&1 | grep -c "^test .* ok$"` → 期望: 数字 ≥ 50（当前为 54）
- **异常排查:**
  - 若 headless 测试失败：检查 `panel_ops.rs` 中 `new_headless` 是否正确迁移，以及 `agent_ops.rs` 中 `push_agent_event`/`process_pending_events` 是否正确迁移

---

### 场景 2：app 模块拆分验证

#### - [x] 2.1 app 类型文件迁移正确

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "PendingAttachment" rust-agent-tui/src/event.rs` → 期望: 找到引用行（表明外部路径 `crate::app::PendingAttachment` 仍有效）
  2. [A] `wc -l rust-agent-tui/src/app/hitl_prompt.rs` → 期望: ≤ 120 行
  3. [A] `wc -l rust-agent-tui/src/app/ask_user_prompt.rs` → 期望: ≤ 170 行
- **异常排查:**
  - 若 PendingAttachment 找不到：检查 `app/mod.rs` 是否有 `pub use hitl_prompt::PendingAttachment;` 重导出

#### - [x] 2.2 app HITL & AskUser 操作方法拆分

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -c "pub fn\|fn " rust-agent-tui/src/app/hitl_ops.rs` → 期望: ≥ 6（含私有 send_hitl_resolved）
  2. [A] `grep -c "pub fn" rust-agent-tui/src/app/ask_user_ops.rs` → 期望: ≥ 7
  3. [A] `grep -n "fn hitl_move\|fn hitl_confirm\|fn ask_user_confirm" rust-agent-tui/src/app/mod.rs` → 期望: 无输出（这些方法已迁移，不应在 mod.rs 中）
- **异常排查:**
  - 若方法仍在 mod.rs：对应 impl 块未完整迁移

#### - [x] 2.3 app 线程与面板管理拆分

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -n "fn start_compact" rust-agent-tui/src/app/thread_ops.rs` → 期望: 找到对应行
  2. [A] `grep -n "fn new_headless" rust-agent-tui/src/app/panel_ops.rs` → 期望: 找到对应行
  3. [A] `wc -l rust-agent-tui/src/app/thread_ops.rs rust-agent-tui/src/app/panel_ops.rs` → 期望: thread_ops.rs ≤ 220，panel_ops.rs ≤ 280
- **异常排查:**
  - 若行数超限：检查是否有额外代码未迁移出 mod.rs

#### - [x] 2.4 app 核心 Agent 事件处理拆分

- **来源:** Task 4 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "fn submit_message\|fn handle_agent_event\|fn poll_agent" rust-agent-tui/src/app/agent_ops.rs` → 期望: 找到 3 行
  2. [A] `wc -l rust-agent-tui/src/app/mod.rs` → 期望: ≤ 450 行（当前实际 433 行）
  3. [A] `wc -l rust-agent-tui/src/app/agent_ops.rs` → 期望: ≤ 600 行（当前实际 566 行）
  4. [A] `grep -n "fn submit_message\|fn handle_agent_event\|fn poll_agent" rust-agent-tui/src/app/mod.rs` → 期望: 无输出（方法已迁走）
- **异常排查:**
  - 若 mod.rs 行数超 450：检查是否有 relay_ops.rs / hint_ops.rs 已额外提取
  - 若 handle_agent_event 编译报 private：确认其已改为 `pub(crate)` 可见性

---

### 场景 3：ui 模块拆分验证

#### - [x] 3.1 ui popups 子模块结构完整

- **来源:** Task 5 检查步骤
- **操作步骤:**
  1. [A] `ls rust-agent-tui/src/ui/main_ui/popups/` → 期望: 输出包含 `mod.rs hitl.rs ask_user.rs hints.rs`
  2. [A] `wc -l rust-agent-tui/src/ui/main_ui/popups/hitl.rs rust-agent-tui/src/ui/main_ui/popups/ask_user.rs rust-agent-tui/src/ui/main_ui/popups/hints.rs` → 期望: 各文件 ≤ 160 行
- **异常排查:**
  - 若目录不存在：确认 `main_ui.rs` 中有 `mod popups;` 且目录位于 `src/ui/main_ui/popups/`（注意 Rust 2018 submodule 路径规则）

#### - [x] 3.2 ui panels 子模块结构完整

- **来源:** Task 6 检查步骤
- **操作步骤:**
  1. [A] `ls rust-agent-tui/src/ui/main_ui/panels/` → 期望: 输出包含 `mod.rs model.rs thread_browser.rs agent.rs`
  2. [A] `wc -l rust-agent-tui/src/ui/main_ui/panels/model.rs` → 期望: ≤ 360 行（当前实际 351 行）
- **异常排查:**
  - 若 render_model_panel 编译错误：检查 panels/model.rs 是否正确 import 了 `AliasEditField, AliasTab, EditField, ModelPanelMode, PROVIDER_TYPES`

#### - [x] 3.3 ui status_bar 拆分正确

- **来源:** Task 7 检查步骤
- **操作步骤:**
  1. [A] `grep -n "fn render_status_bar\|fn format_duration" rust-agent-tui/src/ui/main_ui/status_bar.rs` → 期望: 找到 2 行
  2. [A] `wc -l rust-agent-tui/src/ui/main_ui.rs` → 期望: ≤ 300 行（当前实际 239 行）
- **异常排查:**
  - 若 render_status_bar 找不到：检查 `main_ui.rs` 中是否有 `mod status_bar;` 且文件位于 `src/ui/main_ui/status_bar.rs`

---

### 场景 4：整体质量与约束一致性

#### - [x] 4.1 核心文件行数全部达标

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `wc -l rust-agent-tui/src/app/mod.rs rust-agent-tui/src/ui/main_ui.rs` → 期望: app/mod.rs ≤ 450，main_ui.rs ≤ 300
  2. [A] `find rust-agent-tui/src/app rust-agent-tui/src/ui/main_ui -name "*.rs" | xargs wc -l 2>/dev/null | grep -v total | awk '$1 > 600 {print $0}'` → 期望: 无输出（所有新建文件均 ≤ 600 行）
- **异常排查:**
  - 若单文件超 600 行：检查对应文件是否需要进一步拆分

#### - [x] 4.2 外部 API 路径未发生变化

- **来源:** spec-design.md 验收标准 / Task 8
- **操作步骤:**
  1. [A] `cargo check -p rust-agent-tui 2>&1 | grep -E "error\[" | head -5` → 期望: 无输出
  2. [A] `grep -rn "crate::app::{" rust-agent-tui/src/ | grep -v "^rust-agent-tui/src/app/"` → 期望: 所有引用路径仍有效（结合编译通过验证）
- **异常排查:**
  - 若出现找不到的类型：检查 `app/mod.rs` 中 `pub use` 重导出是否完整

#### - [x] 4.3 新建文件结构完整性（18 个新文件）

- **来源:** Task 8 End-to-end verification
- **操作步骤:**
  1. [A] `find rust-agent-tui/src/app -name "*.rs" | xargs -I{} basename {} | sort | grep -E "agent_ops|ask_user_ops|ask_user_prompt|hint_ops|hitl_ops|hitl_prompt|panel_ops|relay_ops|thread_ops"` → 期望: 输出包含以上 9 个新建文件名
- **异常排查:**
  - 若某文件缺失：检查 `app/mod.rs` 中是否有对应 `mod <name>;` 声明

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 编译与构建质量 | 1.1 | 全量编译无错误无警告 | 2 | 0 | ✅ | |
| 编译与构建质量 | 1.2 | 所有测试通过（含 headless） | 2 | 0 | ✅ | |
| app 模块拆分 | 2.1 | app 类型文件迁移正确 | 3 | 0 | ✅ | |
| app 模块拆分 | 2.2 | HITL & AskUser 操作方法拆分 | 3 | 0 | ✅ | |
| app 模块拆分 | 2.3 | 线程与面板管理拆分 | 3 | 0 | ✅ | |
| app 模块拆分 | 2.4 | 核心 Agent 事件处理拆分 | 4 | 0 | ✅ | |
| ui 模块拆分 | 3.1 | ui popups 子模块结构完整 | 2 | 0 | ✅ | |
| ui 模块拆分 | 3.2 | ui panels 子模块结构完整 | 2 | 0 | ✅ | |
| ui 模块拆分 | 3.3 | ui status_bar 拆分正确 | 2 | 0 | ✅ | |
| 整体质量 | 4.1 | 核心文件行数全部达标 | 2 | 0 | ✅ | model_panel.rs(预存) 不计入新建约束 |
| 整体质量 | 4.2 | 外部 API 路径未变化 | 2 | 0 | ✅ | |
| 整体质量 | 4.3 | 新建文件结构完整（18个） | 1 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
