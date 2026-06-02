use std::{any::Any, sync::Arc};

use ratatui::{layout::Rect, Frame};
use tui_textarea::Input;

use peri_widgets::InputState;

use crate::app::{
    panel_component::PanelComponent,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

use super::{ThreadId, ThreadMeta, ThreadStore};

/// TUI 内 Thread 历史浏览面板
#[derive(Clone)]
pub struct ThreadBrowser {
    /// 全量 thread 列表（按 updated_at 降序）
    pub threads: Vec<ThreadMeta>,
    /// 当前光标位置（指向过滤后列表的索引）
    pub cursor: usize,
    pub store: Arc<dyn ThreadStore>,
    /// 内容滚动偏移
    pub scroll_offset: u16,
    /// 是否处于删除确认状态
    pub confirm_delete: bool,
    /// 搜索输入状态
    pub search_query: InputState,
    /// 搜索框是否聚焦
    pub search_focused: bool,
    /// 当前 cwd 的 git 分支
    pub branch: Option<String>,
    /// 过滤后的索引映射（存储 threads 中的原始索引）
    filtered_indices: Vec<usize>,
}

impl ThreadBrowser {
    pub fn new(
        threads: Vec<ThreadMeta>,
        store: Arc<dyn ThreadStore>,
        branch: Option<String>,
    ) -> Self {
        let filtered_indices: Vec<usize> = (0..threads.len()).collect();
        Self {
            threads,
            cursor: 0,
            store,
            scroll_offset: 0,
            confirm_delete: false,
            search_query: InputState::new(),
            search_focused: true,
            branch,
            filtered_indices,
        }
    }

    /// 过滤后的 thread 总数
    pub fn total(&self) -> usize {
        self.filtered_indices.len()
    }

    /// 全量 thread 总数
    pub fn total_all(&self) -> usize {
        self.threads.len()
    }

    /// 重新计算过滤索引
    pub fn refresh_filter(&mut self) {
        let query = self.search_query.value().to_lowercase();
        self.filtered_indices = if query.is_empty() {
            (0..self.threads.len()).collect()
        } else {
            self.threads
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    t.title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query)
                })
                .map(|(i, _)| i)
                .collect()
        };
        // 光标修正
        if self.cursor >= self.filtered_indices.len() {
            self.cursor = self.filtered_indices.len().saturating_sub(1);
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let total = self.total();
        if total == 0 {
            return;
        }
        self.cursor = ((self.cursor as isize + delta).rem_euclid(total as isize)) as usize;
    }

    /// 获取光标指向的过滤后 thread
    pub fn selected_thread(&self) -> Option<&ThreadMeta> {
        self.filtered_indices
            .get(self.cursor)
            .and_then(|&idx| self.threads.get(idx))
    }

    /// 获取光标指向的 ThreadId
    pub fn selected_id(&self) -> Option<&ThreadId> {
        self.selected_thread().map(|t| &t.id)
    }

    /// 删除光标所在的历史 thread（同步，block_in_place），返回被删除的对话标题
    pub fn delete_selected(&mut self) -> Option<String> {
        let &orig_idx = self.filtered_indices.get(self.cursor)?;
        let meta = self.threads.get(orig_idx)?;
        let id = meta.id.clone();
        let title = meta.title.clone().unwrap_or_else(|| "(无标题)".to_string());
        let store = self.store.clone();
        let ok = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.delete_thread(&id))
                .is_ok()
        });
        if ok {
            self.threads.remove(orig_idx);
            // 重建过滤索引
            self.refresh_filter();
            Some(title)
        } else {
            None
        }
    }

    /// 获取过滤后的 thread 列表引用
    pub fn filtered_threads(&self) -> Vec<&ThreadMeta> {
        self.filtered_indices
            .iter()
            .filter_map(|&idx| self.threads.get(idx))
            .collect()
    }
}

