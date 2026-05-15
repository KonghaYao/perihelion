use super::Command;
use crate::app::App;

pub struct PluginCommand;

impl Command for PluginCommand {
    fn name(&self) -> &str {
        "plugin"
    }
    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-plugin-description")
    }
    fn execute(&self, app: &mut App, _args: &str) {
        app.open_plugin_panel();
    }
}
