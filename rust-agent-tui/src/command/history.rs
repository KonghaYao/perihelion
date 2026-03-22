use crate::app::{App, MessageViewModel};
use super::Command;

pub struct HistoryCommand;

impl Command for HistoryCommand {
    fn name(&self) -> &str {
        "history"
    }

    fn description(&self) -> &str {
        "打开历史对话浏览面板"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        if app.loading {
            app.view_messages.push(MessageViewModel::system(
                "Agent 运行中，无法打开历史面板".to_string(),
            ));
            return;
        }
        app.open_thread_browser();
    }
}
