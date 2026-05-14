use std::any::Any;

use perihelion_widgets::InputState;
use ratatui::layout::Rect;
use ratatui::Frame;
use rust_agent_middlewares::plugin::InstallScope;
use std::collections::HashSet;
use tui_textarea::{Input, Key};

use super::panel_component::PanelComponent;
use super::panel_list::PanelList;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

/// Discover 视图中展示的可用插件
#[derive(Debug, Clone)]
pub struct DiscoverPlugin {
    pub name: String,
    pub description: String,
    pub marketplace: String,
    pub version: String,
    pub author: Option<String>,
    pub installed: bool,
    pub plugin_id: String,
    pub install_count: Option<u64>,
}

/// Discover 详情页操作菜单
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
            Self::InstallUser => "Install (User scope)",
            Self::InstallProject => "Install (Project scope)",
            Self::BackToList => "Back to list",
        }
    }
}

/// Marketplace 条目（Marketplaces 视图用）
#[derive(Debug, Clone)]
pub struct MarketplaceViewEntry {
    pub name: String,
    pub source: rust_agent_middlewares::plugin::MarketplaceSource,
    pub source_label: String,
    pub plugin_count: usize,
    pub installed_count: usize,
    pub status: MarketplaceViewStatus,
    pub last_updated: Option<String>,
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

/// 插件条目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginItemType {
    Plugin,
    Mcp,
}

/// 面板中展示的插件条目
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub plugin_type: PluginItemType,
    pub marketplace: String,
    pub enabled: bool,
    pub scope: InstallScope,
    pub version: String,
    pub install_path: std::path::PathBuf,
    pub project_path: Option<String>,
    pub load_error: Option<String>,
    pub description: String,
    pub author: Option<String>,
    pub commands: Vec<String>,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub mcp_servers: Vec<String>,
}

/// 详情页操作菜单
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
            Self::ToggleEnabled => {
                if enabled {
                    "Disable plugin"
                } else {
                    "Enable plugin"
                }
            }
            Self::Uninstall => "Uninstall",
            Self::BackToList => "Back to plugin list",
        }
    }
}

/// 插件面板视图
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
            Self::Installed => "Installed",
            Self::Discover => "Discover",
            Self::Marketplaces => "Marketplaces",
            Self::Errors => "Errors",
        }
    }

    pub const ALL: [PluginPanelView; 4] = [
        PluginPanelView::Installed,
        PluginPanelView::Discover,
        PluginPanelView::Marketplaces,
        PluginPanelView::Errors,
    ];

    pub fn next(&mut self) {
        *self = match self {
            Self::Installed => Self::Discover,
            Self::Discover => Self::Marketplaces,
            Self::Marketplaces => Self::Errors,
            Self::Errors => Self::Installed,
        };
    }

    pub fn prev(&mut self) {
        *self = match self {
            Self::Installed => Self::Errors,
            Self::Discover => Self::Installed,
            Self::Marketplaces => Self::Discover,
            Self::Errors => Self::Marketplaces,
        };
    }
}

/// /plugin 面板状态
#[derive(Debug, Clone)]
pub struct PluginPanel {
    pub view: PluginPanelView,
    pub entries: Vec<PluginEntry>,
    pub installed_list: PanelList<PluginEntry>,
    pub confirm_delete: Option<String>,
    /// 详情视图：已进入时为 Some(entry_index)
    pub detail_index: Option<usize>,
    /// 详情页操作菜单光标
    pub detail_cursor: usize,

    // --- Discover 视图状态 ---
    pub discover_plugins: Vec<DiscoverPlugin>,
    pub discover_search: InputState,
    pub discover_searching: bool,
    pub discover_list: PanelList<DiscoverPlugin>,
    pub discover_loading: bool,
    pub discover_selected: HashSet<String>,
    pub discover_detail_index: Option<usize>,
    pub discover_detail_cursor: usize,

    // --- Marketplaces 视图状态 ---
    pub marketplace_entries: Vec<MarketplaceViewEntry>,
    pub marketplace_list: PanelList<MarketplaceViewEntry>,
    pub marketplace_confirm_delete: Option<usize>,
    pub marketplace_updating: HashSet<String>,
    /// 添加 marketplace 输入框
    pub add_marketplace_input: InputState,
    /// 是否处于添加 marketplace 模式
    pub add_marketplace_active: bool,

