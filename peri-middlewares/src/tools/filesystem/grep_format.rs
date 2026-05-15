use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use grep::searcher::{Searcher, Sink, SinkContext, SinkContextKind, SinkMatch};

use super::grep_args::OutputMode;

/// 自定义 Sink，支持多种输出模式和行数限制
pub(crate) struct SearchSink {
    pub(crate) output_mode: OutputMode,
    pub(crate) results: Arc<Mutex<Vec<String>>>,
    pub(crate) total_lines: Arc<AtomicUsize>,
    pub(crate) max_limit: usize,
    pub(crate) stopped: Arc<AtomicBool>,
    pub(crate) display_path: String,
    pub(crate) match_count: Cell<usize>,
    pub(crate) has_match: Cell<bool>,
    pub(crate) after_context: usize,
    pub(crate) before_context: usize,
    pub(crate) show_line_numbers: bool,
}

impl Sink for SearchSink {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        if self.stopped.load(Ordering::Relaxed) {
            return Ok(false);
        }

        match self.output_mode {
            OutputMode::Default => {
                let line_number = mat.line_number().unwrap_or(0);
                let content = String::from_utf8_lossy(mat.bytes());
                let content = content.trim_end_matches(['\n', '\r']);
                let line = if self.show_line_numbers {
                    format!("{}:{}: {}", self.display_path, line_number, content)
                } else {
                    format!("{}: {}", self.display_path, content)
                };

                let total = self.total_lines.fetch_add(1, Ordering::Relaxed) + 1;
                if self.max_limit > 0 && total >= self.max_limit {
                    self.stopped.store(true, Ordering::Relaxed);
                }

                self.results.lock().unwrap().push(line);
                Ok(!self.stopped.load(Ordering::Relaxed))
            }
            OutputMode::CountOnly => {
                self.match_count.set(self.match_count.get() + 1);
                Ok(true)
            }
            OutputMode::FilesOnly => {
                self.has_match.set(true);
                Ok(false)
            }
            OutputMode::FilesWithoutMatch => {
                self.has_match.set(true);
                Ok(true) // 不 early return，需确认文件无匹配
            }
        }
    }

    fn context(
        &mut self,
        _searcher: &Searcher,
        ctx: &SinkContext<'_>,
    ) -> Result<bool, Self::Error> {
        if self.stopped.load(Ordering::Relaxed) {
            return Ok(true);
        }
        if self.output_mode != OutputMode::Default {
            return Ok(true);
        }
        // 非对称上下文：before 和 after 分别控制
        match ctx.kind() {
            SinkContextKind::After if self.after_context == 0 => return Ok(true),
            SinkContextKind::Before if self.before_context == 0 => return Ok(true),
            _ => {}
        }

        let line_number = ctx.line_number().unwrap_or(0);
        let content = String::from_utf8_lossy(ctx.bytes());
        let content = content.trim_end_matches(['\n', '\r']);

        let separator = match ctx.kind() {
            SinkContextKind::Before => '-',
            SinkContextKind::After => '+',
            SinkContextKind::Other => '-',
        };

        let line = if self.show_line_numbers {
            format!(
                "{}:{}{}: {}",
                self.display_path, line_number, separator, content
            )
        } else {
            format!("{}{}: {}", self.display_path, separator, content)
        };

        let total = self.total_lines.fetch_add(1, Ordering::Relaxed) + 1;
        if total >= self.max_limit {
            self.stopped.store(true, Ordering::Relaxed);
        }

        self.results.lock().unwrap().push(line);
        Ok(!self.stopped.load(Ordering::Relaxed))
    }
}
