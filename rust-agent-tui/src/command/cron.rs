use super::Command;
use crate::app::{App, CronPanel};
use crate::ui::message_view::MessageViewModel;
use crate::ui::render_thread::RenderEvent;

pub struct CronCommand;

impl Command for CronCommand {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "查看和管理定时任务"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let tasks: Vec<_> = app
            .cron
            .scheduler
            .lock()
            .list_tasks()
            .into_iter()
            .cloned()
            .collect();

        if tasks.is_empty() {
            let vm = MessageViewModel::system("无定时任务".to_string());
            app.core.view_messages.push(vm.clone());
            let _ = app.core.render_tx.send(RenderEvent::AddMessage(vm));
            return;
        }

        app.cron.cron_panel = Some(CronPanel::new(tasks));
    }
}
