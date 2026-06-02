use std::any::Any;

use parking_lot::Mutex;
use peri_middlewares::cron::{CronScheduler, CronTask};
use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    Frame,
};
use tui_textarea::Input;

use super::{
    panel_component::PanelComponent,
    panel_list::PanelList,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

// ─── TasksTab ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TasksTab {
    AgentThreads,
    CronTasks,
}

// ─── AgentThreadEntry ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentThreadEntry {
    pub thread_id: String,
    pub title: String,
    pub status: String,
    pub is_active: bool,
    pub message_count: usize,
}

// ─── TasksPanel ────────────────────────────────────────────────────────────

pub struct TasksPanel {
    pub tab: TasksTab,
    pub agent_list: PanelList<AgentThreadEntry>,
    pub detail_thread_id: Option<String>,
    /// Detail view messages loaded lazily on Enter
    pub detail_messages: Vec<String>,
    pub cron_list: PanelList<CronTask>,
    pub confirm_delete: bool,
}

impl TasksPanel {
    pub fn new(agents: Vec<AgentThreadEntry>, cron_tasks: Vec<CronTask>) -> Self {
        let mut agent_list = PanelList::new();
        agent_list.set_items(agents);
        let mut cron_list = PanelList::new();
        cron_list.set_items(cron_tasks);
        Self {
            tab: TasksTab::AgentThreads,
            agent_list,
            detail_thread_id: None,
            detail_messages: Vec::new(),
            cron_list,
            confirm_delete: false,
        }
    }

    pub fn refresh_cron(&mut self, scheduler: &Mutex<CronScheduler>) {
        let new_tasks: Vec<CronTask> = scheduler.lock().list_tasks().into_iter().cloned().collect();
        self.cron_list.set_items(new_tasks);
    }
}

// ─── PanelComponent ────────────────────────────────────────────────────────

impl PanelComponent for TasksPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Tasks
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;

        // confirm_delete mode (CronTasks tab only)
        if self.confirm_delete {
            match input {
                Input {
                    key: Key::Enter, ..
                } => {
                    self.do_confirm_delete_cron(ctx);
                    if self.cron_list.is_empty() {
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
        } else if self.detail_thread_id.is_some() {
            // Detail view: Esc to close
            match input {
                Input { key: Key::Esc, .. } => {
                    self.detail_thread_id = None;
                    self.detail_messages.clear();
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            }
        } else {
            match input {
                // Tab switching
                Input { key: Key::Left, .. }
                | Input {
                    key: Key::Char('h'),
                    ..
                } => {
                    self.switch_tab(-1);
                    EventResult::Consumed
                }
                Input {
                    key: Key::Right, ..
                }
                | Input {
                    key: Key::Char('l'),
                    ..
                } => {
                    self.switch_tab(1);
                    EventResult::Consumed
                }
                Input { key: Key::Tab, .. } => {
                    self.switch_tab(1);
                    EventResult::Consumed
                }
                // List navigation
                Input { key: Key::Up, .. } => {
                    self.active_list_move_cursor(-1);
                    EventResult::Consumed
                }
                Input { key: Key::Down, .. } => {
                    self.active_list_move_cursor(1);
                    EventResult::Consumed
                }
                // Enter
                Input {
                    key: Key::Enter, ..
                } => {
                    match self.tab {
                        TasksTab::AgentThreads => {
                            // Open detail view — signal to load messages
                            if let Some(entry) = self.agent_list.selected() {
                                self.detail_thread_id = Some(entry.thread_id.clone());
                                self.detail_messages.clear();
                            }
                        }
                        TasksTab::CronTasks => {
                            self.do_toggle_cron(ctx);
                        }
                    }
                    EventResult::Consumed
                }
                Input {
                    key: Key::Char(' '),
                    ..
                } => {
                    if self.tab == TasksTab::CronTasks {
                        self.do_toggle_cron(ctx);
                    }
                    EventResult::Consumed
                }
                // Ctrl+D: delete cron task
                Input {
                    key: Key::Char('d'),
                    ctrl: true,
                    ..
                } => {
                    if self.tab == TasksTab::CronTasks
                        && self.cron_list.cursor() < self.cron_list.len()
                    {
                        self.confirm_delete = true;
                    }
                    EventResult::Consumed
                }
                Input { key: Key::Esc, .. } => EventResult::ClosePanel,
                _ => EventResult::Consumed,
            }
        }
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        match self.tab {
            TasksTab::AgentThreads => self.agent_list.handle_scroll(lines, 10),
            TasksTab::CronTasks => self.cron_list.handle_scroll(lines, 10),
        }
        EventResult::Consumed
    }

    fn set_scroll_offset(&mut self, offset: u16) {
        match self.tab {
            TasksTab::AgentThreads => self.agent_list.set_scroll_offset(offset),
            TasksTab::CronTasks => self.cron_list.set_scroll_offset(offset),
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            match self.tab {
                TasksTab::AgentThreads => {
                    self.agent_list
                        .handle_mouse_click(mouse.row, mouse.column, area, 3)
                }
                TasksTab::CronTasks => {
                    self.cron_list
                        .handle_mouse_click(mouse.row, mouse.column, area, 3)
                }
            };
            EventResult::Consumed
        } else {
            EventResult::NotConsumed
        }
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        14
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::tasks::render_tasks_panel(f, self, app, area);
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
                ("Enter".to_string(), _lc.tr("hint-cron-confirm-delete")),
                ("Esc".to_string(), _lc.tr("key-cancel")),
            ];
        }
        if self.detail_thread_id.is_some() {
            return vec![("Esc".to_string(), _lc.tr("key-close"))];
        }
        let mut hints = vec![
            ("\u{2190}\u{2192}".to_string(), _lc.tr("key-switch-tab")),
            ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
        ];
        if self.tab == TasksTab::AgentThreads {
            hints.push(("Enter".to_string(), _lc.tr("key-detail")));
        } else {
            hints.push(("Enter/Space".to_string(), _lc.tr("key-switch")));
            hints.push(("Ctrl+D".to_string(), _lc.tr("key-delete")));
        }
        hints.push(("Esc".to_string(), _lc.tr("key-close")));
        hints
    }
}

