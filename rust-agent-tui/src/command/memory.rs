use super::Command;
use crate::app::App;

pub struct MemoryCommand;

impl Command for MemoryCommand {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-memory-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_memory_panel();
    }
}
