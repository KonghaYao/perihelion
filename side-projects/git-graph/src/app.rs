use crate::git::commit::{CommitDetail, DiffStats};
use crate::git::repo::GitRepo;
use crate::git::stash::StashInfo;
use crate::graph::color::BranchColors;
use crate::graph::layout::GraphLayout;
use crate::graph::topology::Topology;
use crate::theme::GigTheme;
use crate::ui::toolbar::{GlobalToolbarState, ToolbarState};
use anyhow::Result;
use git2::Oid;
use peri_widgets::FileNode;
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Toast 通知消息
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub style: ToastStyle,
    pub expires_at: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastStyle {
    Success,
    Error,
    Info,
}

/// 弹窗输入状态（创建 tag / branch 共用）
#[derive(Debug, Clone)]
pub struct InputDialog {
    pub title: String,
    pub value: String,
    pub cursor_pos: usize,
    pub action: InputAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    CreateTag,
    CreateBranch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    FileTree,
    Status,
    Graph,
    Detail,
}

/// Status 焦点下的子面板（Staged / Changes）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusSubPanel {
    Staged,
    Changes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Overlay {
    None,
    BranchList,
    TagList,
    StashList,
    ConfirmDialog,
    FilterBar,
    SearchBar,
    InputDialog,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ConfirmAction {
    ResetHard(Oid),
    DeleteBranch(String),
    DeleteTag(String),
    StashDrop(usize),
    ForcePush,
    PushSetUpstream(String), // branch name
    PullRebase,
    CheckoutBranch(String), // branch name
}

#[allow(dead_code)]
pub struct App {
    pub running: bool,
    pub mouse_enabled: bool,
    pub dirty: bool,
    pub theme: GigTheme,
    pub repo: GitRepo,
    pub topology: Topology,
    pub layout: GraphLayout,
    pub colors: BranchColors,
    pub head_oid: Oid,
    pub focus: Focus,
    pub overlay: Overlay,
    /// overlay 列表选中索引（BranchList/TagList/StashList 共用）
    pub overlay_selected: usize,
    pub selected_idx: usize,
    pub scroll_offset: usize,
    pub viewport_height: usize,
    pub graph_width: u16,
    pub detail_width: u16,
    pub selected_oid: Option<Oid>,
    pub selected_detail: Option<CommitDetail>,
    pub selected_diff_stats: Option<DiffStats>,
    pub stash_map: HashMap<Oid, Vec<StashInfo>>,
    pub confirm_message: Option<String>,
    pub confirm_action: Option<ConfirmAction>,
    pub filter_branch: Option<String>,
    pub search_query: Option<String>,
    /// 弹窗输入框内容（tag/branch 名称）
    pub input_dialog: Option<InputDialog>,
    /// Toast 通知（统一替代 remote_status）
    pub toast: Option<Toast>,
    /// 远程操作完成的结果通道（主循环轮询更新 toast）
    pub remote_result_rx: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    pub toolbar_state: ToolbarState,
    pub global_toolbar_state: GlobalToolbarState,
    /// 当前分支相对 upstream 的 ahead/behind（reload 时更新）
    pub ahead_behind: Option<(usize, usize)>,
    /// graph 面板内容区域的 y 坐标（用于鼠标点击偏移计算）
    pub graph_inner_y: u16,
    /// graph 面板区域（用于鼠标点击检测）
    pub graph_area: Rect,
    // === Sidebar 状态 ===
    /// 文件树状态
    pub file_tree_state: peri_widgets::FileTreeState,
    /// git status 查询结果
    pub git_status: crate::git::status::StatusResult,
    /// sidebar 面板区域（鼠标事件检测用）
    pub sidebar_area: Rect,
    /// sidebar 分割线 y 坐标
    pub sidebar_split_y: u16,
    /// status 面板各 section 折叠状态
    pub status_staged_expanded: bool,
    pub status_unstaged_expanded: bool,
    pub status_untracked_expanded: bool,
    /// status 面板中折叠的目录路径（空=全部展开）
    pub status_dir_collapsed: HashSet<String>,
    /// Staged 面板滚动偏移
    pub staged_scroll: u16,
    /// Staged 面板内容总行数（渲染时更新）
    pub staged_total_lines: u16,
    /// Staged 面板可见行数（渲染时更新）
    pub staged_viewport: u16,
    /// Changes 面板滚动偏移
    pub changes_scroll: u16,
    /// Changes 面板内容总行数（渲染时更新）
    pub changes_total_lines: u16,
    /// Changes 面板可见行数（渲染时更新）
    pub changes_viewport: u16,
    /// detail 面板滚动偏移
    pub detail_scroll: u16,
    /// detail 面板内容总行数（渲染时更新）
    pub detail_total_lines: u16,
    /// detail 面板可见行数（渲染时更新）
    pub detail_viewport: u16,
    /// 全屏区域（渲染时更新，供弹窗点击检测用）
    pub frame_area: ratatui::layout::Rect,
    /// detail 面板内容区 y 起始坐标
    pub detail_content_y: u16,
    /// detail 面板区域
    pub detail_area: Rect,
    /// status 面板布局（每次渲染后更新，用于点击检测）
    pub sidebar_layout: crate::ui::sidebar::SidebarLayout,
    /// sidebar 上次刷新时间
    last_sidebar_refresh: std::time::Instant,
    // === 文件预览状态 ===
    /// 预览文件：(相对路径, 是否 staged)
    pub preview_file: Option<(String, bool)>,
    /// Status 焦点下的子面板
    pub status_sub_panel: StatusSubPanel,
    /// Status 子面板中的文件光标索引
    pub status_file_index: usize,
    /// Staged 面板可见文件路径列表（渲染时更新）
    pub staged_visible_files: Vec<String>,
    /// Changes 面板可见文件路径列表（渲染时更新）
    pub changes_visible_files: Vec<String>,
    /// 文件预览滚动偏移（虚拟滚动）
    pub preview_scroll: u16,
    /// 文件预览内容（一次性高亮后的 styled spans，每行一个 Vec）
    pub preview_highlighted: Vec<Vec<(ratatui::style::Style, String)>>,
    /// 文件预览是否被截断（超大文件）
    pub preview_truncated: bool,
}

impl App {
    pub fn new(mut repo: GitRepo) -> Result<Self> {
        let head_oid = repo.head_oid()?;
        let branch_map = repo.branch_map()?;
        let tag_map = repo.tag_map()?;
        let remote_branch_map = repo.remote_branch_map()?;
        let stash_map = repo.stash_by_commit()?;
        let stash_oids: Vec<git2::Oid> = stash_map.values().flatten().map(|s| s.oid).collect();
        let nodes = repo.scan_topology_with_extra(&stash_oids)?;
        let topology = Topology::new(
            nodes,
            branch_map.clone(),
            tag_map.clone(),
            stash_map.clone(),
        );
        let mut colors = BranchColors::new();
        let layout = crate::graph::layout::build_layout(
            topology.nodes(),
            &branch_map,
            &stash_map,
            &mut colors,
            topology.tag_map(),
            &remote_branch_map,
        );
        let selected_idx = layout
            .rows
            .iter()
            .position(|r| r.oid == Some(head_oid))
            .unwrap_or(0);

        // 准备 sidebar 数据（在 repo move 之前）
        let workdir = repo.repo().workdir().map(|p| p.to_path_buf());
        let git_status = crate::git::status::read_status(repo.repo()).unwrap_or_default();
        let ahead_behind = repo.ahead_behind();

        let mut file_tree_state = peri_widgets::FileTreeState::new();
        if let Some(wd) = &workdir {
            let root_nodes = scan_dir_top_level(wd);
            file_tree_state.set_root(root_nodes);
            file_tree_state.sort();
        }

        let mut app = Self {
            running: true,
            mouse_enabled: true,
            dirty: true,
            theme: GigTheme::new(),
            repo,
            topology,
            layout,
            colors,
            head_oid,
            focus: Focus::Graph,
            overlay: Overlay::None,
            overlay_selected: 0,
            selected_idx,
            scroll_offset: 0,
            viewport_height: 40,
            graph_width: 60,
            detail_width: 40,
            selected_oid: Some(head_oid),
            selected_detail: None,
            selected_diff_stats: None,
            stash_map,
            confirm_message: None,
            confirm_action: None,
            filter_branch: None,
            search_query: None,
            input_dialog: None,
            toast: None,
            remote_result_rx: std::sync::Arc::new(std::sync::Mutex::new(None)),
            toolbar_state: ToolbarState::new(),
            global_toolbar_state: GlobalToolbarState::new(),
            ahead_behind,
            graph_inner_y: 0,
            graph_area: Rect::default(),
            file_tree_state,
            git_status,
            sidebar_area: Rect::default(),
            sidebar_split_y: 0,
            status_staged_expanded: true,
            status_unstaged_expanded: true,
            status_untracked_expanded: true,
            status_dir_collapsed: HashSet::new(),
            staged_scroll: 0,
            staged_total_lines: 0,
            staged_viewport: 0,
            changes_scroll: 0,
            changes_total_lines: 0,
            changes_viewport: 0,
            detail_scroll: 0,
            detail_total_lines: 0,
            detail_viewport: 0,
            frame_area: ratatui::layout::Rect::default(),
            detail_content_y: 0,
            detail_area: Rect::default(),
            sidebar_layout: crate::ui::sidebar::SidebarLayout::default(),
            last_sidebar_refresh: std::time::Instant::now(),
            preview_file: None,
            status_sub_panel: StatusSubPanel::Changes,
            status_file_index: 0,
            staged_visible_files: Vec::new(),
            changes_visible_files: Vec::new(),
            preview_scroll: 0,
            preview_highlighted: Vec::new(),
            preview_truncated: false,
        };
        app.select(selected_idx);
        Ok(app)
    }

    pub fn select(&mut self, idx: usize) {
        if idx >= self.layout.rows.len() {
            return;
        }
        self.selected_idx = idx;
        self.detail_scroll = 0;
        self.update_selected_detail();
    }

    /// 刷新选中 commit 的详情，但保留 scroll 位置
    fn select_keep_scroll(&mut self, idx: usize) {
        if idx >= self.layout.rows.len() {
            return;
        }
        self.selected_idx = idx;
        self.update_selected_detail();
    }

    fn update_selected_detail(&mut self) {
        let idx = self.selected_idx;
        if idx >= self.layout.rows.len() {
            return;
        }
        let row = &self.layout.rows[idx];
        if let Some(oid) = row.oid {
            self.selected_oid = Some(oid);
            if let Ok(detail) = self.repo.commit_detail(oid) {
                self.selected_detail = Some(detail);
            }
            if let Ok(stats) = self.repo.load_diff_stats(oid) {
                self.selected_diff_stats = Some(stats);
            }
        }
    }

    pub fn show_toast(&mut self, message: String, style: ToastStyle) {
        self.toast = Some(Toast {
            message,
            style,
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(2),
        });
        self.dirty = true;
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    /// 加载文件预览内容，一次性完成语法高亮。调用前确保 preview_file 已设置。
    pub fn load_preview(&mut self) {
        self.preview_highlighted.clear();
        self.preview_truncated = false;
        self.preview_scroll = 0;
        if let Some((ref path, is_staged)) = self.preview_file {
            let result = if is_staged {
                self.repo.read_staged_file(path)
            } else {
                self.repo.read_working_file(path)
            };
            if let Ok(data) = result {
                let text = String::from_utf8_lossy(&data);
                const MAX_LINES: usize = 500_000;
                let lines: Vec<&str> = text.lines().take(MAX_LINES).collect();
                self.preview_truncated = lines.len() >= MAX_LINES;

                // 一次性高亮全部行
                let ext = crate::ui::syntax::extension_from_path(path);
                let syntax = crate::ui::syntax::find_syntax(ext);
                if let Some(syn) = syntax {
                    let theme = crate::ui::syntax::get_theme();
                    let ss = crate::ui::syntax::get_syntax_set();
                    let mut h = syntect::easy::HighlightLines::new(syn, theme);
                    for line in lines {
                        match h.highlight_line(line, ss) {
                            Ok(segments) => {
                                let spans: Vec<_> = segments
                                    .into_iter()
                                    .map(|(s, t)| {
                                        (crate::ui::syntax::to_ratatui_style(s), t.to_string())
                                    })
                                    .collect();
                                self.preview_highlighted.push(spans);
                            }
                            Err(_) => {
                                self.preview_highlighted.push(vec![(
                                    ratatui::style::Style::default(),
                                    line.to_string(),
                                )]);
                            }
                        }
                    }
                } else {
                    // 无语法高亮，存纯文本
                    for line in lines {
                        self.preview_highlighted
                            .push(vec![(ratatui::style::Style::default(), line.to_string())]);
                    }
                }
            }
        }
    }

    pub fn reload(&mut self) -> Result<()> {
        let branch_map = self.repo.branch_map()?;
        let tag_map = self.repo.tag_map()?;
        let remote_branch_map = self.repo.remote_branch_map()?;
        let stash_map = self.repo.stash_by_commit()?;
        let stash_oids: Vec<git2::Oid> = stash_map.values().flatten().map(|s| s.oid).collect();
        let nodes = self.repo.scan_topology_with_extra(&stash_oids)?;
        self.topology = Topology::new(
            nodes,
            branch_map.clone(),
            tag_map.clone(),
            stash_map.clone(),
        );
        self.stash_map = stash_map;
        self.layout = crate::graph::layout::build_layout(
            self.topology.nodes(),
            &branch_map,
            &self.stash_map,
            &mut self.colors,
            self.topology.tag_map(),
            &remote_branch_map,
        );
        self.head_oid = self.repo.head_oid()?;
        self.ahead_behind = self.repo.ahead_behind();
        if let Some(oid) = self.selected_oid {
            if let Some(idx) = self.layout.rows.iter().position(|r| r.oid == Some(oid)) {
                self.selected_idx = idx;
            }
        }
        self.select_keep_scroll(self.selected_idx);
        self.git_status = crate::git::status::read_status(self.repo.repo()).unwrap_or_default();
        self.dirty = true;
        Ok(())
    }

    /// 刷新 sidebar 数据（git status + 文件树 + graph），超过 interval 才刷新
    pub fn refresh_sidebar(&mut self) {
        // 先检查远程操作结果
        let remote_result = if let Ok(mut rx) = self.remote_result_rx.lock() {
            rx.take()
        } else {
            None
        };
        if let Some(result) = remote_result {
            let style = if result.contains("失败") {
                ToastStyle::Error
            } else {
                ToastStyle::Success
            };
            self.show_toast(result, style);
        }

        if self.last_sidebar_refresh.elapsed() < std::time::Duration::from_secs(2) {
            return;
        }
        self.last_sidebar_refresh = std::time::Instant::now();

        // 刷新 git graph + status
        let _ = self.reload();

        // 刷新文件树：记录已展开路径，重建后恢复
        if let Some(wd) = self.repo.repo().workdir() {
            let expanded_paths = self.collect_expanded_paths();
            let new_root = scan_dir_top_level(wd);
            self.file_tree_state.set_root(new_root);
            self.file_tree_state.sort();
            self.restore_expanded_paths(&expanded_paths);
        }
    }

    /// 收集当前文件树中所有已展开目录的路径
    fn collect_expanded_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        // 从 file_tree_state 的 root 无法直接访问，用 flat 列表推导
        for flat in self.file_tree_state.flat() {
            if flat.is_dir && flat.expanded {
                if let Some(ref p) = flat.path {
                    paths.push(p.clone());
                }
            }
        }
        paths
    }

    /// 恢复已展开路径的展开状态
    fn restore_expanded_paths(&mut self, paths: &[String]) {
        let path_set: std::collections::HashSet<&str> = paths.iter().map(|s| s.as_str()).collect();
        // 遍历 flat 列表，对匹配路径的目录执行 toggle
        let flat_len = self.file_tree_state.len();
        for idx in 0..flat_len {
            if let Some(node) = self.file_tree_state.flat().get(idx) {
                if node.is_dir && !node.expanded {
                    if let Some(ref p) = node.path {
                        if path_set.contains(p.as_str()) {
                            // 需要先加载子节点再展开
                            if let Some(result) = self.file_tree_state.toggle(idx) {
                                if result.needs_load {
                                    let children = scan_dir_children(&result.path);
                                    self.file_tree_state.set_children(&result.path, children);
                                    // 找到新的 idx（set_children 可能改变了 flat 列表）
                                    if let Some(new_idx) =
                                        self.file_tree_state.flat().iter().position(|f| {
                                            f.path.as_deref() == Some(result.path.as_str())
                                        })
                                    {
                                        self.file_tree_state.toggle(new_idx);
                                    }
                                }
                            }
                            // 注意：toggle 后 flat 列表会变，但已展开的目录不需要再处理
                        }
                    }
                }
            }
        }
    }
}

/// 扫描目录顶层（不递归，目录标记 loaded=false 等待懒加载）
fn scan_dir_top_level(dir: &Path) -> Vec<FileNode> {
    let mut entries: Vec<FileNode> = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let path = entry.path().to_string_lossy().to_string();
        entries.push(FileNode {
            name,
            is_dir,
            children: Vec::new(),
            expanded: false,
            loaded: !is_dir,
            path: Some(path),
        });
    }
    entries
}

/// 扫描目录子节点（懒加载用）
pub fn scan_dir_children(dir_path: &str) -> Vec<FileNode> {
    let path = Path::new(dir_path);
    let mut nodes = scan_dir_top_level(path);
    sort_nodes(&mut nodes);
    nodes
}

/// 排序：目录优先 + 字母序
fn sort_nodes(nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
}
