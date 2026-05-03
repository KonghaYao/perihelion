use super::Command;
use crate::app::App;

pub struct SplitCommand;

impl Command for SplitCommand {
    fn name(&self) -> &str {
        "split"
    }

    fn description(&self) -> &str {
        "新建分栏会话"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.new_session();
    }
}
