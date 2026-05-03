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

    fn description(&self) -> &str {
        "全局配置（autocompact、语言、系统提示词覆盖）"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_config_panel();
    }
}
