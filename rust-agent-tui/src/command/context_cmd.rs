use super::Command;
use crate::app::status_panel::STATUS_TAB_CONTEXT;
use crate::app::App;

pub struct ContextCommand;

impl Command for ContextCommand {
    fn name(&self) -> &str {
        "context"
    }

    fn description(&self) -> &str {
        "查看上下文使用率和会话统计"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_status_panel(STATUS_TAB_CONTEXT);
    }
}
