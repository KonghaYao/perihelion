pub mod blocks;
pub mod highlight;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::Paragraph,
    widgets::Widget,
};

pub use blocks::BlockRenderStrategy;

pub struct MessageBlockState {
    blocks: Vec<BlockRenderStrategy>,
}

impl MessageBlockState {
    pub fn new(blocks: Vec<BlockRenderStrategy>) -> Self {
        Self { blocks }
    }

    pub fn blocks(&self) -> &[BlockRenderStrategy] {
        &self.blocks
    }
}

pub struct MessageBlockWidget<'a> {
    state: &'a MessageBlockState,
    width: usize,
}

impl<'a> MessageBlockWidget<'a> {
    pub fn new(state: &'a MessageBlockState) -> Self {
        Self { state, width: 80 }
    }

    pub fn width(mut self, w: usize) -> Self {
        self.width = w;
        self
    }
}

impl<'a> Widget for MessageBlockWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut all_lines: Vec<Line<'_>> = Vec::new();
        for block in &self.state.blocks {
            let lines = blocks::render_block(block, self.width);
            all_lines.extend(lines);
        }
        Paragraph::new(all_lines).render(area, buf);
    }
}
