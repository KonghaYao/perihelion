use super::Command;
use crate::app::App;

pub struct SplitCommand;

impl Command for SplitCommand {
    fn name(&self) -> &str {
        "split"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-split-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.new_session();
    }
}
