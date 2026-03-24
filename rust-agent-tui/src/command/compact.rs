use crate::app::App;
use super::Command;

pub struct CompactCommand;

impl Command for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self) -> &str {
        "压缩对话上下文（调用 LLM 生成摘要）"
    }

    fn execute(&self, app: &mut App, args: &str) {
        app.start_compact(args.to_string());
    }
}
