use super::Command;
use crate::app::App;

pub struct MemoryCommand;

impl Command for MemoryCommand {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "编辑用户/项目级 CLAUDE.md 记忆文件"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_memory_panel();
    }
}