impl PanelComponent for ThreadBrowser {
    fn kind(&self) -> PanelKind {
        PanelKind::ThreadBrowser
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;

        // confirm_delete mode
        if self.confirm_delete {
            match input {
                Input {
                    key: Key::Enter, ..
                } => {
                    self.confirm_delete = false;
                    if let Some(title) = self.delete_selected() {
                        ctx.session_mgr.current_mut().messages.view_messages.push(
                            crate::ui::message_view::MessageViewModel::system(format!(
                                "\u{5df2}\u{5220}\u{9664}\u{5bf9}\u{8bdd}: {}",
                                title
                            )),
                        );
                    }
                    // if empty after delete, close
                    if self.total() == 0 {
                        EventResult::ClosePanel
                    } else {
                        EventResult::Consumed
                    }
                }
                _ => {
                    self.confirm_delete = false;
                    EventResult::Consumed
                }
            }
        } else if self.search_focused {
            // search focused mode
            match input {
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => EventResult::Consumed,
                Input { key: Key::Esc, .. } => {
                    if !self.search_query.value().is_empty() {
                        self.search_query.set_value(String::new());
                        self.refresh_filter();
                        EventResult::Consumed
                    } else {
                        EventResult::ClosePanel
                    }
                }
                Input {
                    key: Key::Char('v'),
                    ctrl: true,
                    ..
                } => {
                    if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                        self.search_query.paste(&text);
                        self.refresh_filter();
                    }
                    EventResult::Consumed
                }
                Input {
                    key: Key::Char(c), ..
                } => {
                    self.search_query.insert(c);
                    self.refresh_filter();
                    EventResult::Consumed
                }
                Input {
                    key: Key::Backspace,
                    ..
                } => {
                    self.search_query.backspace();
                    self.refresh_filter();
                    EventResult::Consumed
                }
                Input {
                    key: Key::Delete, ..
                } => {
                    self.search_query.delete();
                    self.refresh_filter();
                    EventResult::Consumed
                }
                Input { key: Key::Left, .. } => {
                    self.search_query.cursor_left();
                    EventResult::Consumed
                }
                Input {
                    key: Key::Right, ..
                } => {
                    self.search_query.cursor_right();
                    EventResult::Consumed
                }
                Input { key: Key::Home, .. } => {
                    self.search_query.cursor_home();
                    EventResult::Consumed
                }
                Input { key: Key::End, .. } => {
                    self.search_query.cursor_end();
                    EventResult::Consumed
                }
                // Down / Tab -> exit search
                Input { key: Key::Down, .. } | Input { key: Key::Tab, .. } => {
                    self.search_focused = false;
                    EventResult::Consumed
                }
                // Enter: open selected thread
                Input {
                    key: Key::Enter, ..
                } => {
                    if let Some(id) = self.selected_id().cloned() {
                        return EventResult::OpenThread(id);
                    }
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            }
        } else {
            // list mode
            match input {
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => EventResult::Consumed,
                Input { key: Key::Esc, .. } => EventResult::ClosePanel,
                Input { key: Key::Up, .. } => {
                    self.move_cursor(-1);
                    EventResult::Consumed
                }
                Input { key: Key::Down, .. } => {
                    self.move_cursor(1);
                    EventResult::Consumed
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    if let Some(id) = self.selected_id().cloned() {
                        return EventResult::OpenThread(id);
                    }
                    EventResult::Consumed
                }
                Input {
                    key: Key::Char('d'),
                    ctrl: true,
                    ..
                } => {
                    if self.total() > 0 {
                        self.confirm_delete = true;
                    }
                    EventResult::Consumed
                }
                // / or Tab -> enter search
                Input {
                    key: Key::Char('/'),
                    ..
                }
                | Input { key: Key::Tab, .. } => {
                    self.search_focused = true;
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            }
        }
    }

    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        if self.search_focused {
            self.search_query.paste(text);
            self.refresh_filter();
        }
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        let total = self.total();
        if total == 0 {
            return EventResult::Consumed;
        }
        let delta: i16 = if lines > 0 { 1 } else { -1 };
        let new = (self.cursor as isize + delta as isize).clamp(0, (total - 1) as isize) as usize;
        self.cursor = new;
        // 同步 scroll_offset，让光标行始终可见（每个 thread 3 行）
        let cursor_line = self.cursor as u16 * 3;
        if cursor_line < self.scroll_offset {
            self.scroll_offset = cursor_line;
        } else {
            // 需要知道可见高度，但这里没有。用保守策略：直接把 scroll_offset 对齐到光标
            // 渲染时 ScrollableArea 会做最终 clamp
            if cursor_line >= self.scroll_offset {
                self.scroll_offset = cursor_line;
            }
        }
        EventResult::Consumed
    }

    fn set_scroll_offset(&mut self, offset: u16) {
        self.scroll_offset = offset;
    }

    fn handle_mouse(
        &mut self,
        mouse: ratatui::crossterm::event::MouseEvent,
        area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};

        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return EventResult::NotConsumed;
        }

        // area 就是 list_area（panel_area 存的是 list_area），无需额外偏移
        let relative_y = mouse.row.saturating_sub(area.y);
        if relative_y >= area.height {
            return EventResult::NotConsumed;
        }

        // 每个 thread 占 3 行（标题 + meta + 空行），加上 scroll_offset
        let row_with_scroll = relative_y as usize + self.scroll_offset as usize;
        let clicked_idx = row_with_scroll / 3;

        if clicked_idx < self.total() {
            self.cursor = clicked_idx;
            return EventResult::Consumed;
        }

        EventResult::NotConsumed
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        (screen_height * 3 / 5).max(16)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::thread_browser::render_thread_browser(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        if self.confirm_delete {
            return vec![
                ("Enter".to_string(), _lc.tr("hint-history-confirm-delete")),
                ("Esc".to_string(), _lc.tr("key-cancel")),
            ];
        }
        if self.search_focused {
            return vec![
                (
                    "\u{2193}/Tab".to_string(),
                    _lc.tr("hint-plugin-exit-search"),
                ),
                ("Esc".to_string(), _lc.tr("key-close")),
            ];
        }
        vec![
            ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
            ("Enter".to_string(), _lc.tr("key-confirm")),
            ("/".to_string(), _lc.tr("hint-plugin-search")),
            ("Ctrl+D".to_string(), _lc.tr("key-delete")),
            ("Esc".to_string(), _lc.tr("key-close")),
        ]
    }
}