// ─── Private helpers ───────────────────────────────────────────────────────

impl TasksPanel {
    fn switch_tab(&mut self, delta: i8) {
        let tabs = [TasksTab::AgentThreads, TasksTab::CronTasks];
        let idx = tabs.iter().position(|&t| t == self.tab).unwrap_or(0);
        let new_idx = if delta > 0 {
            (idx + 1) % tabs.len()
        } else {
            (idx + tabs.len() - 1) % tabs.len()
        };
        self.tab = tabs[new_idx];
        self.confirm_delete = false;
    }

    fn active_list_move_cursor(&mut self, delta: isize) {
        match self.tab {
            TasksTab::AgentThreads => self.agent_list.move_cursor(delta),
            TasksTab::CronTasks => self.cron_list.move_cursor(delta),
        }
    }

    fn do_toggle_cron(&mut self, ctx: &mut PanelContext<'_>) {
        let idx = self.cron_list.cursor();
        let tasks = self.cron_list.items();
        if idx < tasks.len() {
            let id = tasks[idx].id.clone();
            ctx.services.cron.scheduler.lock().toggle(&id);
            self.refresh_cron(&ctx.services.cron.scheduler);
        }
    }

    fn do_confirm_delete_cron(&mut self, ctx: &mut PanelContext<'_>) {
        self.confirm_delete = false;
        let idx = self.cron_list.cursor();
        let tasks = self.cron_list.items();
        if idx < tasks.len() {
            let prompt_preview: String = tasks[idx].prompt.chars().take(30).collect();
            let id = tasks[idx].id.clone();
            ctx.services.cron.scheduler.lock().remove(&id);
            self.refresh_cron(&ctx.services.cron.scheduler);
            ctx.session_mgr
                .current_mut()
                .messages
                .push_system_note(ctx.services.lc.tr_args(
                    "app-cron-deleted",
                    &[("preview".into(), prompt_preview.into())],
                ));
        }
    }
}
