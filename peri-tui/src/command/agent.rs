use crate::app::{App, MessageViewModel};
use crate::command::Command;

pub struct AgentCommand;

impl Command for AgentCommand {
    fn name(&self) -> &str {
        "agent"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-agent-description").into()
    }

    fn execute(&self, app: &mut App, args: &str) {
        let id = args.trim();
        if id.is_empty() {
            // 清除 agent_id
            app.set_agent_id(None);
            app.session_mgr.current_mut().messages.view_messages.push(MessageViewModel::system(
                "Agent 已重置（未设置 agent_id）".to_string(),
            ));
        } else {
            app.set_agent_id(Some(id.to_string()));
            let name = peri_middlewares::format_agent_id(id);
            app.session_mgr.current_mut().messages.view_messages.push(MessageViewModel::system(format!(
                "Agent 已切换为: {} ({})",
                name, id
            )));
        }
    }
}
