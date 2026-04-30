use crate::scrollable::ScrollState;
use ratatui::{
    layout::Rect,
    prelude::*,
    style::Style,
    text::{Line, Text},
    widgets::{Paragraph, StatefulWidget, Widget},
};

/// 泛型列表状态——管理 items + cursor + scroll offset
///
/// T 不要求 Clone。cursor 使用 clamp 模式（不循环）。
/// 内嵌 ScrollState，滚动与光标联动通过 ensure_visible 自动处理。
pub struct ListState<T> {
    items: Vec<T>,
    cursor: usize,
    pub scroll: ScrollState,
}

impl<T> ListState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            cursor: 0,
            scroll: ScrollState::new(),
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 移动光标（clamp 模式，不循环）
    pub fn move_cursor(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len() - 1;
        let new = self.cursor as i32 + delta;
        self.cursor = new.clamp(0, max as i32) as usize;
    }

    /// 确保 cursor 不超过 items.len()（外部修改 items 后调用）
    pub fn clamp_cursor(&mut self) {
        if self.items.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(self.items.len() - 1);
        }
    }

    /// 获取当前 cursor 指向的 item 引用
    pub fn selected(&self) -> Option<&T> {
        self.items.get(self.cursor)
    }

    /// 获取当前 cursor 指向的 item 可变引用
    pub fn selected_mut(&mut self) -> Option<&mut T> {
        self.items.get_mut(self.cursor)
    }

    /// 确保 cursor 行在可见视口内（联动 ScrollState）
    pub fn ensure_visible(&mut self, visible: u16) {
        self.scroll.ensure_visible(self.cursor as u16, visible);
    }

    /// 替换 items 列表，自动 clamp cursor
    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.clamp_cursor();
    }
}

/// 可选择列表 widget——通过闭包自定义每项渲染
///
/// 实现 ratatui StatefulWidget trait，状态类型为 ListState<T>。
/// render_item 闭包签名为 `Fn(&T, bool) -> Line<'a>`，bool 表示当前行是否为 cursor。
/// "特殊首项"模式由调用方在闭包中处理（如 items[0] 是 "New Thread"）。
pub struct SelectableList<'a, T> {
    render_item: Box<dyn Fn(&T, bool) -> Line<'a>>,
    cursor_marker: &'a str,
    cursor_style: Style,
    normal_style: Style,
}

impl<'a, T> SelectableList<'a, T> {
    pub fn new(render_item: impl Fn(&T, bool) -> Line<'a> + 'static) -> Self {
        Self {
            render_item: Box::new(render_item),
            cursor_marker: "▶ ",
            cursor_style: Style::default(),
            normal_style: Style::default(),
        }
    }

    pub fn cursor_marker(mut self, marker: &'a str) -> Self {
        self.cursor_marker = marker;
        self
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn normal_style(mut self, style: Style) -> Self {
        self.normal_style = style;
        self
    }
}

impl<T> StatefulWidget for SelectableList<'_, T> {
    type State = ListState<T>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let cursor = state.cursor;

        let mut lines: Vec<Line<'_>> = Vec::with_capacity(state.items.len());
        for (i, item) in state.items.iter().enumerate() {
            let is_cursor = i == cursor;
            let line = (self.render_item)(item, is_cursor);
            let marker = if is_cursor {
                Span::styled(self.cursor_marker.to_string(), self.cursor_style)
            } else {
                Span::styled(
                    " ".repeat(self.cursor_marker.chars().count()),
                    self.normal_style,
                )
            };
            let mut spans = vec![marker];
            spans.extend(line.spans.iter().cloned());
            lines.push(Line::from(spans));
        }

        let text = Text::from(lines);
        let visible = area.height;
        state.scroll.ensure_visible(cursor as u16, visible);

        let paragraph = Paragraph::new(text).scroll((state.scroll.offset(), 0));
        Widget::render(paragraph, area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn list_state_move_cursor_clamp() {
        let mut state = ListState::new(vec!["a", "b", "c"]);
        state.move_cursor(1);
        assert_eq!(state.cursor(), 1);
        state.move_cursor(5);
        assert_eq!(state.cursor(), 2); // clamped to max
        state.move_cursor(-10);
        assert_eq!(state.cursor(), 0); // clamped to 0
    }

    #[test]
    fn list_state_empty_items() {
        let mut state: ListState<&str> = ListState::new(vec![]);
        state.move_cursor(1); // should not panic
        assert!(state.selected().is_none());
    }

    #[test]
    fn list_state_set_items_clamp_cursor() {
        let mut state = ListState::new(vec!["a", "b", "c", "d"]);
        // move cursor to last item
        state.move_cursor(3);
        assert_eq!(state.cursor(), 3);
        // replace with shorter list
        state.set_items(vec!["x"]);
        assert_eq!(state.cursor(), 0); // clamped
    }

    #[test]
    fn list_state_ensure_visible() {
        let mut state = ListState::new((0..20i32).collect());
        state.move_cursor(15);
        state.ensure_visible(10);
        assert_eq!(state.scroll.offset(), 6); // 15 - (10-1)
    }

    #[test]
    fn selectable_list_renders_cursor_marker() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = ListState::new(vec!["a", "b", "c"]);
        state.move_cursor(1); // cursor on "b"
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 20, 5);
                let list = SelectableList::new(|item: &&str, is_cursor: bool| Line::from(*item));
                f.render_stateful_widget(list, area, &mut state);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let area = buf.area;
        // Row 1 (cursor) should contain the marker prefix
        let row1: String = (area.x..area.x + area.width)
            .map(|x| buf.cell((x, area.y + 1)).unwrap().symbol().to_string())
            .collect();
        assert!(
            row1.contains("▶"),
            "Expected cursor marker on row 1, got: {:?}",
            row1
        );
        // Row 0 should contain spaces (non-cursor marker)
        let row0: String = (area.x..area.x + area.width)
            .map(|x| buf.cell((x, area.y)).unwrap().symbol().to_string())
            .collect();
        assert!(!row0.contains("▶"), "Expected no cursor marker on row 0");
    }

    #[test]
    fn selectable_list_custom_render_item() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = ListState::new(vec!["a", "b"]);
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 20, 5);
                let list = SelectableList::new(|item: &&str, _is_cursor: bool| {
                    Line::from(format!("[{}]", item))
                });
                f.render_stateful_widget(list, area, &mut state);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let area = buf.area;
        let row0: String = (area.x..area.x + area.width)
            .map(|x| buf.cell((x, area.y)).unwrap().symbol().to_string())
            .collect();
        assert!(
            row0.contains("[a]"),
            "Expected [a] on row 0, got: {:?}",
            row0
        );
    }
}
