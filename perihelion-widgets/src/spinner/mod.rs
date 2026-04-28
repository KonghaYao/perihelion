pub mod animation;
pub mod verb;

use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    widgets::Widget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpinnerMode {
    Thinking,
    ToolUse,
    Responding,
    Idle,
}

pub struct SpinnerState {
    mode: SpinnerMode,
    verb: String,
    start_time: Instant,
    token_count: usize,
    displayed_tokens: usize,
    tick: u64,
}

impl SpinnerState {
    pub fn new(mode: SpinnerMode) -> Self {
        Self {
            mode,
            verb: verb::pick_verb(None),
            start_time: Instant::now(),
            token_count: 0,
            displayed_tokens: 0,
            tick: 0,
        }
    }

    pub fn set_mode(&mut self, mode: SpinnerMode) {
        self.mode = mode;
        self.verb = match &self.mode {
            SpinnerMode::Thinking => "思考中…".to_string(),
            SpinnerMode::ToolUse => "执行工具…".to_string(),
            SpinnerMode::Responding => "正在生成回复…".to_string(),
            SpinnerMode::Idle => String::new(),
        };
    }

    pub fn set_verb(&mut self, active_form: Option<&str>) {
        self.verb = verb::pick_verb(active_form);
    }

    pub fn set_token_count(&mut self, count: usize) {
        self.token_count = count;
    }

    pub fn advance_tick(&mut self) {
        self.tick += 1;
        self.displayed_tokens = animation::smooth_increment(self.displayed_tokens, self.token_count);
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn verb(&self) -> &str {
        &self.verb
    }

    pub fn mode(&self) -> &SpinnerMode {
        &self.mode
    }

    pub fn displayed_tokens(&self) -> usize {
        self.displayed_tokens
    }
}

pub struct SpinnerWidget<'a> {
    state: &'a SpinnerState,
    show_elapsed: bool,
    show_tokens: bool,
}

impl<'a> SpinnerWidget<'a> {
    pub fn new(state: &'a SpinnerState) -> Self {
        Self {
            state,
            show_elapsed: true,
            show_tokens: true,
        }
    }

    pub fn show_elapsed(mut self, show: bool) -> Self {
        self.show_elapsed = show;
        self
    }

    pub fn show_tokens(mut self, show: bool) -> Self {
        self.show_tokens = show;
        self
    }
}

impl<'a> Widget for SpinnerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans: Vec<Span<'_>> = vec![];

        let frame = animation::tick_to_frame(self.state.tick());
        spans.push(Span::styled(
            format!("{} ", frame),
            Style::default().fg(Color::Cyan),
        ));

        spans.push(Span::styled(
            self.state.verb().to_string(),
            Style::default().fg(Color::White),
        ));

        if self.show_elapsed {
            let elapsed = self.state.elapsed_ms();
            if elapsed > 30_000 {
                spans.push(Span::raw(format!(" {}", animation::format_elapsed(elapsed))));
            }
        }

        if self.show_tokens && self.state.displayed_tokens() > 0 {
            spans.push(Span::raw(format!(
                " {} tokens",
                self.state.displayed_tokens()
            )));
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
