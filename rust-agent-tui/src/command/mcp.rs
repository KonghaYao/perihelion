use super::Command;
use crate::app::App;

pub struct McpCommand;

impl Command for McpCommand {
    fn name(&self) -> &str {
        "mcp"
    }

    fn description(&self) -> &str {
        "管理 MCP 服务器连接"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let infos = app
            .mcp_pool
            .as_ref()
            .map(|p| p.server_infos())
            .unwrap_or_default();

        if infos.is_empty() {
            let vm = crate::ui::message_view::MessageViewModel::system(
                "无 MCP 服务器配置（请在 .mcp.json 或 settings.json 中添加）".to_string(),
            );
            app.sessions[app.active].core.view_messages.push(vm.clone());
            let _ = app.sessions[app.active]
                .core
                .render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
            return;
        }

        app.mcp_panel = Some(crate::app::McpPanel::new(infos));
    }
}
