# Headless 测试降级计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 headless_test.rs 中不需要端到端 App 的测试拆分到对应模块的纯单元测试文件，减少 headless 测试总量和 `cargo test` 耗时。

**Architecture:** headless_test.rs 目前 4356 行 / 105 个测试，其中 42 个测试（~1000 行）完全不依赖端到端渲染或事件管线——它们只是用了 `new_headless()` 图方便构造 App 实例，但实际只测了纯函数或简单状态读写。将这些测试移到对应模块的 `_test.rs` 文件中，改用最小依赖构造，headless_test.rs 只保留真正的端到端测试。

**Tech Stack:** Rust, tokio::test, ratatui TestBackend（仅保留给 E2E 测试）

---

## 当前状态

| 类别 | 测试数 | 行数 | 去向 |
|------|--------|------|------|
| Markdown 纯函数 | 15 | 272 | → `ui/markdown_test.rs` |
| 纯配置/逻辑（用 new_headless 但只读 services） | 14 | 327 | → 对应模块 `_test.rs` |
| 纯组件逻辑（构造 widget/VM 直接断言） | 13 | 403 | → 对应模块 `_test.rs` |
| **保留 E2E** | **63** | **~3000** | **留在 `headless_test.rs`** |

降级后 headless_test.rs 从 4356 行 / 105 个测试 → ~3000 行 / 63 个测试。
纯单元测试增加 ~1000 行 / 42 个测试，运行速度显著提升（不需要 App 初始化和 SQLite）。

---

## 辅助函数迁移

headless_test.rs 顶部的辅助函数也需要拆分：

| 辅助函数 | 去向 |
|----------|------|
| `parse_markdown_default()` | → `ui/markdown_test.rs`（跟着 markdown 测试走） |
| `advance_to_form()` | → `ui/main_ui/popups/setup_wizard_test.rs`（跟着 wizard 测试走） |
| 其他 headless-only 辅助 | 留在 `headless_test.rs` |

---

## Task 1: 迁移 Markdown 测试到 ui/markdown_test.rs

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs`（删除 15 个 test_md_* 测试 + parse_markdown_default）
- Modify: `peri-tui/src/ui/markdown.rs`（添加 `#[cfg(test)] mod tests` 引用）
- Create: `peri-tui/src/ui/markdown_test.rs`

这 15 个测试全部是纯函数测试，调用 `parse_markdown_default()` 解析 markdown 文本后断言输出行内容，完全不依赖 App 或渲染。

要迁移的测试：
- `test_md_heading`, `test_md_heading_h2`, `test_md_inline_styles`, `test_md_inline_code`
- `test_md_code_block`, `test_md_unordered_list`, `test_md_ordered_list`, `test_md_blockquote`
- `test_md_rule`, `test_md_incomplete_does_not_panic`, `test_md_table_basic`
- `test_md_table_cell_count`, `test_md_table_border_alignment`, `test_md_table_alignment`
- `test_md_table_with_inline_code`

- [ ] **Step 1: 创建 `ui/markdown_test.rs`**

从 headless_test.rs 中提取 `parse_markdown_default()` 辅助函数和全部 15 个 `test_md_*` 测试。在 markdown_test.rs 中 `use super::*;` 即可访问 markdown 模块的类型。将 `parse_markdown_default` 改为直接调用 `parse_markdown` 的默认参数版本。

- [ ] **Step 2: 在 `ui/markdown.rs` 底部添加 test module 引用**

```rust
#[cfg(test)]
#[path = "markdown_test.rs"]
mod tests;
```

- [ ] **Step 3: 从 `headless_test.rs` 中删除已迁移的测试和辅助函数**

删除 `parse_markdown_default()` 函数定义和全部 15 个 `test_md_*` 测试。

- [ ] **Step 4: 验证编译和测试通过**

```bash
cargo test -p peri-tui --lib -- ui::markdown::tests
cargo test -p peri-tui --lib -- ui::headless::tests
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: move markdown tests from headless to ui/markdown_test.rs"
```

---

## Task 2: 迁移纯组件逻辑测试（render_view_model 类）

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs`（删除 5 个测试）
- Modify: `peri-tui/src/ui/message_render.rs`（添加 test module 引用，如尚未有）

这 5 个测试构造 `MessageViewModel` 变体，调用 `render_view_model()` 断言渲染输出，完全不依赖 App。

要迁移的测试：
- `test_tool_block_error_visible_when_collapsed`（31 行）
- `test_tool_block_success_no_summary_when_collapsed`（21 行）
- `test_tool_call_group_error_visible_when_collapsed`（44 行）
- `test_subagent_group_error_red_title_and_summary`（39 行）
- `test_system_note_error_detection`（19 行）

- [ ] **Step 1: 在 `ui/message_render.rs` 底部添加 test module 引用（如尚未有）**

```rust
#[cfg(test)]
#[path = "message_render_test.rs"]
mod tests;
```

- [ ] **Step 2: 创建 `ui/message_render_test.rs`，迁移 5 个测试**

`use super::*;` 访问 message_render 模块的类型。测试中用到 `MessageViewModel` 需要从 `crate::app` 引入。

- [ ] **Step 3: 从 `headless_test.rs` 中删除已迁移的测试**

- [ ] **Step 4: 验证**

```bash
cargo test -p peri-tui --lib -- ui::message_render::tests
cargo test -p peri-tui --lib -- ui::headless::tests
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: move render_view_model tests from headless to message_render_test.rs"
```

---

## Task 3: 迁移 SetupWizard 纯组件测试

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs`（删除 5 个测试 + advance_to_form 辅助）
- Modify: `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` 或其 test 文件