    // --- 安装/卸载进度 ---
    pub installing: HashSet<String>,
    pub uninstalling: HashSet<String>,
}

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
            discover_detail_index: None,
            discover_detail_cursor: 0,
            marketplace_entries: Vec::new(),
            marketplace_list: PanelList::new(),
            marketplace_confirm_delete: None,
            marketplace_updating: HashSet::new(),
            add_marketplace_input: InputState::new(),
            add_marketplace_active: false,
            installing: HashSet::new(),
            uninstalling: HashSet::new(),
        }
    }

    pub fn is_detail(&self) -> bool {
        self.detail_index.is_some()
            || self.discover_detail_index.is_some()
            || self.add_marketplace_active
    }

    /// 按搜索词过滤后的 Discover 插件列表
    pub fn discover_filtered_plugins(&self) -> Vec<&DiscoverPlugin> {
        let search = self.discover_search.value();
        if search.is_empty() {
            self.discover_plugins.iter().collect()
        } else {
            let query = search.to_lowercase();
            self.discover_plugins
                .iter()
                .filter(|p| {
                    p.name.to_lowercase().contains(&query)
                        || p.description.to_lowercase().contains(&query)
                        || p.marketplace.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    /// 获取当前光标处的 Discover 插件
    pub fn discover_current_plugin(&self) -> Option<&DiscoverPlugin> {
        let filtered = self.discover_filtered_plugins();
        filtered.get(self.discover_list.cursor()).copied()
    }

    /// 根据当前视图过滤后的可见条目索引列表
    pub fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| match self.view {
                PluginPanelView::Installed => true,
                PluginPanelView::Errors => e.load_error.is_some(),
                PluginPanelView::Discover | PluginPanelView::Marketplaces => false,
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn current_list_len(&self) -> usize {
        match self.view {
            PluginPanelView::Installed => self.installed_list.len(),
            PluginPanelView::Errors => {
                // Errors 视图过滤有 load_error 的条目
                self.entries
                    .iter()
                    .filter(|e| e.load_error.is_some())
                    .count()
            }
            PluginPanelView::Discover => self.discover_list.len(),
            PluginPanelView::Marketplaces => {
                // marketplace_cursor = 0 是 Add Marketplace，+ marketplace_entries.len()
                self.marketplace_entries.len() + 1
            }
        }
    }

    /// 根据当前视图返回 cursor
    pub fn cursor(&self) -> usize {
        match self.view {
            PluginPanelView::Installed => self.installed_list.cursor(),
            PluginPanelView::Discover => self.discover_list.cursor(),
            PluginPanelView::Marketplaces => self.marketplace_list.cursor(),
            PluginPanelView::Errors => self.installed_list.cursor(),
        }
    }

    /// 根据当前视图返回 scroll_offset
    pub fn scroll_offset(&self) -> u16 {
        match self.view {
            PluginPanelView::Installed => self.installed_list.scroll_offset(),
            PluginPanelView::Discover => self.discover_list.scroll_offset(),
            PluginPanelView::Marketplaces => self.marketplace_list.scroll_offset(),
            PluginPanelView::Errors => self.installed_list.scroll_offset(),
        }
    }

    /// 根据当前视图设置 scroll_offset
    pub fn set_scroll_offset(&mut self, offset: u16) {
        match self.view {
            PluginPanelView::Installed => self.installed_list.set_scroll_offset(offset),
            PluginPanelView::Discover => self.discover_list.set_scroll_offset(offset),
            PluginPanelView::Marketplaces => self.marketplace_list.set_scroll_offset(offset),
            PluginPanelView::Errors => self.installed_list.set_scroll_offset(offset),
        }
    }

    pub fn selected_entry(&self) -> Option<&PluginEntry> {
        let indices = self.visible_indices();
        indices
            .get(self.cursor())
            .and_then(|&i| self.entries.get(i))
    }

    /// 切换视图后同步当前视图的 PanelList items
    fn sync_current_view_items(&mut self) {
        match self.view {
            PluginPanelView::Installed => {
                // installed_list items 已在 new() 时设置，无需同步
            }
            PluginPanelView::Errors => {
                // Errors 视图：只显示有 load_error 的 entries
                let error_entries: Vec<PluginEntry> = self
                    .entries
                    .iter()
                    .filter(|e| e.load_error.is_some())
                    .cloned()
                    .collect();
                self.installed_list.set_items(error_entries);
            }
            PluginPanelView::Discover => {
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
            }
            PluginPanelView::Marketplaces => {
                // marketplace_list items 在 open_plugin_panel 中设置
            }
        }
    }
}

// ─── PanelComponent 实现 ──────────────────────────────────────────────────────

impl PanelComponent for PluginPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Plugin
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // 1. confirm_delete 模式
        if self.confirm_delete.is_some() {
            return self.handle_confirm_delete(input, ctx);
        }

        // 2. discover_searching 模式
        if self.discover_searching {
            return self.handle_discover_searching(input, ctx);
        }

        // 3. discover_detail_index 模式
        if self.discover_detail_index.is_some() {
            return self.handle_discover_detail(input, ctx);
        }

        // 4. detail_index 模式
        if self.detail_index.is_some() {
            return self.handle_installed_detail(input, ctx);
        }

        // 5. 列表视图（按 PluginPanelView 分发）
        match self.view {
            PluginPanelView::Discover => self.handle_discover_list(input, ctx),
            PluginPanelView::Marketplaces => self.handle_marketplaces_list(input, ctx),
            PluginPanelView::Installed | PluginPanelView::Errors => {
                self.handle_installed_list(input, ctx)
            }
        }
    }

    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        if self.add_marketplace_active {
            for ch in text.chars() {
                self.add_marketplace_input.insert(ch);
            }
            return EventResult::Consumed;
        }
        if self.discover_searching {
            for ch in text.chars() {
                self.discover_search.insert(ch);
            }
            self.discover_list.set_items(
                self.discover_filtered_plugins()
                    .into_iter()
                    .cloned()
                    .collect(),
            );
            return EventResult::Consumed;
        }
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        match self.view {
            PluginPanelView::Installed => self.installed_list.handle_scroll(lines, 10),
            PluginPanelView::Discover => self.discover_list.handle_scroll(lines, 10),
            PluginPanelView::Marketplaces => self.marketplace_list.handle_scroll(lines, 10),
            PluginPanelView::Errors => self.installed_list.handle_scroll(lines, 10),
        }
        EventResult::Consumed
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        screen_height * 70 / 100
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::plugin::render_plugin_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.confirm_delete.is_some() {
            return vec![
                ("Enter", "\u{786e}\u{8ba4}\u{5378}\u{8f7d}"),
                ("\u{5176}\u{4ed6}\u{952e}", "\u{53d6}\u{6d88}"),
            ];
        }
        if self.marketplace_confirm_delete.is_some() {
            return vec![
                ("Enter", "\u{786e}\u{8ba4}\u{5220}\u{9664}"),
                ("Esc", "\u{53d6}\u{6d88}"),
            ];
        }
        if self.add_marketplace_active {
            return vec![("Enter", "\u{6dfb}\u{52a0}"), ("Esc", "\u{53d6}\u{6d88}")];
        }
        if self.discover_searching {
            return vec![
                ("Esc/\u{2191}\u{2193}", "\u{9000}\u{51fa}\u{641c}\u{7d22}"),
                ("\u{2190}\u{2192}", "Tab"),
                ("Enter", "\u{5b89}\u{88c5}"),
                ("Backspace", "\u{5220}\u{9664}"),
            ];
        }
        if self.discover_detail_index.is_some() {
            return vec![
                ("\u{2191}\u{2193}", "\u{5bfc}\u{822a}"),
                ("Enter", "\u{6267}\u{884c}"),
                ("Esc", "\u{8fd4}\u{56de}\u{5217}\u{8868}"),
            ];
        }
        if self.detail_index.is_some() {
            return vec![
                ("\u{2191}\u{2193}", "\u{5bfc}\u{822a}"),
                ("Enter", "\u{6267}\u{884c}"),
                ("Esc", "\u{8fd4}\u{56de}\u{5217}\u{8868}"),
            ];
        }
        match self.view {
            PluginPanelView::Discover => vec![
                ("\u{2191}\u{2193}", "\u{9009}\u{62e9}"),
                ("\u{8f93}\u{5165}", "\u{641c}\u{7d22}"),
                ("Enter", "\u{5b89}\u{88c5}"),
                ("\u{2190}\u{2192}/Tab", "Tab"),
                ("Esc", "\u{5173}\u{95ed}"),
            ],
            PluginPanelView::Marketplaces => vec![
                ("\u{2191}\u{2193}", "\u{9009}\u{62e9}"),
                ("Enter", "\u{6dfb}\u{52a0}/\u{66f4}\u{65b0}"),
                ("Backspace", "\u{79fb}\u{9664}"),
                ("\u{2190}\u{2192}/Tab", "Tab"),
                ("Esc", "\u{5173}\u{95ed}"),
            ],
            PluginPanelView::Installed | PluginPanelView::Errors => vec![
                ("\u{2191}\u{2193}", "\u{5bfc}\u{822a}"),
                ("Space", "\u{5207}\u{6362}"),
                ("Enter", "\u{8be6}\u{60c5}"),
                ("\u{2190}\u{2192}/Tab", "Tab"),
                ("Esc", "\u{5173}\u{95ed}"),
            ],
        }
    }
}

impl PluginPanel {
    // ─── 内部 handle_key 分发方法 ─────────────────────────────────────────────

    fn handle_confirm_delete(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input {
            Input {
                key: Key::Enter, ..
            } => {
                let (plugin_id, project_path) = if let Some(id) = self.confirm_delete.clone() {
                    let entry = self.entries.iter().find(|e| e.id == id);
                    let project_path = entry.and_then(|e| e.project_path.clone());
                    (Some(id), project_path)
                } else {
                    (None, None)
                };

                if let Some(plugin_id) = plugin_id {
                    self.uninstalling.insert(plugin_id.clone());
                    self.confirm_delete = None;

                    let tx = ctx.services.bg_event_tx.clone();
                    let claude_dir = rust_agent_middlewares::plugin::claude_home();
                    let project_dir = project_path.map(std::path::PathBuf::from);
                    tokio::spawn(async move {
                        let result = rust_agent_middlewares::plugin::uninstall_plugin(
                            &plugin_id,
                            &claude_dir,
                            project_dir.as_deref(),
                        )
                        .await;
                        let success = result.is_ok();
                        let message = if let Err(e) = result {
                            format!("\u{5378}\u{8f7d}\u{5931}\u{8d25}: {e}")
                        } else {
                            "\u{5378}\u{8f7d}\u{6210}\u{529f}".to_string()
                        };
                        let _ = tx.try_send(super::AgentEvent::PluginActionCompleted {
                            plugin_id,
                            action: "uninstall".to_string(),
                            success,
                            message,
                        });
                    });
                } else {
                    self.confirm_delete = None;
                }
                EventResult::Consumed
            }
            _ => {
                self.confirm_delete = None;
                EventResult::Consumed
            }
        }
    }

    fn handle_discover_searching(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input {
                key: Key::Char(c), ..
            } => {
                self.discover_search.insert(c);
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                self.discover_search.backspace();
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.discover_searching = false;
                self.discover_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.discover_searching = false;
                self.discover_list.move_cursor(1);
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input {
                key: Key::Right, ..
            } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.spawn_install_current(ctx);
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_discover_detail(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input {
            Input { key: Key::Up, .. } => {
                if self.discover_detail_cursor > 0 {
                    self.discover_detail_cursor -= 1;
                }
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                let max = DiscoverDetailAction::ALL.len().saturating_sub(1);
                if self.discover_detail_cursor < max {
                    self.discover_detail_cursor += 1;
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let action = DiscoverDetailAction::ALL
                    .get(self.discover_detail_cursor)
                    .copied();
                let plugin_idx = self.discover_detail_index;
                match action {
                    Some(DiscoverDetailAction::InstallUser) => {
                        if let Some(dp) = plugin_idx.and_then(|i| self.discover_plugins.get(i)) {
                            let name = dp.name.clone();
                            let marketplace = dp.marketplace.clone();
                            let plugin_id = format!("{}@{}", name, marketplace);
                            self.installing.insert(plugin_id.clone());
                            let project_dir = std::path::PathBuf::from(&ctx.services.cwd);
                            let claude_dir = rust_agent_middlewares::plugin::claude_home();
                            let cache_dir =
                                rust_agent_middlewares::plugin::marketplaces_cache_dir();
                            let tx = ctx.services.bg_event_tx.clone();
                            tokio::spawn(async move {
                                let result = rust_agent_middlewares::plugin::install_plugin(
                                    &name,
                                    &marketplace,
                                    InstallScope::User,
                                    &cache_dir,
                                    &claude_dir,
                                    Some(&project_dir),
                                )
                                .await;
                                let _ = tx.try_send(super::AgentEvent::PluginActionCompleted {
                                    plugin_id: format!("{}@{}", name, marketplace),
                                    action: "install".to_string(),
                                    success: result.is_ok(),
                                    message: result
                                        .map(|_| String::new())
                                        .unwrap_or_else(|e| e.to_string()),
                                });
                            });
                        }
                        self.discover_detail_index = None;
                        self.discover_detail_cursor = 0;
                    }
                    Some(DiscoverDetailAction::InstallProject) => {
                        if let Some(dp) = plugin_idx.and_then(|i| self.discover_plugins.get(i)) {
                            let name = dp.name.clone();
                            let marketplace = dp.marketplace.clone();
                            let plugin_id = format!("{}@{}", name, marketplace);
                            self.installing.insert(plugin_id.clone());
                            let project_dir = std::path::PathBuf::from(&ctx.services.cwd);
                            let claude_dir = rust_agent_middlewares::plugin::claude_home();
                            let cache_dir =
                                rust_agent_middlewares::plugin::marketplaces_cache_dir();
                            let tx = ctx.services.bg_event_tx.clone();
                            tokio::spawn(async move {
                                let result = rust_agent_middlewares::plugin::install_plugin(
                                    &name,
                                    &marketplace,
                                    InstallScope::Project,
                                    &cache_dir,
                                    &claude_dir,
                                    Some(&project_dir),
                                )
                                .await;
                                let _ = tx.try_send(super::AgentEvent::PluginActionCompleted {
                                    plugin_id: format!("{}@{}", name, marketplace),
                                    action: "install".to_string(),
                                    success: result.is_ok(),
                                    message: result
                                        .map(|_| String::new())
                                        .unwrap_or_else(|e| e.to_string()),
                                });
                            });
                        }
                        self.discover_detail_index = None;
                        self.discover_detail_cursor = 0;
                    }
                    Some(DiscoverDetailAction::BackToList) => {
                        self.discover_detail_index = None;
                        self.discover_detail_cursor = 0;
                    }
                    None => {}
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.discover_detail_index = None;
                self.discover_detail_cursor = 0;
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_installed_detail(&mut self, input: Input, ctx: &PanelContext<'_>) -> EventResult {
        match input {
            Input { key: Key::Up, .. } => {
                if self.detail_cursor > 0 {
                    self.detail_cursor -= 1;
                }
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                let max = DetailAction::ALL.len().saturating_sub(1);
                if self.detail_cursor < max {
                    self.detail_cursor += 1;
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                self.do_detail_action(ctx);
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.detail_index = None;
                self.detail_cursor = 0;
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_installed_list(&mut self, input: Input, ctx: &PanelContext<'_>) -> EventResult {
        match input {
            Input {
                key: Key::Right, ..
            }
            | Input { key: Key::Tab, .. } => {
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.installed_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.installed_list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Char(' '),
                ..
            } => {
                if let Some(&entry_idx) = self.visible_indices().get(self.installed_list.cursor()) {
                    if let Some(entry) = self.entries.get_mut(entry_idx) {
                        entry.enabled = !entry.enabled;
                    }
                }
                self.persist_enabled_state(ctx.services.claude_settings_override.as_ref());
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                if let Some(&entry_idx) = self.visible_indices().get(self.installed_list.cursor()) {
                    self.detail_index = Some(entry_idx);
                    self.detail_cursor = 0;
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }

    fn handle_discover_list(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input {
            Input {
                key: Key::Right, ..
            }
            | Input { key: Key::Tab, .. } => {
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.discover_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.discover_list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Char(c), ..
            } => {
                self.discover_searching = true;
                self.discover_search.insert(c);
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                self.spawn_install_current(ctx);
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }

    fn handle_marketplaces_list(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        // marketplace_confirm_delete 子状态
        if self.marketplace_confirm_delete.is_some() {
            return self.handle_marketplace_confirm_delete(input, ctx);
        }

        // add_marketplace_active 子状态
        if self.add_marketplace_active {
            return self.handle_marketplace_add(input, ctx);
        }

        // 默认列表视图
        match input {
            Input {
                key: Key::Right, ..
            }
            | Input { key: Key::Tab, .. } => {
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.marketplace_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.marketplace_list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                if self.marketplace_list.cursor() == 0 {
                    // Add Marketplace
                    self.add_marketplace_input = InputState::new();
                    self.add_marketplace_active = true;
                } else if let Some(entry) = self
                    .marketplace_entries
                    .get(self.marketplace_list.cursor() - 1)
                {
                    let name = entry.name.clone();
                    let source = entry.source.clone();
                    self.marketplace_updating.insert(name.clone());
                    let name_for_msg = name.clone();
                    let source_for_update = source.clone();
                    let tx = ctx.services.bg_event_tx.clone();
                    tokio::spawn(async move {
                        let result =
                            rust_agent_middlewares::plugin::marketplace::refresh_marketplace(
                                &source, &name,
                            )
                            .await;
                        match result {
                            Ok((_manifest, install_location)) => {
                                if let Ok(mut marketplaces) =
                                    rust_agent_middlewares::plugin::load_known_marketplaces(None)
                                {
                                    if let Some(km) = marketplaces
                                        .iter_mut()
                                        .find(|km| km.source == source_for_update)
                                    {
                                        km.install_location = install_location;
                                        km.last_updated = chrono::Utc::now().to_rfc3339();
                                        let _ =
                                            rust_agent_middlewares::plugin::save_known_marketplaces(
                                                &marketplaces,
                                                None,
                                            );
                                    }
                                }
                                let _ = tx
                                    .send(super::AgentEvent::PluginActionCompleted {
                                        plugin_id: name.clone(),
                                        action: "refresh".to_string(),
                                        success: true,
                                        message: format!(
                                            "Marketplace '{}' \u{5df2}\u{66f4}\u{65b0}",
                                            name
                                        ),
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(super::AgentEvent::PluginActionCompleted {
                                        plugin_id: name.clone(),
                                        action: "refresh".to_string(),
                                        success: false,
                                        message: format!("\u{66f4}\u{65b0}\u{5931}\u{8d25}: {}", e),
                                    })
                                    .await;
                            }
                        }
                    });
                    ctx.session_mgr.sessions[ctx.session_mgr.active]
                        .messages
                        .push_system_note(format!(
                            "\u{6b63}\u{5728}\u{66f4}\u{65b0} marketplace: {}",
                            name_for_msg
                        ));
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                if self.marketplace_list.cursor() > 0 {
                    let idx = self.marketplace_list.cursor() - 1;
                    if self.marketplace_entries.get(idx).is_some() {
                        self.marketplace_confirm_delete = Some(idx);
                    }
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }

    fn handle_marketplace_confirm_delete(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input { key: Key::Esc, .. } => {
                self.marketplace_confirm_delete = None;
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                if let Some(idx) = self.marketplace_confirm_delete.take() {
                    if let Some(entry) = self.marketplace_entries.get(idx) {
                        let name = entry.name.clone();
                        self.marketplace_entries.remove(idx);
                        self.marketplace_list
                            .set_items(self.marketplace_entries.clone());

                        // Persist delete
                        if let Err(e) = self.persist_marketplace_delete(&name) {
                            ctx.session_mgr.sessions[ctx.session_mgr.active]
                                .messages
                                .push_system_note(format!(
                                    "\u{5220}\u{9664}\u{5931}\u{8d25}: {}",
                                    e
                                ));
                        }
                    }
                }
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_marketplace_add(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input {
            Input { key: Key::Esc, .. } => {
                self.add_marketplace_active = false;
                self.add_marketplace_input = InputState::new();
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let input_str = self.add_marketplace_input.value().trim().to_string();
                self.add_marketplace_active = false;
                self.add_marketplace_input = InputState::new();
                if !input_str.is_empty() {
                    if let Err(e) = self.persist_marketplace_add(&input_str, ctx) {
                        ctx.session_mgr.sessions[ctx.session_mgr.active]
                            .messages
                            .push_system_note(format!("\u{6dfb}\u{52a0}\u{5931}\u{8d25}: {}", e));
                    }
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                self.add_marketplace_input.backspace();
                EventResult::Consumed
            }
            Input {
                key: Key::Char(ch), ..
            } => {
                self.add_marketplace_input.insert(ch);
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    // ─── 辅助方法 ──────────────────────────────────────────────────────────

    /// 异步安装 Discover 视图中当前光标处的插件
    fn spawn_install_current(&mut self, ctx: &PanelContext<'_>) {
        let plugin = match self.discover_current_plugin() {
            Some(p) => p,
            None => return,
        };
        let name = plugin.name.clone();
        let marketplace = plugin.marketplace.clone();
        let plugin_id = plugin.plugin_id.clone();
        self.installing.insert(plugin_id.clone());

        let project_dir = std::path::PathBuf::from(&ctx.services.cwd);
        let claude_dir = rust_agent_middlewares::plugin::claude_home();
        let cache_dir = rust_agent_middlewares::plugin::marketplaces_cache_dir();
        let tx = ctx.services.bg_event_tx.clone();
        tokio::spawn(async move {
            let result = rust_agent_middlewares::plugin::install_plugin(
                &name,
                &marketplace,
                InstallScope::User,
                &cache_dir,
                &claude_dir,
                Some(&project_dir),
            )
            .await;
            let _ = tx.try_send(super::AgentEvent::PluginActionCompleted {
                plugin_id,
                action: "install".to_string(),
                success: result.is_ok(),
                message: result
                    .map(|_| String::new())
                    .unwrap_or_else(|e| e.to_string()),
            });
        });
    }

    /// 执行详情页当前操作（ToggleEnabled/Uninstall/BackToList）
    fn do_detail_action(&mut self, ctx: &PanelContext<'_>) {
        let action = DetailAction::ALL.get(self.detail_cursor).copied();
        let entry_idx = self.detail_index;
        match action {
            Some(DetailAction::ToggleEnabled) => {
                if let Some(idx) = entry_idx {
                    if let Some(entry) = self.entries.get_mut(idx) {
                        entry.enabled = !entry.enabled;
                    }
                }
                self.persist_enabled_state(ctx.services.claude_settings_override.as_ref());
            }
            Some(DetailAction::Uninstall) => {
                if let Some(idx) = entry_idx {
                    let id = self.entries.get(idx).map(|e| e.id.clone());
                    if let Some(id) = id {
                        self.confirm_delete = Some(id);
                    }
                }
            }
            Some(DetailAction::BackToList) => {
                self.detail_index = None;
                self.detail_cursor = 0;
            }
            None => {}
        }
    }

    /// 持久化 enabled 状态到 Claude settings
    fn persist_enabled_state(&self, claude_settings_override: Option<&std::path::PathBuf>) {
        let states: Vec<(String, bool)> = self
            .entries
            .iter()
            .map(|e| (e.id.clone(), e.enabled))
            .collect();
        if let Err(e) = rust_agent_middlewares::plugin::save_claude_settings_enabled_plugins(
            &states,
            claude_settings_override.map(|p| p.as_path()),
        ) {
            tracing::warn!(error = %e, "\u{4fdd}\u{5b58} enabledPlugins \u{5931}\u{8d25}");
        }
    }

    /// 持久化删除 marketplace
    fn persist_marketplace_delete(&self, name: &str) -> anyhow::Result<()> {
        use rust_agent_middlewares::plugin::{
            load_known_marketplaces, save_known_marketplaces, MarketplaceSource,
        };
        let marketplaces = load_known_marketplaces(None).unwrap_or_default();
        let filtered: Vec<_> = marketplaces
            .into_iter()
            .filter(|km| {
                let km_name = match &km.source {
                    MarketplaceSource::GitHub { repo } => {
                        repo.split('/').next_back().unwrap_or(repo).to_string()
                    }
                    MarketplaceSource::Git { url } => url
                        .split('/')
                        .next_back()
                        .and_then(|s| s.strip_suffix(".git"))
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Url { url } => {
                        let last = url.split('/').next_back().unwrap_or("marketplace");
                        last.strip_suffix(".json").unwrap_or(last).to_string()
                    }
                    MarketplaceSource::File { path } => std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Directory { path } => std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Npm { package } => {
                        package.split('@').next().unwrap_or(package).to_string()
                    }
                };
                km_name != name
            })
            .collect();
        save_known_marketplaces(&filtered, None)?;
        Ok(())
    }

    /// 持久化添加 marketplace
    fn persist_marketplace_add(
        &mut self,
        input: &str,
        ctx: &mut PanelContext<'_>,
    ) -> anyhow::Result<()> {
        use rust_agent_middlewares::plugin::{
            load_known_marketplaces, parse_marketplace_input, save_known_marketplaces,
            KnownMarketplace, MarketplaceManager,
        };
        let source = parse_marketplace_input(input)
            .map_err(|e| anyhow::anyhow!("\u{89e3}\u{6790}\u{5931}\u{8d25}: {}", e))?;
        let mut marketplaces = load_known_marketplaces(None).unwrap_or_default();
        for existing in &marketplaces {
            if existing.source == source {
                anyhow::bail!("Marketplace \u{5df2}\u{5b58}\u{5728}");
            }
        }
        let name = MarketplaceManager::extract_name(&source);
        let new_entry = KnownMarketplace {
            source: source.clone(),
            install_location: String::new(),
            auto_update: false,
            last_updated: String::new(),
        };
        marketplaces.push(new_entry);
        save_known_marketplaces(&marketplaces, None)?;

        ctx.session_mgr.sessions[ctx.session_mgr.active]
            .messages
            .push_system_note(format!(
                "Marketplace \u{5df2}\u{6dfb}\u{52a0}: {} (\u{6b63}\u{5728}\u{83b7}\u{53d6}\u{5185}\u{5bb9}...)",
                name
            ));

        // Add placeholder entry to marketplace_entries
        self.marketplace_entries.push(MarketplaceViewEntry {
            name: name.clone(),
            source: source.clone(),
            source_label: format!("{:?}", source),
            plugin_count: 0,
            installed_count: 0,
            status: MarketplaceViewStatus::Fetching,
            last_updated: None,
            auto_update: false,
        });

        // Spawn background refresh
        let name_clone = name.clone();
        let tx = ctx.services.bg_event_tx.clone();
        tokio::spawn(async move {
            use rust_agent_middlewares::plugin::marketplace::refresh_marketplace;
            match refresh_marketplace(&source, &name_clone).await {
                Ok((_manifest, install_location)) => {
                    if let Ok(mut mkt_places) =
                        rust_agent_middlewares::plugin::load_known_marketplaces(None)
                    {
                        if let Some(entry) = mkt_places.iter_mut().find(|km| km.source == source) {
                            entry.install_location = install_location;
                            entry.last_updated = chrono::Utc::now().to_rfc3339();
                            let _ = rust_agent_middlewares::plugin::save_known_marketplaces(
                                &mkt_places,
                                None,
                            );
                        }
                    }
                    let _ = tx
                        .send(super::AgentEvent::PluginActionCompleted {
                            plugin_id: name_clone.clone(),
                            action: "add".to_string(),
                            success: true,
                            message: format!(
                                "Marketplace '{}' \u{5185}\u{5bb9}\u{5df2}\u{83b7}\u{53d6}",
                                name_clone
                            ),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(super::AgentEvent::PluginActionCompleted {
                            plugin_id: name_clone.clone(),
                            action: "add".to_string(),
                            success: false,
                            message: format!(
                                "\u{83b7}\u{53d6}\u{5185}\u{5bb9}\u{5931}\u{8d25}: {}",
                                e
                            ),
                        })
                        .await;
                }
            }
        });

        Ok(())
    }
}

// ─── App 操作方法 ────────────────────────────────────────────────────────────

impl App {
    pub fn plugin_panel_move_up(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            match panel.view {
                PluginPanelView::Installed | PluginPanelView::Errors => {
                    panel.installed_list.move_cursor(-1);
                }
                PluginPanelView::Discover => {
                    panel.discover_list.move_cursor(-1);
                }
                PluginPanelView::Marketplaces => {
                    panel.marketplace_list.move_cursor(-1);
                }
            }
        }
    }

    pub fn plugin_panel_move_down(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            match panel.view {
                PluginPanelView::Installed | PluginPanelView::Errors => {
                    panel.installed_list.move_cursor(1);
                }
                PluginPanelView::Discover => {
                    panel.discover_list.move_cursor(1);
                }
                PluginPanelView::Marketplaces => {
                    panel.marketplace_list.move_cursor(1);
                }
            }
        }
    }

    pub fn plugin_panel_tab(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.view.next();
            panel.sync_current_view_items();
        }
    }

    pub fn plugin_panel_shift_tab(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.view.prev();
            panel.sync_current_view_items();
        }
    }

    pub fn plugin_panel_close(&mut self) {
        self.global_panels.close();
    }

    pub fn plugin_panel_request_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if let Some(entry) = panel.selected_entry() {
                panel.confirm_delete = Some(entry.id.clone());
            }
        }
    }

    pub fn plugin_panel_cancel_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.confirm_delete = None;
        }
    }

    pub fn plugin_panel_confirm_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if let Some(id) = panel.confirm_delete.take() {
                panel.entries.retain(|p| p.id != id);
                panel.installed_list.set_items(panel.entries.clone());
            }
        }
    }

    pub fn plugin_panel_toggle_enabled(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if let Some(entry_idx) = panel.visible_indices().get(panel.cursor()).copied() {
                if let Some(entry) = panel.entries.get_mut(entry_idx) {
                    entry.enabled = !entry.enabled;
                    self.persist_plugin_enabled_state();
                }
            }
        }
    }

    /// 将当前面板中所有插件的启用状态持久化到 ~/.claude/settings.json
    fn persist_plugin_enabled_state(&self) {
        if let Some(panel) = self.global_panels.get::<PluginPanel>() {
            let states: Vec<(String, bool)> = panel
                .entries
                .iter()
                .map(|e| (e.id.clone(), e.enabled))
                .collect();
            if let Err(e) = rust_agent_middlewares::plugin::save_claude_settings_enabled_plugins(
                &states,
                self.services.claude_settings_override.as_deref(),
            ) {
                tracing::warn!(error = %e, "保存 enabledPlugins 失败");
            }
        }
    }

    /// 进入选中插件的详情视图
    pub fn plugin_panel_enter_detail(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if let Some(&entry_idx) = panel.visible_indices().get(panel.cursor()) {
                panel.detail_index = Some(entry_idx);
                panel.detail_cursor = 0;
            }
        }
    }

    /// 退出详情视图回到列表
    pub fn plugin_panel_exit_detail(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.detail_index = None;
            panel.detail_cursor = 0;
        }
    }

    /// 详情页操作菜单上移
    pub fn plugin_panel_detail_up(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if panel.detail_cursor > 0 {
                panel.detail_cursor -= 1;
            }
        }
    }

    /// 详情页操作菜单下移
    pub fn plugin_panel_detail_down(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            let max = DetailAction::ALL.len().saturating_sub(1);
            if panel.detail_cursor < max {
                panel.detail_cursor += 1;
            }
        }
    }

    /// 执行详情页当前操作
    pub fn plugin_panel_detail_action(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            let action = DetailAction::ALL.get(panel.detail_cursor).copied();
            let entry_idx = panel.detail_index;
            match action {
                Some(DetailAction::ToggleEnabled) => {
                    if let Some(idx) = entry_idx {
                        if let Some(entry) = panel.entries.get_mut(idx) {
                            entry.enabled = !entry.enabled;
                        }
                        // 面板引用已释放，调用保存
                        let states: Vec<(String, bool)> = panel
                            .entries
                            .iter()
                            .map(|e| (e.id.clone(), e.enabled))
                            .collect();
                        if let Err(e) =
                            rust_agent_middlewares::plugin::save_claude_settings_enabled_plugins(
                                &states,
                                self.services.claude_settings_override.as_deref(),
                            )
                        {
                            tracing::warn!(error = %e, "保存 enabledPlugins 失败");
                        }
                    }
                }
                Some(DetailAction::Uninstall) => {
                    if let Some(idx) = entry_idx {
                        let id = panel.entries.get(idx).map(|e| e.id.clone());
                        if let Some(id) = id {
                            panel.confirm_delete = Some(id);
                        }
                    }
                }
                Some(DetailAction::BackToList) => {
                    panel.detail_index = None;
                    panel.detail_cursor = 0;
                }
                None => {}
            }
        }
    }

    // ─── Discover 视图操作 ─────────────────────────────────────────────────────

    pub fn discover_move_up(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_list.move_cursor(-1);
        }
    }

    pub fn discover_move_down(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_list.move_cursor(1);
        }
    }

    pub fn discover_enter_search(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_searching = true;
        }
    }

    pub fn discover_exit_search(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_searching = false;
            panel.discover_list.set_items(
                panel
                    .discover_filtered_plugins()
                    .into_iter()
                    .cloned()
                    .collect(),
            );
        }
    }

    pub fn discover_search_input(&mut self, ch: char) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_search.insert(ch);
            panel.discover_list.set_items(
                panel
                    .discover_filtered_plugins()
                    .into_iter()
                    .cloned()
                    .collect(),
            );
        }
    }

    pub fn discover_search_backspace(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_search.backspace();
            panel.discover_list.set_items(
                panel
                    .discover_filtered_plugins()
                    .into_iter()
                    .cloned()
                    .collect(),
            );
        }
    }

    pub fn discover_enter_detail(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if panel.discover_current_plugin().is_some() {
                panel.discover_detail_index = Some(panel.discover_list.cursor());
                panel.discover_detail_cursor = 0;
            }
        }
    }

    pub fn discover_exit_detail(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.discover_detail_index = None;
            panel.discover_detail_cursor = 0;
        }
    }

    pub fn discover_detail_up(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if panel.discover_detail_cursor > 0 {
                panel.discover_detail_cursor -= 1;
            }
        }
    }

    pub fn discover_detail_down(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            let max = DiscoverDetailAction::ALL.len().saturating_sub(1);
            if panel.discover_detail_cursor < max {
                panel.discover_detail_cursor += 1;
            }
        }
    }

    /// 执行 Discover 详情页操作（安装或返回）
    pub fn discover_detail_action(&mut self) -> Option<(String, String, InstallScope)> {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            let action = DiscoverDetailAction::ALL
                .get(panel.discover_detail_cursor)
                .copied();
            let plugin_idx = panel.discover_detail_index;
            match action {
                Some(DiscoverDetailAction::InstallUser) => {
                    if let Some(dp) = plugin_idx.and_then(|i| panel.discover_plugins.get(i)) {
                        return Some((dp.name.clone(), dp.marketplace.clone(), InstallScope::User));
                    }
                }
                Some(DiscoverDetailAction::InstallProject) => {
                    if let Some(dp) = plugin_idx.and_then(|i| panel.discover_plugins.get(i)) {
                        return Some((
                            dp.name.clone(),
                            dp.marketplace.clone(),
                            InstallScope::Project,
                        ));
                    }
                }
                Some(DiscoverDetailAction::BackToList) => {
                    panel.discover_detail_index = None;
                    panel.discover_detail_cursor = 0;
                }
                None => {}
            }
        }
        None
    }

    // ─── Marketplaces 视图操作 ──────────────────────────────────────────────────

    pub fn marketplace_move_up(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.marketplace_list.move_cursor(-1);
        }
    }

    pub fn marketplace_move_down(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            // cursor = 0 是 Add Marketplace，最大值是 marketplace_entries.len()
            let max = panel.marketplace_entries.len();
            if panel.marketplace_list.cursor() < max {
                panel.marketplace_list.move_cursor(1);
            }
        }
    }

    /// 检查当前是否选中了 "Add Marketplace" 选项
    pub fn marketplace_is_add_selected(&self) -> bool {
        self.global_panels
            .get::<PluginPanel>()
            .map(|p| p.marketplace_list.cursor() == 0)
            .unwrap_or(false)
    }

    /// 获取当前选中的 marketplace 名称（如果选中 Add Marketplace 则返回 None）
    pub fn marketplace_current_name(&self) -> Option<String> {
        self.global_panels
            .get::<PluginPanel>()
            .filter(|p| p.marketplace_list.cursor() > 0)
            .and_then(|p| p.marketplace_entries.get(p.marketplace_list.cursor() - 1))
            .map(|m| m.name.clone())
    }

    /// 请求删除当前 marketplace（进入确认状态）
    pub fn marketplace_request_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            // cursor = 0 是 Add Marketplace，不能删除
            if panel.marketplace_list.cursor() > 0 {
                let idx = panel.marketplace_list.cursor() - 1;
                if panel.marketplace_entries.get(idx).is_some() {
                    panel.marketplace_confirm_delete = Some(idx);
                }
            }
        }
    }

    /// 取消删除 marketplace
    pub fn marketplace_cancel_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.marketplace_confirm_delete = None;
        }
    }

    /// 确认删除当前 marketplace，返回要删除的 marketplace 名称
    pub fn marketplace_confirm_delete(&mut self) -> Option<String> {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            if let Some(idx) = panel.marketplace_confirm_delete.take() {
                if let Some(entry) = panel.marketplace_entries.get(idx) {
                    let name = entry.name.clone();
                    // 从列表中移除
                    panel.marketplace_entries.remove(idx);
                    panel
                        .marketplace_list
                        .set_items(panel.marketplace_entries.clone());
                    return Some(name);
                }
            }
        }
        None
    }

    /// 请求更新当前 marketplace（添加到 updating 集合）
    pub fn marketplace_request_update(&mut self) -> Option<String> {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            // cursor = 0 是 Add Marketplace，不能更新
            if panel.marketplace_list.cursor() > 0 {
                let idx = panel.marketplace_list.cursor() - 1;
                if let Some(entry) = panel.marketplace_entries.get(idx) {
                    let name = entry.name.clone();
                    panel.marketplace_updating.insert(name.clone());
                    return Some(name);
                }
            }
        }
        None
    }

    /// 请求更新当前 marketplace，返回名称和 source
    pub fn marketplace_request_update_with_source(
        &mut self,
    ) -> Option<(String, rust_agent_middlewares::plugin::MarketplaceSource)> {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            // cursor = 0 是 Add Marketplace，不能更新
            if panel.marketplace_list.cursor() > 0 {
                let idx = panel.marketplace_list.cursor() - 1;
                if let Some(entry) = panel.marketplace_entries.get(idx) {
                    let name = entry.name.clone();
                    let source = entry.source.clone();
                    panel.marketplace_updating.insert(name.clone());
                    return Some((name, source));
                }
            }
        }
        None
    }

    /// 标记 marketplace 更新完成
    pub fn marketplace_update_done(&mut self, name: &str) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.marketplace_updating.remove(name);
        }
    }

    /// 进入添加 marketplace 模式
    pub fn marketplace_enter_add(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.add_marketplace_input = InputState::new();
            panel.add_marketplace_active = true;
        }
    }

    /// 退出添加 marketplace 模式
    pub fn marketplace_exit_add(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.add_marketplace_active = false;
            panel.add_marketplace_input = InputState::new();
        }
    }

    /// 添加 marketplace 输入字符
    pub fn marketplace_add_input(&mut self, ch: char) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.add_marketplace_input.insert(ch);
        }
    }

    /// 添加 marketplace 退格
    pub fn marketplace_add_backspace(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.add_marketplace_input.backspace();
        }
    }

    /// 确认添加 marketplace，返回输入的 source 字符串
    pub fn marketplace_add_confirm(&mut self) -> Option<String> {
        if let Some(panel) = self.global_panels.get_mut::<PluginPanel>() {
            let input = panel.add_marketplace_input.value().trim().to_string();
            panel.add_marketplace_active = false;
            panel.add_marketplace_input = InputState::new();
            if input.is_empty() {
                None
            } else {
                Some(input)
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, name: &str, enabled: bool) -> PluginEntry {
        PluginEntry {
            id: id.into(),
            name: name.into(),
            plugin_type: PluginItemType::Plugin,
            marketplace: "test".into(),
            enabled,
            scope: InstallScope::User,
            version: "1.0.0".into(),
            install_path: std::path::PathBuf::new(),
            project_path: None,
            load_error: None,
            description: String::new(),
            author: None,
            commands: vec![],
            skills: vec![],
            agents: vec![],
            mcp_servers: vec![],
        }
    }

    #[test]
    fn test_plugin_panel_new() {
        let panel = PluginPanel::new(vec![]);
        assert_eq!(panel.cursor(), 0);
        assert_eq!(panel.view, PluginPanelView::Installed);
        assert!(panel.confirm_delete.is_none());
    }

    #[tokio::test]
    async fn test_plugin_panel_move_cursor() {
        let panel = PluginPanel::new(vec![
            make_entry("a@test", "a", true),
            make_entry("b@test", "b", true),
            make_entry("c@test", "c", true),
        ]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));

        for _ in 0..5 {
            app.plugin_panel_move_up();
        }
        assert_eq!(app.global_panels.get::<PluginPanel>().unwrap().cursor(), 0);

        for _ in 0..5 {
            app.plugin_panel_move_down();
        }
        assert_eq!(app.global_panels.get::<PluginPanel>().unwrap().cursor(), 2);
    }

    #[tokio::test]
    async fn test_plugin_panel_tab_cycles_views() {
        let panel = PluginPanel::new(vec![]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));

        app.plugin_panel_tab();
        assert_eq!(
            app.global_panels.get::<PluginPanel>().unwrap().view,
            PluginPanelView::Discover
        );
        app.plugin_panel_tab();
        assert_eq!(
            app.global_panels.get::<PluginPanel>().unwrap().view,
            PluginPanelView::Marketplaces
        );
        app.plugin_panel_tab();
        assert_eq!(
            app.global_panels.get::<PluginPanel>().unwrap().view,
            PluginPanelView::Errors
        );
        app.plugin_panel_tab();
        assert_eq!(
            app.global_panels.get::<PluginPanel>().unwrap().view,
            PluginPanelView::Installed
        );
    }

    #[tokio::test]
    async fn test_plugin_panel_shift_tab_cycles_back() {
        let panel = PluginPanel::new(vec![]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));

        app.plugin_panel_shift_tab();
        assert_eq!(
            app.global_panels.get::<PluginPanel>().unwrap().view,
            PluginPanelView::Errors
        );
        app.plugin_panel_shift_tab();
        assert_eq!(
            app.global_panels.get::<PluginPanel>().unwrap().view,
            PluginPanelView::Marketplaces
        );
    }

    #[tokio::test]
    async fn test_plugin_panel_close() {
        let panel = PluginPanel::new(vec![]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));
        app.plugin_panel_close();
        assert!(!app.global_panels.is_active(crate::app::PanelKind::Plugin));
    }

    #[tokio::test]
    async fn test_plugin_panel_request_cancel_delete() {
        let panel = PluginPanel::new(vec![make_entry("my-plugin@test", "my-plugin", true)]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));

        app.plugin_panel_request_delete();
        assert_eq!(
            app.global_panels
                .get::<PluginPanel>()
                .unwrap()
                .confirm_delete,
            Some("my-plugin@test".into())
        );

        app.plugin_panel_cancel_delete();
        assert!(app
            .global_panels
            .get::<PluginPanel>()
            .unwrap()
            .confirm_delete
            .is_none());
    }

    #[tokio::test]
    async fn test_plugin_panel_toggle_enabled() {
        let panel = PluginPanel::new(vec![make_entry("p@test", "p", true)]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));

        app.plugin_panel_toggle_enabled();
        assert!(!app.global_panels.get::<PluginPanel>().unwrap().entries[0].enabled);

        app.plugin_panel_toggle_enabled();
        assert!(app.global_panels.get::<PluginPanel>().unwrap().entries[0].enabled);
    }

    #[tokio::test]
    async fn test_plugin_panel_errors_view() {
        let mut entry = make_entry("bad@t", "bad-plugin", true);
        entry.load_error = Some("missing manifest".into());
        let panel = PluginPanel::new(vec![make_entry("good@t", "good-plugin", true), entry]);
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Plugin(Box::new(
                panel,
            )));

        // Default view (Installed): 2 items
        assert_eq!(
            app.global_panels
                .get::<PluginPanel>()
                .unwrap()
                .current_list_len(),
            2
        );

        // Switch to Errors view: 1 item
        app.plugin_panel_tab(); // -> Discover
        app.plugin_panel_tab(); // -> Marketplaces
        app.plugin_panel_tab(); // -> Errors
        assert_eq!(
            app.global_panels
                .get::<PluginPanel>()
                .unwrap()
                .current_list_len(),
            1
        );
    }
}
