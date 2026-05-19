# Panel Ops 按面板拆分为子模块

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 将 1190 行的 `panel_ops.rs` 按面板类型拆分为独立子文件：`panel_plugin.rs`、`panel_login.rs`、`panel_model.rs`、`panel_config.rs`、`panel_agent.rs`、`panel_memory.rs`、`panel_status.rs`、`panel_hooks.rs`，原文件降为 re-export 入口

**Architecture:** `panel_ops.rs` 是控制器桥接层——每个函数都是 `command/panel/X.rs` → `panel_ops::func()` → `app::{panel_name}/` 的中间胶水。拆分逻辑是按面板类型垂直切分，每个面板拥有一组独立方法。测试辅助函数（cfg-gated）保留在原地。

**Tech Stack:** Rust 2021

---

### Task 1: 创建子文件并重新导出

**Files:**
- Create: `peri-tui/src/app/panel_plugin.rs`
- Create: `peri-tui/src/app/panel_login.rs`
- Create: `peri-tui/src/app/panel_model.rs`
- Create: `peri-tui/src/app/panel_config.rs`
- Create: `peri-tui/src/app/panel_agent.rs`
- Create: `peri-tui/src/app/panel_memory.rs`
- Create: `peri-tui/src/app/panel_status.rs`
- Create: `peri-tui/src/app/panel_hooks.rs`
- Modify: `peri-tui/src/app/panel_ops.rs` (reduce to re-exports)

- [ ] **Step 1: Create all empty files**

```bash
touch peri-tui/src/app/panel_plugin.rs
touch peri-tui/src/app/panel_login.rs
touch peri-tui/src/app/panel_model.rs
touch peri-tui/src/app/panel_config.rs
touch peri-tui/src/app/panel_agent.rs
touch peri-tui/src/app/panel_memory.rs
touch peri-tui/src/app/panel_status.rs
touch peri-tui/src/app/panel_hooks.rs
```

---

### Task 2: 移动 Plugin + Marketplace 操作到 `panel_plugin.rs`

**Files:**
- Create: `peri-tui/src/app/panel_plugin.rs`
- Copy from: `peri-tui/src/app/panel_ops.rs:320-890`

- [ ] **Step 1: Write `panel_plugin.rs`**

Move the following functions from `panel_ops.rs`:
- `open_mcp_panel` (lines 320-356)
- `open_cron_panel` (lines 358-382)
- `open_plugin_panel` (lines 384-648)
- `close_plugin_panel` (lines 650-658)
- `marketplace_add_and_save` (lines 660-754)
- `marketplace_delete_and_save` (lines 756-844)
- `open_agent_panel` (no — this is agent, not plugin)
- `close_agent_panel` (no)
- `marketplace_*` functions are marketplace, keep with plugin since they're used by the Plugin panel.

```rust
use super::*;
use anyhow::Result;

impl App {
    pub fn open_mcp_panel(&mut self) {
        // ... (copy existing body) ...
    }

    pub fn open_cron_panel(&mut self) {
        // ... (copy existing body) ...
    }

    pub fn open_plugin_panel(&mut self) {
        // ... (copy existing body, ~250 lines) ...
    }

    pub fn close_plugin_panel(&mut self) {
        // ... (copy existing body) ...
    }

    pub fn marketplace_add_and_save(&mut self, input: &str) -> Result<()> {
        // ... (copy existing body, ~90 lines) ...
    }

    pub fn marketplace_delete_and_save(&mut self, name: &str) -> Result<()> {
        // ... (copy existing body, ~90 lines) ...
    }
}
```

- [ ] **Step 2: Delete these functions from `panel_ops.rs`**

Remove lines 320-890 from `panel_ops.rs`.

---

### Task 3: 移动 Login 面板操作到 `panel_login.rs`

**Files:**
- Create: `peri-tui/src/app/panel_login.rs`
- Copy from: `peri-tui/src/app/panel_ops.rs:88-244`

```rust
use super::*;

impl App {
    pub fn open_login_panel(&mut self) { /* ... */ }
    pub fn close_login_panel(&mut self) { /* ... */ }
    pub fn login_panel_select_provider(&mut self) { /* ... */ }
    pub fn login_panel_apply_edit(&mut self) { /* ... */ }
    pub fn login_panel_confirm_delete(&mut self) { /* ... */ }
}
```

---

### Task 4: 移动 Model, Config, Agent, Memory, Status, Hooks 面板操作