这 5 个测试直接构造 `SetupWizardPanel` 操作状态，不依赖 App 渲染：

- `test_setup_wizard_full_flow_anthropic`（23 行）
- `test_setup_wizard_full_flow_openai`（40 行）
- `test_setup_wizard_esc_navigation`（25 行）
- `test_setup_wizard_toggle_select`（11 行）
- `test_setup_wizard_multi_provider`（22 行）

同时迁移 `advance_to_form()` 辅助函数。

注意：`test_setup_wizard_saves_and_clears` 用了 `new_headless` + `app.services`，已在 Task 4 中处理。

- [ ] **Step 1: 将 5 个 wizard 测试 + `advance_to_form()` 从 headless_test.rs 移到 `ui/main_ui/popups/setup_wizard_test.rs`**

检查 setup_wizard_test.rs 是否已存在。如果已有，追加测试；如果没有，创建新文件并在 setup_wizard.rs 底部添加 `#[cfg(test)]` 引用。

- [ ] **Step 2: 从 headless_test.rs 中删除已迁移的测试和辅助函数**

- [ ] **Step 3: 验证**

```bash
cargo test -p peri-tui --lib -- ui::main_ui::popups::setup_wizard
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "refactor: move SetupWizard unit tests from headless to setup_wizard_test.rs"
```

---

## Task 4: 迁移纯配置/逻辑测试

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs`（删除 14 个测试）
- 多个目标 `_test.rs` 文件

这 14 个测试用了 `new_headless()` 但只访问 `app.services.*` 字段或简单状态操作。改写为不需要 App 的纯单元测试。

### 4a. 权限模式测试（→ app 级别）

- `test_app_default_permission_mode_is_bypass`（10 行）→ `app/` 某个 test 文件
- `test_permission_mode_store_and_load`（20 行）→ 只测 `SharedPermissionMode`
- `test_permission_mode_cycle`（11 行）→ 只测 `SharedPermissionMode`
- `test_shift_tab_cycles_permission_mode`（18 行）→ 只测 `SharedPermissionMode`
- `test_mode_highlight_until_set_on_cycle`（21 行）→ 可能需要 App 状态

这些测试的核心是 `SharedPermissionMode` 的 `store/load/cycle`。前三个完全不需要 App，后两个需要看具体实现。

### 4b. 配置读取测试（→ app 级别）

- `test_get_compact_config_default`（8 行）→ `CompactConfig::default()` 不需要 App
- `test_get_compact_config_from_settings`（15 行）→ 需要 `PeriConfig`，不需要 App
- `test_needs_setup_triggers_for_empty_config`（7 行）→ 静态判断逻辑

### 4c. 面板状态逻辑测试（→ 对应面板 test 文件）

- `test_model_panel_confirm_shows_feedback`（62 行）→ `ui/main_ui/panels/model_test.rs`（已存在）
- `test_login_select_provider_shows_feedback`（65 行）→ `ui/main_ui/panels/login_test.rs`（已存在）
- `test_model_panel_space_selects_model`（41 行）→ `ui/main_ui/panels/model_test.rs`
- `test_cron_panel_delete_confirmation`（60 行）→ 需新建或追加到 cron 测试
- `test_split_session_panel_independence`（40 行）→ 需要 App session 状态

### 4d. Pipeline 纯逻辑测试

- `test_state_snapshot_is_incremental`（27 行）→ `app/message_pipeline_test.rs`，只测 `MessagePipeline`

- [ ] **Step 1: 逐一将 4a 组测试改写为纯单元测试，移到合适位置**

`SharedPermissionMode` 测试移到 `app/` 目录下合适的 `_test.rs`，直接构造 `SharedPermissionMode::new()` 不需要 App。

- [ ] **Step 2: 逐一将 4b 组测试改写**

`get_compact_config` 测试直接构造 `App` 的最小必要状态或直接测 `CompactConfig::default()`。`needs_setup` 测试测静态函数。

- [ ] **Step 3: 逐一将 4c 组测试改写**

面板状态测试移到对应面板的 `_test.rs`，直接构造面板状态不通过 App。

- [ ] **Step 4: 将 `test_state_snapshot_is_incremental` 移到 message_pipeline 测试**

- [ ] **Step 5: 验证全部通过**

```bash
cargo test -p peri-tui --lib
```

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor: downgrade config/logic tests from headless to unit tests"
```

---

## Task 5: 最终清理 + 验证

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs`

- [ ] **Step 1: 检查 headless_test.rs 中是否还有不需要 headless 的残留测试**

```bash
# 确认 headless_test.rs 中所有测试都用到 new_headless 或渲染管线
grep -n "fn test_" peri-tui/src/ui/headless_test.rs
```

- [ ] **Step 2: 删除 headless_test.rs 中不再使用的辅助函数和 import**

- [ ] **Step 3: 全量测试验证**

```bash
cargo test --workspace
```

- [ ] **Step 4: 统计耗时对比**

```bash
time cargo test -p peri-tui --lib
```

预期：headless 测试从 105 个降到 ~63 个，减少 ~40% 的重量级 App 初始化。

- [ ] **Step 5: Final commit**

```bash
git add -A && git commit -m "refactor: clean up headless_test.rs after test migration"
```

---

## 预期收益

| 指标 | 优化前 | 优化后 |
|------|--------|--------|
| headless_test.rs 行数 | 4356 | ~3000 |
| headless 测试数量 | 105 | ~63 |
| 纯单元测试数量 | ~440 | ~482 |
| `cargo test` 耗时（peri-tui） | ~5s | 预期 ~3.5s（减少 ~40 个 App 初始化） |
