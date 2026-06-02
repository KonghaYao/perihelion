use tui_textarea::{Input, Key};

use crate::app::{
    panel_manager::{EventResult, PanelContext},
    plugin_panel::PluginPanel,
    AgentEvent,
};

impl PluginPanel {
    pub(crate) fn handle_confirm_delete(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input {
                key: Key::Enter, ..
            } => {
                let (plugin_id, project_path) = if let Some(id) = self.confirm_delete.clone() {
                    let entry = self.entries.iter().find(|e| e.id == id);
                    let project_path = entry.and_then(|e| e.project_path.clone());
                    (Some(id), project_path)
                } else {
                    (None, None)
                };

                if let Some(plugin_id) = plugin_id {
                    self.uninstalling.insert(plugin_id.clone());
                    self.confirm_delete = None;

                    let tx = ctx.services.bg_event_tx.clone();
                    let claude_dir = peri_middlewares::plugin::claude_home();
                    let project_dir = project_path.map(std::path::PathBuf::from);
                    tokio::spawn(async move {
                        let result = peri_middlewares::plugin::uninstall_plugin(
                            &plugin_id,
                            &claude_dir,
                            project_dir.as_deref(),
                        )
                        .await;
                        let success = result.is_ok();
                        let message = if let Err(e) = result {
                            format!("\u{5378}\u{8f7d}\u{5931}\u{8d25}: {e}")
                        } else {
                            "\u{5378}\u{8f7d}\u{6210}\u{529f}".to_string()
                        };
                        let _ = tx.try_send(AgentEvent::PluginActionCompleted {
                            plugin_id,
                            action: "uninstall".to_string(),
                            success,
                            message,
                        });
                    });
                } else {
                    self.confirm_delete = None;
                }
                EventResult::Consumed
            }
            _ => {
                self.confirm_delete = None;
                EventResult::Consumed
            }
        }
    }

    pub(super) fn handle_marketplace_confirm_delete(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input { key: Key::Esc, .. } => {
                self.marketplace_confirm_delete = None;
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                if let Some(idx) = self.marketplace_confirm_delete.take() {
                    if let Some(entry) = self.marketplace_entries.get(idx) {
                        let name = entry.name.clone();
                        self.marketplace_entries.remove(idx);
                        self.sync_marketplace_list_items();

                        if let Err(e) = self.persist_marketplace_delete(&name) {
                            ctx.session_mgr.current_mut().messages.push_system_note(
                                ctx.services.lc.tr_args(
                                    "app-plugin-delete-failed",
                                    &[("error".into(), e.to_string().into())],
                                ),
                            );
                        }
                    }
                }
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
}
