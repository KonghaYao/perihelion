use std::any::Any;

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    Frame,
};
use tui_textarea::Input;

use crate::command::agents::AgentItem;

use super::{
    panel_component::PanelComponent,
    panel_list::PanelList,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

/// AgentPanel 内部用占位单元管理 cursor/scroll，实际 agent 数据在 agents 字段
#[derive(Clone)]
pub(crate) struct AgentEntry;

// ─── AgentPanel ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AgentPanel {
    /// 可用 agent 列表
    pub agents: Vec<AgentItem>,
    /// 当前选中的 agent_id
    pub selected_id: Option<String>,
    /// 光标/滚动状态管理（items 长度 = 1 + agents.len()，包含"无 Agent"占位）
    pub(crate) list: PanelList<AgentEntry>,
}

impl AgentPanel {
    pub fn new(agents: Vec<AgentItem>, current_id: Option<String>) -> Self {
        let total = 1 + agents.len();
        // 如果已有选中，定位光标到对应的 agents 索引+1（+1 因为第0项是"无 Agent"）
        let cursor = current_id
            .as_ref()
            .and_then(|id| agents.iter().position(|a| &a.id == id))
            .map(|i| i + 1)
            .unwrap_or(0);

        let mut list = PanelList::new();
        list.set_items(vec![AgentEntry; total]);
        // 恢复 cursor 位置
        for _ in 0..cursor {
            list.move_cursor(1);
        }

        Self {
            agents,
            selected_id: current_id,
            list,
        }
    }

    /// 总项数 = "无 Agent" 选项 + agents 列表
    pub fn total(&self) -> usize {
        self.list.len()
    }

    /// 光标位置（0 = "无 Agent"，1+ = agents 列表索引）
    pub fn cursor(&self) -> usize {
        self.list.cursor()
    }

    /// 内容滚动偏移
    pub fn scroll_offset(&self) -> u16 {
        self.list.scroll_offset()
    }

    /// 选择当前光标处的 agent（Enter 确认选择）
    /// 返回 (is_none: bool, agent_id: Option<String>)
    pub fn get_selection(&self) -> (bool, Option<String>) {
        if self.cursor() == 0 {
            (true, None)
        } else if let Some(agent) = self.agents.get(self.cursor() - 1) {
            (false, Some(agent.id.clone()))
        } else {
            (true, None)
        }
    }

    /// 获取当前光标处的 agent（不包含"无 Agent"选项）
    pub fn current_agent(&self) -> Option<&AgentItem> {
        if self.cursor() == 0 {
            None
        } else {
            self.agents.get(self.cursor() - 1)
        }
    }
}

// ─── PanelComponent 实现 ──────────────────────────────────────────────────────

impl PanelComponent for AgentPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Agent
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => EventResult::NotConsumed,
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. } => {
                self.list.move_cursor(-1);
                self.list.ensure_visible(10);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.list.move_cursor(1);
                self.list.ensure_visible(10);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let (is_none, agent_id, agent_name) = {
                    let (is_none, agent_id) = self.get_selection();
                    let agent_name = if is_none {
                        None
                    } else {
                        agent_id
                            .as_ref()
                            .and_then(|_id| self.current_agent().map(|a| a.name.clone()))
                    };
                    (is_none, agent_id, agent_name)
                };

                if is_none {
                    ctx.session_mgr.current_mut().agent.agent_id = None;
                    ctx.session_mgr
                        .current_mut()
                        .messages
                        .push_system_note(ctx.services.lc.tr("app-agent-reset"));
                } else if let Some(id) = agent_id {
                    ctx.session_mgr.current_mut().agent.agent_id = Some(id.clone());
                    let name = agent_name.unwrap_or_else(|| id.clone());
                    ctx.session_mgr.current_mut().messages.push_system_note(
                        ctx.services.lc.tr_args(
                            "app-agent-switched",
                            &[("name".into(), name.into()), ("id".into(), id.into())],
                        ),
                    );
                }
                EventResult::ClosePanel
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.list.handle_scroll(lines, 10);
        EventResult::Consumed
    }

    fn set_scroll_offset(&mut self, offset: u16) {
        self.list.set_scroll_offset(offset);
    }

    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && self
                .list
                .handle_mouse_click(mouse.row, mouse.column, area, 1)
        {
            return self.handle_key(
                Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                ctx,
            );
        }
        EventResult::NotConsumed
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        (self.agents.len() as u16 * 2 + 6).max(6)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::agent::render_agent_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            ("↑↓".to_string(), _lc.tr("key-select")),
            ("Enter".to_string(), _lc.tr("key-confirm")),
            ("Esc".to_string(), _lc.tr("key-cancel")),
        ]
    }
}
