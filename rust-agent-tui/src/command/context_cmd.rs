use super::Command;
use crate::app::status_panel::STATUS_TAB_CONTEXT;
use crate::app::App;

pub struct ContextCommand;

impl Command for ContextCommand {
    fn name(&self) -> &str {
        "context"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-context-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_status_panel(STATUS_TAB_CONTEXT);
    }
}
