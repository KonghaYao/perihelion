# Plugin Panel 四向拆分为子模块

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 将 1817 行的 `app/plugin_panel.rs` 拆分为 `app/plugin_panel/` 目录，含三个子文件：`types.rs`（类型定义）、`handlers.rs`（事件处理）、`mod.rs`（查询+PanelComponent+App 桥接）

**Architecture:** 按职责垂直拆分。类型定义独立为 `types.rs` 供渲染模块零依赖引用。事件处理方法独立为 `handlers.rs` 保持 handler 内聚。`mod.rs` 保留构造、查询和 App 桥接胶水代码。外部调用者通过 `use crate::app::plugin_panel::{...}` 统一路径不受影响。

**Tech Stack:** Rust 2021, ratatui, tui-textarea, tokio

---

### Task 1: 创建 `plugin_panel/` 目录结构

**Files:**
- Create: `peri-tui/src/app/plugin_panel/`
- Create: `peri-tui/src/app/plugin_panel/mod.rs`
- Create: `peri-tui/src/app/plugin_panel/types.rs`
- Create: `peri-tui/src/app/plugin_panel/handlers.rs`

- [ ] **Step 1: Create directory and empty files**

```bash
mkdir -p peri-tui/src/app/plugin_panel
touch peri-tui/src/app/plugin_panel/mod.rs
touch peri-tui/src/app/plugin_panel/types.rs
touch peri-tui/src/app/plugin_panel/handlers.rs
```

---

### Task 2: 移动类型定义到 `types.rs`

**Files:**
- Create: `peri-tui/src/app/plugin_panel/types.rs`
- Modify: `peri-tui/src/app/plugin_panel.rs:1-212`

- [ ] **Step 1: 将 `plugin_panel.rs` 第 1-212 行的类型定义移至 `types.rs`**

Copy these entire blocks from `plugin_panel.rs` lines 1-212:
- Imports (lines 1-13)
- `DiscoverPlugin` struct and `DiscoverDetailAction` enum
- `MarketplaceViewEntry` struct
- `MarketplaceViewStatus` enum
- `PluginItemType` enum
- `PluginEntry` struct and `DetailAction` enum
- `PluginPanelView` enum and its impl block
- `PluginPanel` struct definition

Write `types.rs`:

```rust
use std::collections::HashSet;

use peri_middlewares::plugin::InstallScope;
use peri_widgets::InputState;

use super::super::panel_list::PanelList;

/// Discover 视图中展示的可用插件
#[derive(Debug, Clone)]
pub struct DiscoverPlugin {
    pub name: String,
    pub description: String,
    pub marketplace: String,
    pub version: String,
    pub author: String,
    pub installed: Option<String>,
    pub plugin_id: String,
    pub install_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverDetailAction {
    InstallUser,
    InstallProject,
    BackToList,
}

impl DiscoverDetailAction {
    pub const ALL: [DiscoverDetailAction; 3] = [
        DiscoverDetailAction::InstallUser,
        DiscoverDetailAction::InstallProject,
        DiscoverDetailAction::BackToList,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DiscoverDetailAction::InstallUser => "Install (user)",
            DiscoverDetailAction::InstallProject => "Install (project)",
            DiscoverDetailAction::BackToList => "Back to list",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarketplaceViewEntry {
    pub name: String,
    pub source: peri_middlewares::plugin::MarketplaceSource,
    pub source_label: &'static str,
    pub plugin_count: usize,
    pub installed_count: usize,
    pub status: MarketplaceViewStatus,
    pub last_updated: String,
    pub auto_update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketplaceViewStatus {
    Fresh,
    Cached,
    Fetching,
    Stale,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginItemType {
    Plugin,
    Mcp,
}

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub plugin_type: PluginItemType,
    pub marketplace: String,
    pub enabled: bool,
    pub scope: InstallScope,
    pub version: String,
    pub install_path: Option<std::path::PathBuf>,
    pub project_path: Option<std::path::PathBuf>,
    pub load_error: Option<String>,
    pub description: String,
    pub author: String,
    pub commands: Vec<String>,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailAction {
    ToggleEnabled,
    Uninstall,
    BackToList,
}

impl DetailAction {
    pub const ALL: [DetailAction; 3] = [
        DetailAction::ToggleEnabled,
        DetailAction::Uninstall,
        DetailAction::BackToList,
    ];

    pub fn label(&self, enabled: bool) -> &'static str {
        match self {
            DetailAction::ToggleEnabled => {
                if enabled {
                    "Disable"
                } else {
                    "Enable"
                }
            }
            DetailAction::Uninstall => "Uninstall",
            DetailAction::BackToList => "Back to list",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginPanelView {
    Installed,
    Discover,
    Marketplaces,
    Errors,
}

impl PluginPanelView {
    pub fn label(&self) -> &'static str {
        match self {
            PluginPanelView::Installed => "Installed",
            PluginPanelView::Discover => "Discover",
            PluginPanelView::Marketplaces => "Marketplaces",
            PluginPanelView::Errors => "Errors",
        }
    }

    pub const ALL: [PluginPanelView; 4] = [
        PluginPanelView::Installed,
        PluginPanelView::Discover,
        PluginPanelView::Marketplaces,
        PluginPanelView::Errors,
    ];

    pub fn next(&mut self) {
        let idx = Self::ALL.iter().position(|v| v == self).unwrap_or(0);
        *self = Self::ALL[(idx + 1) % Self::ALL.len()];
    }

    pub fn prev(&mut self) {
        let idx = Self::ALL.iter().position(|v| v == self).unwrap_or(0);
        *self = Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()];
    }
}

#[derive(Debug)]
pub struct PluginPanel {
    pub view: PluginPanelView,
    pub entries: Vec<PluginEntry>,
    pub installed_list: PanelList,
    pub confirm_delete: Option<String>,
    pub detail_index: Option<usize>,
    pub detail_cursor: usize,
    pub discover_plugins: Vec<DiscoverPlugin>,
    pub discover_search: InputState,
    pub discover_searching: bool,
    pub discover_list: PanelList,
    pub discover_loading: bool,
    pub discover_selected: HashSet<String>,
    pub marketplace_list: PanelList,
    pub marketplace_entries: Vec<MarketplaceViewEntry>,
    pub marketplace_confirm_delete: bool,
    pub marketplace_add_name: InputState,
    pub marketplace_add_active: bool,
    pub uninstalling: HashSet<String>,
}
```