每个对应一个文件，每文件 2-5 个函数。

**`panel_model.rs`** (lines 3-86):
```rust
impl App {
    pub fn open_model_panel(&mut self) { /* ... */ }
    pub fn close_model_panel(&mut self) { /* ... */ }
    pub fn model_panel_confirm(&mut self) { /* ... */ }
}
```

**`panel_config.rs`** (lines 246-305):
```rust
impl App {
    pub fn open_config_panel(&mut self) { /* ... */ }
    pub fn close_config_panel(&mut self) { /* ... */ }
    pub fn config_panel_apply(&mut self) { /* ... */ }
}
```

**`panel_agent.rs`** (lines 955-1020):
```rust
impl App {
    pub fn open_agent_panel(&mut self, agents: Vec<AgentItem>) { /* ... */ }
    pub fn close_agent_panel(&mut self) { /* ... */ }
    pub fn agent_panel_confirm(&mut self) { /* ... */ }
}
```

**`panel_memory.rs`** (lines 892-953):
```rust
use anyhow::Result;

impl App {
    pub fn open_memory_panel(&mut self) { /* ... */ }
    pub fn close_memory_panel(&mut self) { /* ... */ }
    pub fn memory_panel_open_editor(&mut self) -> Result<()> { /* ... */ }
}
```

**`panel_status.rs`** (lines 307-318):
```rust
impl App {
    pub fn open_status_panel(&mut self, tab: usize) { /* ... */ }
    pub fn close_status_panel(&mut self) { /* ... */ }
}
```

**`panel_hooks.rs`** (lines 1022-1053):
```rust
impl App {
    pub fn open_hooks_panel(&mut self) { /* ... */ }
    pub fn close_hooks_panel(&mut self) { /* ... */ }
    pub fn open_setup_wizard(&mut self) { /* ... */ }  // 1-line: self.setup_wizard = true
}
```

---

### Task 5: 将 `panel_ops.rs` 降为 re-export 入口 + 测试辅助

**Files:**
- Modify: `peri-tui/src/app/panel_ops.rs`

- [ ] **Step 1: Rewrite `panel_ops.rs` as a thin re-export hub**

```rust
mod panel_plugin;
mod panel_login;
mod panel_model;
mod panel_config;
mod panel_agent;
mod panel_memory;
mod panel_status;
mod panel_hooks;

pub(crate) use panel_plugin::*;
pub(crate) use panel_login::*;
pub(crate) use panel_model::*;
pub(crate) use panel_config::*;
pub(crate) use panel_agent::*;
pub(crate) use panel_memory::*;
pub(crate) use panel_status::*;
pub(crate) use panel_hooks::*;

// ─── Test helpers (cfg-gated) ──────────────────────────────────────────────

#[cfg(any(test, feature = "headless"))]
pub fn push_agent_event(app: &mut super::App, event: super::AgentEvent) {
    // ... (copy existing body from panel_ops.rs) ...
}

#[cfg(any(test, feature = "headless"))]
pub fn flush_rebuild(app: &mut super::App) {
    // ... (copy existing body) ...
}

#[cfg(any(test, feature = "headless"))]
pub fn process_pending_events(app: &mut super::App) {
    // ... (copy existing body) ...
}

#[cfg(any(test, feature = "headless"))]
pub async fn new_headless(
    width: u16,
    height: u16,
) -> (super::App, super::HeadlessHandle) {
    // ... (copy existing body, ~90 lines) ...
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

Expected: compilation succeeds. External callers (command/panel/*.rs) access functions through `panel_ops` re-exports unchanged.

---

### Task 6: 全量测试和提交

- [ ] **Step 1: Run all tests**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -10
```

- [ ] **Step 2: Commit**

```bash
cargo fmt -p peri-tui
git add peri-tui/src/app/panel_*.rs peri-tui/src/app/panel_ops.rs
git commit -m "refactor: split panel_ops.rs into per-panel submodules

- panel_plugin.rs: 6 functions (MCP, Cron, Plugin, Marketplace)
- panel_login.rs: 5 functions
- panel_model.rs: 3 functions
- panel_config.rs: 3 functions
- panel_agent.rs: 3 functions
- panel_memory.rs: 3 functions
- panel_status.rs: 2 functions
- panel_hooks.rs: 3 functions
- panel_ops.rs: reduced to re-export hub + test helpers

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
