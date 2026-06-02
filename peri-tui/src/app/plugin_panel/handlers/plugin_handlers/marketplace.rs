use tui_textarea::{Input, Key};

use peri_widgets::InputState;

use crate::app::{
    panel_manager::{EventResult, PanelContext},
    plugin_panel::PluginPanel,
    AgentEvent,
};

impl PluginPanel {
    pub(crate) fn handle_marketplaces_list(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        // marketplace_confirm_delete 子状态
        if self.marketplace_confirm_delete.is_some() {
            return self.handle_marketplace_confirm_delete(input, ctx);
        }

        // add_marketplace_active 子状态
        if self.add_marketplace_active {
            return self.handle_marketplace_add(input, ctx);
        }

        // 默认列表视图
        match input {
            Input {
                key: Key::Right, ..
            }
            | Input { key: Key::Tab, .. } => {
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.marketplace_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.marketplace_list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                if self.marketplace_list.cursor() == 0 {
                    self.add_marketplace_input = InputState::new();
                    self.add_marketplace_active = true;
                } else if let Some(entry) = self
                    .marketplace_entries
                    .get(self.marketplace_list.cursor() - 1)
                {
                    let name = entry.name.clone();
                    let source = entry.source.clone();
                    self.marketplace_updating.insert(name.clone());
                    let name_for_msg = name.clone();
                    let source_for_update = source.clone();
                    let tx = ctx.services.bg_event_tx.clone();
                    tokio::spawn(async move {
                        let result = peri_middlewares::plugin::marketplace::refresh_marketplace(
                            &source, &name,
                        )
                        .await;
                        match result {
                            Ok((_manifest, install_location)) => {
                                if let Ok(mut marketplaces) =
                                    peri_middlewares::plugin::load_known_marketplaces(None)
                                {
                                    if let Some(km) = marketplaces
                                        .iter_mut()
                                        .find(|km| km.source == source_for_update)
                                    {
                                        km.install_location = install_location;
                                        km.last_updated = chrono::Utc::now().to_rfc3339();
                                        let _ = peri_middlewares::plugin::save_known_marketplaces(
                                            &marketplaces,
                                            None,
                                        );
                                    }
                                }
                                let _ = tx
                                    .send(AgentEvent::PluginActionCompleted {
                                        plugin_id: name.clone(),
                                        action: "refresh".to_string(),
                                        success: true,
                                        message: format!(
                                            "Marketplace '{}' \u{5df2}\u{66f4}\u{65b0}",
                                            name
                                        ),
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(AgentEvent::PluginActionCompleted {
                                        plugin_id: name.clone(),
                                        action: "refresh".to_string(),
                                        success: false,
                                        message: format!("\u{66f4}\u{65b0}\u{5931}\u{8d25}: {}", e),
                                    })
                                    .await;
                            }
                        }
                    });
                    ctx.session_mgr.current_mut().messages.push_system_note(
                        ctx.services.lc.tr_args(
                            "app-plugin-updating",
                            &[("name".into(), name_for_msg.into())],
                        ),
                    );
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                if self.marketplace_list.cursor() > 0 {
                    let idx = self.marketplace_list.cursor() - 1;
                    if self.marketplace_entries.get(idx).is_some() {
                        self.marketplace_confirm_delete = Some(idx);
                    }
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }

    pub(super) fn handle_marketplace_add(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input { key: Key::Esc, .. } => {
                self.add_marketplace_active = false;
                self.add_marketplace_input = InputState::new();
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let input_str = self.add_marketplace_input.value().trim().to_string();
                self.add_marketplace_active = false;
                self.add_marketplace_input = InputState::new();
                if !input_str.is_empty() {
                    if let Err(e) = self.persist_marketplace_add(&input_str, ctx) {
                        ctx.session_mgr.current_mut().messages.push_system_note(
                            ctx.services.lc.tr_args(
                                "app-plugin-add-failed",
                                &[("error".into(), e.to_string().into())],
                            ),
                        );
                    }
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                self.add_marketplace_input.backspace();
                EventResult::Consumed
            }
            Input {
                key: Key::Char(ch), ..
            } => {
                self.add_marketplace_input.insert(ch);
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
}