- [ ] **Step 2: Remove types from `plugin_panel.rs`**

Delete lines 1-212 from `plugin_panel.rs`. Keep the rest (lines 214 onward) for the next step.

- [ ] **Step 3: Verify build**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

Expected: FAIL — `mod.rs` doesn't re-export types yet.

---

### Task 3: 创建 `mod.rs` 并保留查询+PanelComponent+App 桥接

**Files:**
- Modify: `peri-tui/src/app/plugin_panel/mod.rs`

- [ ] **Step 1: Write `mod.rs` re-exporting submodules and containing PluginPanel methods + PanelComponent impl + App bridge**

```rust
pub mod types;
pub mod handlers;

pub use types::*;

use std::any::Any;

use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use super::panel_component::PanelComponent;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

impl PluginPanel {
    pub fn new(entries: Vec<PluginEntry>) -> Self {
        let mut installed_list = PanelList::new();
        installed_list.set_items(entries.clone());
        Self {
            view: PluginPanelView::Installed,
            entries,
            installed_list,
            confirm_delete: None,
            detail_index: None,
            detail_cursor: 0,
            discover_plugins: Vec::new(),
            discover_search: InputState::new(),
            discover_searching: false,
            discover_list: PanelList::new(),
            discover_loading: false,
            discover_selected: HashSet::new(),
            marketplace_list: PanelList::new(),
            marketplace_entries: Vec::new(),
            marketplace_confirm_delete: false,
            marketplace_add_name: InputState::new(),
            marketplace_add_active: false,
            uninstalling: HashSet::new(),
        }
    }

    pub fn is_detail(&self) -> bool {
        self.detail_index.is_some()
    }

    pub fn discover_filtered_plugins(&self) -> Vec<&DiscoverPlugin> {
        let term = self.discover_search.value().to_lowercase();
        self.discover_plugins
            .iter()
            .filter(|p| {
                if term.is_empty() {
                    true
                } else {
                    p.name.to_lowercase().contains(&term)
                        || p.description.to_lowercase().contains(&term)
                }
            })
            .collect()
    }

    pub fn discover_current_plugin(&self) -> Option<&DiscoverPlugin> {
        self.discover_filtered_plugins()
            .get(self.discover_list.cursor())
            .copied()
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => self.installed_list.visible_indices(),
            PluginPanelView::Discover => self.discover_list.visible_indices(),
            PluginPanelView::Marketplaces => self.marketplace_list.visible_indices(),
        }
    }

    pub fn current_list_len(&self) -> usize {
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => self.installed_list.len(),
            PluginPanelView::Discover => self.discover_list.len(),
            PluginPanelView::Marketplaces => self.marketplace_list.len(),
        }
    }

    pub fn cursor(&self) -> usize {
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => self.installed_list.cursor(),
            PluginPanelView::Discover => self.discover_list.cursor(),
            PluginPanelView::Marketplaces => self.marketplace_list.cursor(),
        }
    }

    pub fn scroll_offset(&self) -> u16 {
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => {
                self.installed_list.scroll_offset()
            }
            PluginPanelView::Discover => self.discover_list.scroll_offset(),
            PluginPanelView::Marketplaces => self.marketplace_list.scroll_offset(),
        }
    }

    pub fn set_scroll_offset(&mut self, offset: u16) {
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => {
                self.installed_list.set_scroll_offset(offset);
            }
            PluginPanelView::Discover => {
                self.discover_list.set_scroll_offset(offset);
            }
            PluginPanelView::Marketplaces => {
                self.marketplace_list.set_scroll_offset(offset);
            }
        }
    }

    pub fn selected_entry(&self) -> Option<&PluginEntry> {
        self.installed_list
            .cursor_item::<PluginEntry>()
            .or_else(|| self.entries.get(self.installed_list.cursor()))
    }

    fn sync_current_view_items(&mut self) {
        match self.view {
            PluginPanelView::Installed => {
                self.installed_list.set_items(self.entries.clone());
            }
            PluginPanelView::Errors => {
                let error_entries: Vec<PluginEntry> = self
                    .entries
                    .iter()
                    .filter(|e| e.load_error.is_some())
                    .cloned()
                    .collect();
                self.installed_list.set_items(error_entries);
            }
            PluginPanelView::Discover => {
                let filtered = self.discover_filtered_plugins();
                let items: Vec<DiscoverPlugin> = filtered.into_iter().cloned().collect();
                self.discover_list.set_items(items);
            }
            PluginPanelView::Marketplaces => {
                let items: Vec<MarketplaceViewEntry> = self.marketplace_entries.clone();
                self.marketplace_list.set_items(items);
            }
        }
    }
}

impl PanelComponent for PluginPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Plugin
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        if self.confirm_delete.is_some() {
            return self.handle_confirm_delete(input, ctx);
        }
        if self.discover_searching {
            return self.handle_discover_searching(input, ctx);
        }
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => {
                if self.detail_index.is_some() {
                    self.handle_installed_detail(input, ctx)
                } else {
                    self.handle_installed_list(input, ctx)
                }
            }
            PluginPanelView::Discover => {
                if self.detail_index.is_some() {
                    self.handle_discover_detail(input, ctx)
                } else {
                    self.handle_discover_list(input, ctx)
                }
            }
            PluginPanelView::Marketplaces => {
                if self.marketplace_add_active {
                    self.handle_marketplace_add(input, ctx)
                } else if self.marketplace_confirm_delete {
                    self.handle_marketplace_confirm_delete(input, ctx)
                } else {
                    self.handle_marketplaces_list(input, ctx)
                }
            }
        }
    }

    fn handle_paste(&mut self, _text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::Ignored
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        match self.view {
            PluginPanelView::Installed | PluginPanelView::Errors => {
                self.installed_list.scroll(lines);
            }
            PluginPanelView::Discover => {
                self.discover_list.scroll(lines);
            }
            PluginPanelView::Marketplaces => {
                self.marketplace_list.scroll(lines);
            }
        }
        EventResult::Consumed
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        screen_height.saturating_sub(4)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::plugin::render_plugin_panel(f, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        // ... (keep existing 70-line status_bar_hints body unchanged) ...
    }
}

// ─── App 桥接方法 ────────────────────────────────────────────────────────────

impl App {
    pub fn plugin_panel_move_up(&mut self) {
        // ... (keep all existing App methods unchanged) ...
    }
    // ... (all 25 App bridge methods) ...
}
```

