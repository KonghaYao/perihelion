use super::Command;
use crate::app::{App, MessageViewModel};

pub struct CompactCommand;

impl Command for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self) -> &str {
        "压缩对话上下文（结构化摘要 + 重新注入最近文件/Skills）"
    }

    fn execute(&self, app: &mut App, args: &str) {
        if app.core.loading {
            app.core.view_messages.push(MessageViewModel::system(
                "Agent 运行中，无法执行压缩".to_string(),
            ));
            return;
        }
        app.start_compact(args.to_string());
    }
}
