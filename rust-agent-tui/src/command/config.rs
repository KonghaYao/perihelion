use super::Command;
use crate::app::App;

pub struct ConfigCommand;

impl Command for ConfigCommand {
    fn name(&self) -> &str {
        "config"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["settings"]
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-config-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_config_panel();
    }
}
