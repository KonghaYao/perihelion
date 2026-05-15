use super::Command;
use crate::app::App;
use crate::ui::message_view::MessageViewModel;

pub struct RenameCommand;

impl Command for RenameCommand {
    fn name(&self) -> &str {
        "rename"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-rename-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        let name = args.trim();
        let thread_id = app.session_mgr.current_mut().current_thread_id.clone();

        let Some(thread_id) = thread_id else {
            let vm = MessageViewModel::system("当前无活跃会话，无法重命名".to_string());
            app.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(vm);
            app.render_rebuild();
            return;
        };

        if name.is_empty() {
            // 显示当前标题
            let store = app.services.thread_store.clone();
            let title = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { store.load_meta(&thread_id).await })
                    .ok()
                    .and_then(|m| m.title)
            })
            .unwrap_or_else(|| "(无标题)".to_string());
            let vm = MessageViewModel::system(format!("当前标题: {}", title));
            app.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(vm);
            app.render_rebuild();
        } else {
            // 更新标题
            let store = app.services.thread_store.clone();
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(store.update_title(&thread_id, name))
            });
            match result {
                Ok(()) => {
                    let vm = MessageViewModel::system(format!("会话标题已更新为: {}", name));
                    app.session_mgr
                        .current_mut()
                        .messages
                        .view_messages
                        .push(vm);
                }
                Err(e) => {
                    let vm = MessageViewModel::system(format!("重命名失败: {}", e));
                    app.session_mgr
                        .current_mut()
                        .messages
                        .view_messages
                        .push(vm);
                }
            }
            app.render_rebuild();
        }
    }
}
