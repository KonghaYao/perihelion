pub mod collapse;
pub mod display;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    widgets::Widget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ToolCallState {
    pub tool_name: String,
    pub args_summary: String,
    pub status: ToolCallStatus,
    pub collapsed: bool,
    pub result_lines: Vec<String>,
    pub is_error: bool,
    pub tick: u64,
    pub color: Color,
    pub omitted_lines: Option<usize>,
}

impl ToolCallState {
    pub fn new(tool_name: String, color: Color) -> Self {
        let collapsed = collapse::should_collapse_by_default(&tool_name);
        Self {
            tool_name,
            args_summary: String::new(),
            status: ToolCallStatus::Pending,
            collapsed,
            result_lines: Vec::new(),
            is_error: false,
            tick: 0,
            color,
            omitted_lines: None,
        }
    }

    pub fn advance_tick(&mut self) {
        self.tick += 1;
    }

    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn set_result(&mut self, content: String) {
        let lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
        let (truncated, omitted) = collapse::truncate_result(&lines, collapse::MAX_RESULT_LINES);
        self.result_lines = truncated;
        self.omitted_lines = omitted;
    }
}

pub struct ToolCallWidget<'a> {
    state: &'a ToolCallState,
}

impl<'a> ToolCallWidget<'a> {
    pub fn new(state: &'a ToolCallState) -> Self {
        Self { state }
    }
}

impl<'a> Widget for ToolCallWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let indicator = display::format_indicator(self.state.status.clone(), self.state.tick);
        let arrow = if self.state.collapsed { "▸" } else { "▾" };

        let mut header_spans: Vec<Span<'_>> = vec![
            Span::styled(
                format!("{} ", indicator),
                Style::default().fg(self.state.color),
            ),
            Span::styled(format!("{} ", arrow), Style::default().fg(self.state.color)),
            Span::styled(
                self.state.tool_name.clone(),
                Style::default()
                    .fg(self.state.color)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if !self.state.args_summary.is_empty() {
            let summary = display::format_args_summary(&self.state.args_summary, 40);
            header_spans.push(Span::styled(
                format!("({})", summary),
                Style::default().fg(ratatui::style::Color::DarkGray),
            ));
        }

        let mut lines: Vec<Line<'_>> = vec![Line::from(header_spans)];

        if !self.state.collapsed && !self.state.result_lines.is_empty() {
            for line in &self.state.result_lines {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(ratatui::style::Color::DarkGray)),
                    Span::raw(line.clone()),
                ]));
            }
            if let Some(omitted) = self.state.omitted_lines {
                lines.push(Line::from(vec![Span::styled(
                    format!("  … ({} more lines)", omitted),
                    Style::default().fg(ratatui::style::Color::DarkGray),
                )]));
            }
        }

        Paragraph::new(lines).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_collapse() {
        let mut state = ToolCallState::new("read_file".to_string(), Color::Blue);
        assert!(state.collapsed, "read_file should collapse by default");
        state.toggle_collapse();
        assert!(!state.collapsed);
    }
}
