# perihelion-widgets 组件库 人工验收清单

**生成时间:** 2026-04-27
**关联计划:** spec-plan-1.md / spec-plan-2.md / spec-plan-3.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 编译全量 workspace: `cargo build 2>&1 | tail -5` → `Finished` (2 warnings, 无 error)

---

## 验收项目

### 场景 1: 独立 Crate 可用性

验证 perihelion-widgets 作为独立 crate 可编译、可测试，且无循环依赖。

#### - [x] 1.1 workspace Cargo.toml 注册新 crate
- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 crate 已正确注册
- **操作步骤:**
  1. [A] `grep "perihelion-widgets" Cargo.toml` → 期望包含: `"perihelion-widgets"` ✅ 找到

#### - [x] 1.2 独立编译（不含 markdown feature）
- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 crate 基础编译通过
- **操作步骤:**
  1. [A] `cargo build -p perihelion-widgets 2>&1 | tail -3` → 期望包含: `Finished` ✅ `Finished` (1 warning)

#### - [x] 1.3 全量单元测试（不含 markdown feature）
- **来源:** spec-plan-1.md Task 1-3 检查步骤
- **目的:** 确认 10 个组件基础测试全部通过
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets 2>&1 | grep "test result"` → 期望包含: `passed` ✅ 41 passed

#### - [x] 1.4 markdown feature 编译与测试
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 MarkdownRenderer 组件正常
- **操作步骤:**
  1. [A] `cargo build -p perihelion-widgets --features markdown 2>&1 | tail -3` → 期望包含: `Finished` ✅ `Finished` (2 warnings)
  2. [A] `cargo test -p perihelion-widgets --features markdown -- markdown::tests 2>&1 | grep "test result"` → 期望包含: `passed` ✅ 11 passed

#### - [x] 1.5 无循环依赖
- **来源:** spec-plan-3.md Task 11 端到端验证 §3
- **目的:** 确认 widget crate 不依赖业务 crate
- **操作步骤:**
  1. [A] `cargo tree -p perihelion-widgets 2>&1 | head -20` → 期望精确: 输出不含 `rust-create-agent` 和 `rust-agent-tui` ✅ 仅依赖 ratatui

#### - [x] 1.6 公共 API 重导出验证
- **来源:** spec-plan-1.md Task 1-3 / spec-plan-2.md Task 4-6
- **目的:** 确认所有组件类型在 lib.rs 中正确重导出
- **操作步骤:**
  1. [A] `grep "pub use" perihelion-widgets/src/lib.rs` → 期望包含: `BorderedPanel`、`ScrollState`、`ScrollableArea`、`ListState`、`SelectableList`、`InputState`、`InputField`、`TabBar`、`TabState`、`RadioGroup`、`RadioState`、`CheckboxGroup`、`CheckboxState`、`FormField`、`FormState`、`Theme`、`DarkTheme` ✅ 全部找到（含 markdown 模块的 `DefaultMarkdownTheme`, `MarkdownTheme`）

---

### 场景 2: TUI BorderedPanel 替换完整性

验证 rust-agent-tui 中所有 Clear+Block+border 模式已替换为 BorderedPanel。

#### - [x] 2.1 panels 目录无 Clear+Block 残留
- **来源:** spec-plan-3.md Task 8 检查步骤
- **目的:** 确认 panels 全部替换完成
- **操作步骤:**
  1. [A] `grep -c "f.render_widget(Clear" rust-agent-tui/src/ui/main_ui/panels/*.rs` → 期望精确: 每行输出均为 `:0` ✅ 全部 0

#### - [x] 2.2 popups 目录无 Clear+Block 残留
- **来源:** spec-plan-3.md Task 8 检查步骤
- **目的:** 确认 popups 全部替换完成
- **操作步骤:**
  1. [A] `grep -c "f.render_widget(Clear" rust-agent-tui/src/ui/main_ui/popups/*.rs` → 期望精确: 每行输出均为 `:0` ✅ 全部 0

#### - [x] 2.3 BorderedPanel 引入确认
- **来源:** spec-plan-3.md Task 8
- **目的:** 确认 TUI 代码实际使用 BorderedPanel
- **操作步骤:**
  1. [A] `grep -r "BorderedPanel" rust-agent-tui/src/ui/main_ui/ --include="*.rs" | wc -l` → 期望包含: 至少 10 处引用 ✅ 21 处

---

### 场景 3: TUI FormState 状态替换

验证 RelayPanel 和 ModelPanel 的 buf_* 字段已迁移到 FormState。

#### - [x] 3.1 RelayPanel buf_* 字段已移除
- **来源:** spec-plan-3.md Task 10 检查步骤
- **目的:** 确认 RelayPanel 完全迁移
- **操作步骤:**
  1. [A] `grep -c "buf_url\|buf_token\|buf_name" rust-agent-tui/src/app/relay_panel.rs` → 期望精确: `0` ✅ 0

#### - [x] 3.2 RelayPanel 使用 FormState
- **来源:** spec-plan-3.md Task 10
- **目的:** 确认 FormState 已引入
- **操作步骤:**
  1. [A] `grep "FormState" rust-agent-tui/src/app/relay_panel.rs` → 期望包含: `FormState` ✅ `use perihelion_widgets::{FormField, FormState};`

#### - [x] 3.3 ModelPanel 文本 buf_* 字段已移除
- **来源:** spec-plan-3.md Task 10 检查步骤
- **目的:** 确认 ModelPanel 文本字段迁移（buf_thinking_enabled 除外）
- **操作步骤:**
  1. [A] `grep -c "buf_name\|buf_type\|buf_model\|buf_api_key\|buf_base_url\|buf_thinking_budget" rust-agent-tui/src/app/model_panel.rs` → 期望精确: `0` ✅ 0

#### - [x] 3.4 ModelPanel 使用 FormState
- **来源:** spec-plan-3.md Task 10
- **目的:** 确认 FormState 已引入
- **操作步骤:**
  1. [A] `grep "FormState" rust-agent-tui/src/app/model_panel.rs` → 期望包含: `FormState` ✅ `use perihelion_widgets::{FormField, FormState};`

#### - [x] 3.5 event.rs 已适配 FormState API
- **来源:** spec-plan-3.md Task 10
- **目的:** 确认键盘事件处理使用 form.handle_* 方法
- **操作步骤:**
  1. [A] `grep "form\.handle_char\|form\.handle_backspace\|form\.handle_paste" rust-agent-tui/src/event.rs` → 期望包含: `handle_char` ✅ 找到 `handle_paste`, `handle_backspace`, `handle_char`

---

### 场景 4: MarkdownRenderer 迁移

验证 TUI 的 Markdown 渲染已迁移到 widget crate。

#### - [x] 4.1 TUI 引用 widget crate markdown 模块
- **来源:** spec-plan-3.md Task 11 端到端验证 §5
- **目的:** 确认迁移完成
- **操作步骤:**
  1. [A] `grep -r "perihelion_widgets.*markdown\|perihelion_widgets.*parse_markdown" rust-agent-tui/src/ --include="*.rs"` → 期望包含: `parse_markdown` 或 `markdown` ✅ `perihelion_widgets::markdown::parse_markdown`

---

### 场景 5: 跳过项确认（Task 9 未完成替换）

验证 Task 9 中因 widget 限制而跳过的替换项确实未执行，避免半完成状态。

#### - [x] 5.1 SelectableList 渲染未替换（agent/thread_browser/cron panel）
- **来源:** spec-plan-3.md Task 9 跳过原因
- **目的:** 确认 SelectableList widget 限制未导致半替换
- **操作步骤:**
  1. [A] `grep -c "SelectableList" rust-agent-tui/src/ui/main_ui/panels/agent.rs rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs rust-agent-tui/src/ui/main_ui/panels/cron.rs` → 期望精确: 每行均为 `:0` ✅ 全部 0

#### - [x] 5.2 InputField 渲染未替换（relay/model edit 模式）
- **来源:** spec-plan-3.md Task 9 跳过原因
- **目的:** 确认 InputField cursor 限制未导致半替换
- **操作步骤:**
  1. [A] `grep -c "InputField" rust-agent-tui/src/ui/main_ui/panels/relay.rs rust-agent-tui/src/ui/main_ui/panels/model.rs` → 期望精确: 每行均为 `:0` ✅ 全部 0

#### - [x] 5.3 RadioGroup 渲染未替换（ask_user popup）
- **来源:** spec-plan-3.md Task 9 跳过原因
- **目的:** 确认 RadioGroup 多行 description 限制未导致半替换
- **操作步骤:**
  1. [A] `grep -c "RadioGroup" rust-agent-tui/src/ui/main_ui/popups/ask_user.rs` → 期望精确: `:0` ✅ 0

#### - [x] 5.4 CheckboxGroup 渲染未替换（hitl popup）
- **来源:** spec-plan-3.md Task 9 跳过原因
- **目的:** 确认 CheckboxGroup 单行限制未导致半替换
- **操作步骤:**
  1. [A] `grep -c "CheckboxGroup" rust-agent-tui/src/ui/main_ui/popups/hitl.rs` → 期望精确: `:0` ✅ 0

---

### 场景 6: 端到端回归

验证全量 workspace 编译、测试通过，无回归。

#### - [x] 6.1 全量 workspace 编译
- **来源:** spec-plan-3.md Task 11 端到端验证 §7
- **目的:** 确认所有 6 个 crate 编译成功
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望包含: `Finished`，无 `error` ✅ `Finished` (2 warnings, 无 error)

#### - [x] 6.2 全量 workspace 测试
- **来源:** spec-plan-3.md Task 11 端到端验证 §1
- **目的:** 确认无回归（允许 1 个已知多线程运行时失败）
- **操作步骤:**
  1. [A] `cargo test 2>&1 | grep "test result"` → 期望包含: 多行 `passed`，无 unexpected failure ✅ 全部 passed（480+ tests，0 failed）

#### - [x] 6.3 TUI headless 测试回归
- **来源:** spec-plan-3.md Task 11 端到端验证 §6
- **目的:** 确认 headless 渲染测试不受影响
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- ui::headless 2>&1 | grep "test result"` → 期望包含: `passed` ✅ 49 passed

#### - [x] 6.4 perihelion-widgets 全部组件测试（含 markdown）
- **来源:** spec-plan-3.md Task 11 端到端验证 §8
- **目的:** 确认 11 个组件全部通过测试
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown 2>&1 | grep "test result"` → 期望包含: `passed` ✅ 52 passed

#### - [x] 6.5 TUI lib 测试全量通过
- **来源:** spec-plan-3.md Task 9-10 检查步骤
- **目的:** 确认 TUI 测试基线（153+ tests）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"` → 期望包含: `passed` ✅ 161 passed

---

## 验收后清理

本清单无后台服务需要清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | workspace Cargo.toml 注册 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | 独立编译（无 markdown） | 1 | 0 | ✅ |
| 场景 1 | 1.3 | 全量单元测试（无 markdown） | 1 | 0 | ✅ |
| 场景 1 | 1.4 | markdown feature 编译与测试 | 2 | 0 | ✅ |
| 场景 1 | 1.5 | 无循环依赖 | 1 | 0 | ✅ |
| 场景 1 | 1.6 | 公共 API 重导出验证 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | panels 无 Clear+Block 残留 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | popups 无 Clear+Block 残留 | 1 | 0 | ✅ |
| 场景 2 | 2.3 | BorderedPanel 引入确认 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | RelayPanel buf_* 移除 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | RelayPanel 使用 FormState | 1 | 0 | ✅ |
| 场景 3 | 3.3 | ModelPanel buf_* 移除 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | ModelPanel 使用 FormState | 1 | 0 | ✅ |
| 场景 3 | 3.5 | event.rs FormState 适配 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | MarkdownRenderer 迁移 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | SelectableList 渲染未替换 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | InputField 渲染未替换 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | RadioGroup 渲染未替换 | 1 | 0 | ✅ |
| 场景 5 | 5.4 | CheckboxGroup 渲染未替换 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | 全量 workspace 编译 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | 全量 workspace 测试 | 1 | 0 | ✅ |
| 场景 6 | 6.3 | TUI headless 测试回归 | 1 | 0 | ✅ |
| 场景 6 | 6.4 | 全部组件测试（含 markdown） | 1 | 0 | ✅ |
| 场景 6 | 6.5 | TUI lib 测试全量通过 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过
