use super::Command;
use crate::app::status_panel::STATUS_TAB_COST;
use crate::app::App;

pub struct CostCommand;

impl Command for CostCommand {
    fn name(&self) -> &str {
        "cost"
    }

    fn description(&self) -> &str {
        "查看当前会话费用和 token 消耗"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_status_panel(STATUS_TAB_COST);
    }
}
