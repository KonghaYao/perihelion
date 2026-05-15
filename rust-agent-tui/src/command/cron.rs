use super::Command;
use crate::app::App;

pub struct CronCommand;

impl Command for CronCommand {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-cron-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_cron_panel();
    }
}
