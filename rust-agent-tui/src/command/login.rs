use crate::app::App;
use super::Command;

pub struct LoginCommand;

impl Command for LoginCommand {
    fn name(&self) -> &str {
        "login"
    }

    fn description(&self) -> &str {
        "管理 Provider 配置（新建/编辑/删除）"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_login_panel();
    }
}
