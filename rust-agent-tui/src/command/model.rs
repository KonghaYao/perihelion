use crate::app::App;
use super::Command;

pub struct ModelCommand;

impl Command for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self) -> &str {
        "打开 Provider / Model 配置面板"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_model_panel();
    }
}
