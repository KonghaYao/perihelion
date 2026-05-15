use super::Command;
use crate::app::App;

pub struct LoginCommand;

impl Command for LoginCommand {
    fn name(&self) -> &str {
        "login"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-login-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_login_panel();
    }
}