- [ ] **Step 2: Build to verify mod.rs compiles with type references**

```bash
cargo build -p peri-tui 2>&1 | head -30
```

Expected: Compilation errors about `handle_*` methods not found (they're in `handlers.rs`).

---

### Task 4: 移动事件处理方法到 `handlers.rs`

**Files:**
- Create: `peri-tui/src/app/plugin_panel/handlers.rs`
- Modify: `peri-tui/src/app/plugin_panel.rs` (删除 handlers 代码)

- [ ] **Step 1: Write `handlers.rs` with all `handle_*` methods and helper functions**

Copy the entire `impl PluginPanel { ... }` block from `plugin_panel.rs` lines 530-1340 into `handlers.rs`.

```rust
use std::collections::HashSet;

use peri_middlewares::plugin::{self, claude_home, load_known_marketplaces};
use tui_textarea::{Input, Key};
use tracing::info;

use super::super::panel_manager::{EventResult, PanelContext};
use super::types::*;

impl PluginPanel {
    fn handle_confirm_delete(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_discover_searching(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_discover_detail(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_installed_detail(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_installed_list(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_discover_list(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_marketplaces_list(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_marketplace_confirm_delete(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn handle_marketplace_add(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // ... (entire existing body) ...
    }

    fn spawn_install_current(
        &self,
        plugin: &DiscoverPlugin,
        scope: InstallScope,
        tx: tokio::sync::mpsc::UnboundedSender<super::super::events::AgentEvent>,
    ) {
        // ... (entire existing body) ...
    }

    fn do_detail_action(&mut self, action: DetailAction, ctx: &mut PanelContext<'_>) {
        // ... (entire existing body) ...
    }

    fn persist_enabled_state(&self, ctx: &mut PanelContext<'_>) {
        // ... (entire existing body) ...
    }

    fn persist_marketplace_delete(&mut self, ctx: &mut PanelContext<'_>) {
        // ... (entire existing body) ...
    }

    fn persist_marketplace_add(&mut self, ctx: &mut PanelContext<'_>) -> Option<String> {
        // ... (entire existing body) ...
    }
}
```

**Critical:** All `super::` references in the original file become one more level deep. Update `super::panel_manager::*` → `super::super::panel_manager::*`. All `self.` field accesses on PluginPanel remain unchanged since this is still `impl PluginPanel`.

- [ ] **Step 2: Delete handlers from `plugin_panel.rs`**

Remove lines 530-1340 from `plugin_panel.rs`.

- [ ] **Step 3: Build**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

Expected: compilation succeeds with no errors.

---

### Task 5: Update external references

**Files:**
- Modify: `peri-tui/src/ui/main_ui/panels/plugin.rs:11`
- Modify: `peri-tui/src/app/panel_ops.rs:385`
- Modify: `peri-tui/src/app/panel_manager.rs:18`
- Modify: `peri-tui/src/app/agent_events_plugin.rs:2`

- [ ] **Step 1: Verify imports still work**

All external files use `use crate::app::plugin_panel::{...}` which is resolved through `mod.rs`'s `pub use types::*;`. The external code should compile without changes.

```bash
cargo build -p peri-tui 2>&1 | head -10
```

Expected: compilation succeeds.

- [ ] **Step 2: Verify full crate tests**

```bash
cargo test -p peri-tui --lib plugin_panel 2>&1 | tail -10
```

Expected: all tests pass.

---

### Task 6: Delete old `plugin_panel.rs` and update `mod.rs`

**Files:**
- Delete: `peri-tui/src/app/plugin_panel.rs`
- Modify: `peri-tui/src/app/mod.rs:11`

- [ ] **Step 1: Update `mod.rs` to use directory module**

Change line 11 in `peri-tui/src/app/mod.rs` from:

```rust
pub mod plugin_panel;
```

to:

```rust
pub mod plugin_panel;
```

(No change needed — `pub mod plugin_panel;` automatically resolves to `plugin_panel/mod.rs` when `plugin_panel.rs` doesn't exist. But we must delete `plugin_panel.rs` first.)

- [ ] **Step 2: Delete old file**

```bash
rm peri-tui/src/app/plugin_panel.rs
```

- [ ] **Step 3: Final build**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

Expected: compilation succeeds.

---

### Task 7: Run full tests and commit

- [ ] **Step 1: Run all tests**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -10
```

Expected: All tests pass.

- [ ] **Step 2: Commit**

```bash
cargo fmt -p peri-tui
git add peri-tui/src/app/plugin_panel/ peri-tui/src/app/plugin_panel.rs
git commit -m "refactor: split plugin_panel into submodules

- Move type definitions to plugin_panel/types.rs (212 lines)
- Move event handlers to plugin_panel/handlers.rs (810 lines)
- Keep queries + PanelComponent + App bridge in mod.rs

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
