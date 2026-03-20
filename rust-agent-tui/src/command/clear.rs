use crate::app::App;
use super::Command;

pub struct ClearCommand;

impl Command for ClearCommand {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "清空消息列表"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.messages.clear();
        app.todo_message_index = None;
    }
}
