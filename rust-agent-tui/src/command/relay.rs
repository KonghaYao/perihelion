use super::Command;
use crate::app::App;

pub struct RelayCommand;

impl Command for RelayCommand {
    fn name(&self) -> &str {
        "relay"
    }

    fn description(&self) -> &str {
        "打开远程控制配置面板"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_relay_panel();
    }
}
