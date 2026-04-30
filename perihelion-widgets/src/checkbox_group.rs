use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::StatefulWidget,
};

/// 多选按钮组状态
#[derive(Debug, Clone)]
pub struct CheckboxState {
    checked: Vec<bool>,
    cursor: usize,
}

impl CheckboxState {
    pub fn new(count: usize) -> Self {
        Self {
            checked: vec![false; count],
            cursor: 0,
        }
    }

    pub fn toggle(&mut self) {
        if self.cursor < self.checked.len() {
            self.checked[self.cursor] = !self.checked[self.cursor];
        }
    }

    pub fn select_all(&mut self) {
        for c in &mut self.checked {
            *c = true;
        }
    }

    pub fn select_none(&mut self) {
        for c in &mut self.checked {
            *c = false;
        }
    }

    pub fn move_cursor(&mut self, delta: i32) {
        if self.checked.is_empty() {
            return;
        }
        let max = self.checked.len() - 1;
        let new = self.cursor as i32 + delta;
        self.cursor = new.clamp(0, max as i32) as usize;
    }

    pub fn is_checked(&self, index: usize) -> bool {
        self.checked.get(index).copied().unwrap_or(false)
    }

    pub fn checked_indices(&self) -> Vec<usize> {
        self.checked
            .iter()
            .enumerate()
            .filter(|(_, &v)| v)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.checked.len()
    }
}

/// 多选按钮组 widget
pub struct CheckboxGroup<'a> {
    labels: Vec<&'a str>,
    checked_char: char,
    unchecked_char: char,
    cursor_style: Style,
    normal_style: Style,
}

impl<'a> CheckboxGroup<'a> {
    pub fn new(labels: Vec<&'a str>) -> Self {
        Self {
            labels,
            checked_char: '✓',
            unchecked_char: '✗',
            cursor_style: Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(Modifier::BOLD),
            normal_style: Style::default(),
        }
    }

    pub fn checked_char(mut self, checked: char, unchecked: char) -> Self {
        self.checked_char = checked;
        self.unchecked_char = unchecked;
        self
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }
}

impl StatefulWidget for CheckboxGroup<'_> {
    type State = CheckboxState;

    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        if self.labels.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, label) in self.labels.iter().enumerate() {
            let is_cursor = i == state.cursor;
            let checked = state.is_checked(i);
            let icon = if checked {
                self.checked_char
            } else {
                self.unchecked_char
            };
            let style = if is_cursor {
                self.cursor_style
            } else {
                self.normal_style
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(label.to_string(), style),
            ]));
        }
        let text = ratatui::text::Text::from(lines);
        for (i, line) in text.lines.iter().enumerate() {
            if area.y as usize + i < buf.area.height as usize {
                let _ = buf.set_line(area.x, area.y + i as u16, line, area.width);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_state_toggle() {
        let mut state = CheckboxState::new(3);
        state.toggle();
        assert!(state.is_checked(0));
        state.toggle();
        assert!(!state.is_checked(0));
    }

    #[test]
    fn checkbox_state_select_all_none() {
        let mut state = CheckboxState::new(3);
        state.select_all();
        assert_eq!(state.checked_indices(), vec![0, 1, 2]);
        state.select_none();
        assert_eq!(state.checked_indices(), Vec::<usize>::new());
    }

    #[test]
    fn checkbox_state_move_cursor() {
        let mut state = CheckboxState::new(3);
        state.move_cursor(1);
        assert_eq!(state.cursor(), 1);
        state.move_cursor(100);
        assert_eq!(state.cursor(), 2); // clamped
    }
}
