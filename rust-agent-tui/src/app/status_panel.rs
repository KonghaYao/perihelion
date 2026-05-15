use std::any::Any;

use perihelion_widgets::tab_bar::TabState;
use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use super::panel_component::PanelComponent;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

/// Status 面板 Tab 索引
pub const STATUS_TAB_COST: usize = 0;
pub const STATUS_TAB_CONTEXT: usize = 1;

/// /cost & /context 共用的只读状态面板
#[derive(Clone)]
pub struct StatusPanel {
    pub tab: TabState,
    pub scroll_offset: u16,
}

impl StatusPanel {
    /// 创建面板并激活指定 Tab
    pub fn new(active_tab: usize) -> Self {
        let mut tab = TabState::new(vec!["Cost".to_string(), "Context".to_string()]);
        tab.set_active(active_tab);
        Self {
            tab,
            scroll_offset: 0,
        }
    }
}

// ─── PanelComponent 实现 ──────────────────────────────────────────────────────

impl PanelComponent for StatusPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Status
    }

    fn handle_key(&mut self, input: Input, _ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Left, .. } => {
                self.tab.prev();
                EventResult::Consumed
            }
            Input {
                key: Key::Right, ..
            } => {
                self.tab.next();
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        20
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::status::render_status_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            ("\u{2190}\u{2192}".to_string(), _lc.tr("key-tab")),
            ("Esc".to_string(), "\u{5173}\u{95ed}".to_string()),
        ]
    }
}
