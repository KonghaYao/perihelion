use super::Command;
use crate::app::{App, MessageViewModel};

pub struct CompactCommand;

impl Command for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-compact-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
            app.session_mgr.sessions[app.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    "Agent 运行中，无法执行压缩".to_string(),
                ));
            return;
        }
        app.start_compact(args.to_string());
    }
}
