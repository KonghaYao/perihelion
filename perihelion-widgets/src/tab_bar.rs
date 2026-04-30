use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::StatefulWidget,
};

/// Tab 导航状态
#[derive(Debug, Clone)]
pub struct TabState {
    active: usize,
    labels: Vec<String>,
    /// 每个标签的可选指示字符（如 Some('✓') 表示已完成）
    indicators: Vec<Option<char>>,
}

impl TabState {
    pub fn new(labels: Vec<String>) -> Self {
        let len = labels.len();
        Self {
            active: 0,
            labels,
            indicators: vec![None; len],
        }
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if !self.labels.is_empty() {
            self.active = index % self.labels.len();
        }
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn next(&mut self) {
        if !self.labels.is_empty() {
            self.active = (self.active + 1) % self.labels.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.labels.is_empty() {
            self.active = (self.active + self.labels.len() - 1) % self.labels.len();
        }
    }

    pub fn set_indicator(&mut self, index: usize, indicator: Option<char>) {
        if index < self.indicators.len() {
            self.indicators[index] = indicator;
        }
    }

    pub fn label(&self, index: usize) -> &str {
        self.labels.get(index).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn indicator(&self, index: usize) -> Option<char> {
        self.indicators.get(index).copied().flatten()
    }
}

/// Tab 栏样式配置
#[derive(Debug, Clone)]
pub struct TabStyle {
    pub active: Style,
    pub completed: Style,
    pub incomplete: Style,
    pub separator: &'static str,
}

impl Default for TabStyle {
    fn default() -> Self {
        Self {
            active: Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(Modifier::BOLD),
            completed: Style::default().fg(ratatui::style::Color::Green),
            incomplete: Style::default().fg(ratatui::style::Color::DarkGray),
            separator: " │ ",
        }
    }
}

/// Tab 标签导航栏 widget
pub struct TabBar {
    style: TabStyle,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            style: TabStyle::default(),
        }
    }

    pub fn style(mut self, style: TabStyle) -> Self {
        self.style = style;
        self
    }
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for TabBar {
    type State = TabState;

    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        if state.labels.is_empty() || area.width < 3 {
            return;
        }
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (i, label) in state.labels.iter().enumerate() {
            let indicator = state.indicators.get(i).copied().flatten();
            let indicator_str = indicator
                .map(|c| c.to_string())
                .unwrap_or_else(|| " ".to_string());
            let style = if i == state.active {
                self.style.active
            } else if indicator.is_some() {
                self.style.completed
            } else {
                self.style.incomplete
            };
            spans.push(Span::styled(
                format!(" {} {} ", indicator_str, label),
                style,
            ));
            if i < state.labels.len() - 1 {
                spans.push(Span::styled(
                    self.style.separator.to_string(),
                    self.style.incomplete,
                ));
            }
        }
        let line = Line::from(spans);
        let _ = buf.set_line(area.x, area.y, &line, area.width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn tab_state_navigation() {
        let mut state = TabState::new(vec!["A".into(), "B".into(), "C".into()]);
        state.next();
        assert_eq!(state.active(), 1);
        state.prev();
        assert_eq!(state.active(), 0);
        state.prev();
        assert_eq!(state.active(), 2); // wraps around
    }

    #[test]
    fn tab_state_indicator() {
        let mut state = TabState::new(vec!["A".into(), "B".into(), "C".into()]);
        state.set_indicator(1, Some('✓'));
        assert_eq!(state.indicator(1), Some('✓'));
        assert_eq!(state.indicator(0), None);
    }

    #[test]
    fn tab_bar_render() {
        let backend = TestBackend::new(30, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = TabState::new(vec!["Tab1".into(), "Tab2".into()]);
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 30, 3);
                f.render_stateful_widget(TabBar::new(), area, &mut state);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let row: String = (0..30)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(
            row.contains("Tab1"),
            "Expected Tab1 in output, got: {:?}",
            row
        );
        assert!(
            row.contains("Tab2"),
            "Expected Tab2 in output, got: {:?}",
            row
        );
    }
}
