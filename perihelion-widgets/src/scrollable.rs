use ratatui::{
    layout::Rect,
    style::Style,
    text::Text,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

/// 滚动偏移状态
///
/// 管理垂直滚动 offset，提供 ensure_visible 方法自动调整 offset 使指定行可见。
/// 逻辑从 `rust-agent-tui/src/app/mod.rs:ensure_cursor_visible()` 迁移。
#[derive(Debug, Clone, Default)]
pub struct ScrollState {
    offset: u16,
}

impl ScrollState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_offset(offset: u16) -> Self {
        Self { offset }
    }

    pub fn offset(&self) -> u16 {
        self.offset
    }

    /// 向上滚动 delta 行
    pub fn scroll_up(&mut self, delta: u16) {
        self.offset = self.offset.saturating_sub(delta);
    }

    /// 向下滚动 delta 行（不超过 max_scroll）
    pub fn scroll_down(&mut self, delta: u16, content_height: u16, visible_height: u16) {
        let max_scroll = content_height.saturating_sub(visible_height);
        self.offset = (self.offset + delta).min(max_scroll);
    }

    /// 确保 row 行在可见视口内，自动调整 offset
    ///
    /// 从 `ensure_cursor_visible(cursor_row, scroll_offset, visible_height)` 迁移。
    pub fn ensure_visible(&mut self, row: u16, visible_height: u16) {
        if visible_height == 0 {
            self.offset = 0;
            return;
        }
        if row < self.offset {
            self.offset = row;
        } else if row >= self.offset + visible_height {
            self.offset = row.saturating_sub(visible_height.saturating_sub(1));
        }
    }

    pub fn reset(&mut self) {
        self.offset = 0;
    }
}

/// 可滚动区域——内容 + 可选滚动条
pub struct ScrollableArea<'a> {
    content: Text<'a>,
    show_scrollbar: bool,
    scrollbar_style: Style,
}

impl<'a> ScrollableArea<'a> {
    pub fn new(content: Text<'a>) -> Self {
        Self {
            content,
            show_scrollbar: true,
            scrollbar_style: Style::default(),
        }
    }

    pub fn show_scrollbar(mut self, show: bool) -> Self {
        self.show_scrollbar = show;
        self
    }

    pub fn scrollbar_style(mut self, style: Style) -> Self {
        self.scrollbar_style = style;
        self
    }

    /// 渲染可滚动区域：Paragraph + 可选 Scrollbar
    ///
    /// 自动根据内容高度和可见高度决定是否显示滚动条。
    /// 内容区域宽度减 1 留给滚动条（当 scrollbar 显示时）。
    pub fn render(self, f: &mut Frame, area: Rect, state: &mut ScrollState) {
        let content_height = self.content.height() as u16;
        let visible_height = area.height;
        let max_scroll = content_height.saturating_sub(visible_height);
        // clamp offset
        state.offset = state.offset.min(max_scroll);

        let needs_scrollbar = self.show_scrollbar && content_height > visible_height;
        let text_width = if needs_scrollbar {
            area.width.saturating_sub(1)
        } else {
            area.width
        };
        let text_area = Rect { width: text_width, ..area };

        let paragraph = Paragraph::new(self.content)
            .scroll((state.offset, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, text_area);

        if needs_scrollbar {
            let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
                .position(state.offset as usize);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(self.scrollbar_style);
            f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        backend::TestBackend,
        text::Line,
        Terminal,
    };

    #[test]
    fn scroll_state_ensure_visible_above() {
        let mut state = ScrollState { offset: 5 };
        state.ensure_visible(2, 10);
        assert_eq!(state.offset(), 2);
    }

    #[test]
    fn scroll_state_ensure_visible_below() {
        let mut state = ScrollState { offset: 0 };
        state.ensure_visible(15, 10);
        assert_eq!(state.offset(), 6); // 15 - (10-1) = 6
    }

    #[test]
    fn scroll_state_ensure_visible_within() {
        let mut state = ScrollState { offset: 3 };
        state.ensure_visible(5, 10);
        assert_eq!(state.offset(), 3);
    }

    #[test]
    fn scroll_state_scroll_up_down() {
        let mut state = ScrollState::new();
        state.scroll_down(3, 20, 10);
        assert_eq!(state.offset(), 3);
        state.scroll_up(1);
        assert_eq!(state.offset(), 2);
    }

    #[test]
    fn scrollable_area_renders_content() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let lines: Vec<Line<'_>> = (0..20).map(|i| Line::from(format!("Line {}", i))).collect();
        let content = Text::from(lines);
        let mut scroll_state = ScrollState::new();
        terminal.draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            ScrollableArea::new(content)
                .render(f, area, &mut scroll_state);
        }).unwrap();
    }

    #[test]
    fn scrollable_area_clamps_offset() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let lines: Vec<Line<'_>> = (0..20).map(|i| Line::from(format!("Line {}", i))).collect();
        let content = Text::from(lines);
        let mut scroll_state = ScrollState { offset: 100 };
        terminal.draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            ScrollableArea::new(content)
                .render(f, area, &mut scroll_state);
        }).unwrap();
        // 20 lines, 5 visible -> max_scroll = 15
        assert_eq!(scroll_state.offset(), 15);
    }
}
