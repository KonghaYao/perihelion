use std::any::Any;
use std::path::PathBuf;

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use super::panel_component::PanelComponent;
use super::panel_list::PanelList;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

/// Memory 文件条目
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub label: String,
    pub path: PathBuf,
    pub exists: bool,
}

/// /memory 面板状态
#[derive(Debug, Clone)]
pub struct MemoryPanel {
    pub entries: Vec<MemoryEntry>,
    pub(crate) list: PanelList<MemoryEntry>,
}

impl MemoryPanel {
    /// 根据 cwd 和 home 目录创建面板，自动检测文件是否存在
    pub fn new(cwd: &str, home_dir: Option<PathBuf>) -> Self {
        let project_path = PathBuf::from(cwd).join("CLAUDE.md");
        let global_path = home_dir
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".claude")
            .join("CLAUDE.md");

        let entries = vec![
            MemoryEntry {
                label: "项目说明".to_string(),
                path: project_path,
                exists: false, // 延迟到 refresh_exists 时检查
            },
            MemoryEntry {
                label: "用户全局".to_string(),
                path: global_path,
                exists: false,
            },
        ];

        let mut list = PanelList::new();
        list.set_items(entries.clone());

        Self { entries, list }
    }

    /// 刷新所有条目的 exists 状态
    pub fn refresh_exists(&mut self) {
        for entry in &mut self.entries {
            entry.exists = entry.path.exists();
        }
    }

    /// 光标位置委托
    pub fn cursor(&self) -> usize {
        self.list.cursor()
    }

    /// 滚动偏移委托
    pub fn scroll_offset(&self) -> u16 {
        self.list.scroll_offset()
    }
}

// ─── PanelComponent 实现 ──────────────────────────────────────────────────────

impl PanelComponent for MemoryPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Memory
    }

    fn handle_key(&mut self, input: Input, _ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Up, .. } => {
                self.list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                // 特殊标记：由调用方处理编辑器打开
                EventResult::OpenPanel(PanelKind::Memory)
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.list.handle_scroll(lines, 10);
        EventResult::Consumed
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
        (self.entries.len() as u16 * 2 + 4).max(6)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::memory::render_memory_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            (
                "\u{2191}\u{2193}".to_string(),
                "\u{9009}\u{62e9}".to_string(),
            ),
            ("Enter".to_string(), "\u{7f16}\u{8f91}".to_string()),
            ("Esc".to_string(), "\u{5173}\u{95ed}".to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("memory_panel_test.rs");
}
