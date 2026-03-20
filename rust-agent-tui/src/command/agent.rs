use crate::app::App;
use crate::app::ChatMessage;
use crate::command::Command;

pub struct AgentCommand;

impl Command for AgentCommand {
    fn name(&self) -> &str {
        "agent"
    }

    fn description(&self) -> &str {
        "/agent <id> - 设置 Agent 定义，切换不同的 Agent 角色"
    }

    fn execute(&self, app: &mut App, args: &str) {
        let id = args.trim();
        if id.is_empty() {
            // 清除 agent_id
            app.set_agent_id(None);
            app.messages.push(ChatMessage::system(
                "Agent 已重置（未设置 agent_id）".to_string(),
            ));
        } else {
            app.set_agent_id(Some(id.to_string()));
            let name = rust_agent_middlewares::format_agent_id(id);
            app.messages.push(ChatMessage::system(format!(
                "Agent 已切换为: {} ({})",
                name, id
            )));
        }
    }
}
