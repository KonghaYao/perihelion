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
    raw_tick: u64,
    /// 最后一次从非 Idle 切换到 Idle 时捕获的耗时（ms），0 表示无记录
    last_summary_elapsed_ms: u64,
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
            raw_tick: 0,
            last_summary_elapsed_ms: 0,
        }
    }

    pub fn set_mode(&mut self, mode: SpinnerMode) {
        let was_active = self.mode != SpinnerMode::Idle;
        self.mode = mode;
        self.verb = match &self.mode {
            SpinnerMode::Thinking => "思考中…".to_string(),
            SpinnerMode::ToolUse => "执行工具…".to_string(),
            SpinnerMode::Responding => "正在生成回复…".to_string(),
            SpinnerMode::Idle => String::new(),
        };
        // 从活跃状态切换到 Idle 时，记录耗时用于总结行
        if was_active && self.mode == SpinnerMode::Idle {
            self.last_summary_elapsed_ms = self.elapsed_ms();
        }
        // 从 Idle 切换到活跃状态时，重置总结记录
        if !was_active && self.mode != SpinnerMode::Idle {
            self.last_summary_elapsed_ms = 0;
        }
    }

    pub fn set_verb(&mut self, active_form: Option<&str>) {
        self.verb = verb::pick_verb(active_form);
    }

    pub fn set_token_count(&mut self, count: usize) {
        self.token_count = count;
    }

    pub fn advance_tick(&mut self) {
        self.raw_tick = self.raw_tick.wrapping_add(1);
        self.displayed_tokens = animation::smooth_increment(self.displayed_tokens, self.token_count);
        // 每 2 个 raw tick 才推进一帧（星号旋转更快）
        if self.raw_tick % 2 == 0 {
            self.tick += 1;
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn raw_tick(&self) -> u64 {
        self.raw_tick
    }

    pub fn verb(&self) -> &str {
        &self.verb
    }

    pub fn mode(&self) -> &SpinnerMode {
        &self.mode
    }

    pub fn last_summary_elapsed_ms(&self) -> u64 {
        self.last_summary_elapsed_ms
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
        let orange = Style::default().fg(Color::Rgb(255, 140, 0));
        let gray = Style::default().fg(Color::Gray);

        spans.push(Span::styled(format!("{} ", frame), orange));

        spans.push(Span::styled(self.state.verb().to_string(), orange));

        let elapsed = self.state.elapsed_ms();
        let displayed_tokens = self.state.displayed_tokens();

        let mut suffix_parts = Vec::new();

        if self.show_elapsed {
            suffix_parts.push(animation::format_elapsed(elapsed));
        }

        if self.show_tokens && displayed_tokens > 0 {
            suffix_parts.push(format!("↓ {} tokens", animation::format_tokens(displayed_tokens)));
        }

        if !suffix_parts.is_empty() {
            spans.push(Span::styled(
                format!(" ({}", suffix_parts.join(" · ")),
                gray,
            ));
            spans.push(Span::styled(")", gray));
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
