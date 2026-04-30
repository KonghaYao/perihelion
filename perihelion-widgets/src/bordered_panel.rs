use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear},
    Frame,
};

/// 带边框容器——封装 Clear + Block + borders 一步到位
///
/// render() 返回 inner Rect 供后续渲染使用。
pub struct BorderedPanel<'a> {
    title: Line<'a>,
    border_style: Style,
}

impl<'a> BorderedPanel<'a> {
    pub fn new(title: impl Into<Line<'a>>) -> Self {
        Self {
            title: title.into(),
            border_style: Style::default(),
        }
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// 渲染边框面板：先 Clear 背景，再渲染 Block 边框，返回 inner area
    pub fn render(self, f: &mut Frame, area: Rect) -> Rect {
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(self.title)
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(self.border_style);
        let inner = block.inner(area);
        f.render_widget(&block, area);
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_returns_inner_area() {
        let backend = TestBackend::new(10, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 10, 6);
        let mut inner = Rect::default();
        terminal
            .draw(|f| {
                inner = BorderedPanel::new("Title")
                    .border_style(Style::default())
                    .render(f, area);
            })
            .unwrap();
        // inner width = 10 (no left/right borders)
        assert_eq!(inner.width, 10);
        // inner height = 6 - 2 (top + bottom borders) = 4
        assert_eq!(inner.height, 4);
    }

    #[test]
    fn render_with_empty_title() {
        let backend = TestBackend::new(10, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 10, 6);
        let mut inner = Rect::default();
        terminal
            .draw(|f| {
                inner = BorderedPanel::new("")
                    .border_style(Style::default())
                    .render(f, area);
            })
            .unwrap();
        assert_eq!(inner.width, 10);
        assert_eq!(inner.height, 4);
    }
}
