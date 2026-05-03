use super::Command;
use crate::app::App;

pub struct ClearCommand;

impl Command for ClearCommand {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "清空消息列表"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["reset", "new"]
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.new_thread();
    }
}
