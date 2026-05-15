use super::Command;
use crate::app::App;

pub struct McpCommand;

impl Command for McpCommand {
    fn name(&self) -> &str {
        "mcp"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-mcp-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_mcp_panel();
    }
}
