use super::Command;
use crate::app::status_panel::STATUS_TAB_COST;
use crate::app::App;

pub struct CostCommand;

impl Command for CostCommand {
    fn name(&self) -> &str {
        "cost"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-cost-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_status_panel(STATUS_TAB_COST);
    }
}
