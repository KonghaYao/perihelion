use crate::app::{App, ChatMessage};
use super::Command;

pub struct HelpCommand;

impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "列出所有可用命令"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 借用问题：先收集列表，再推消息
        let list: Vec<(String, String)> = app
            .command_registry
            .list()
            .into_iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect();

        let mut lines = vec!["可用命令：".to_string()];
        for (name, desc) in &list {
            lines.push(format!("  /{:<10} {}", name, desc));
        }

        app.messages.push(ChatMessage::system(lines.join("\n")));
    }
}
