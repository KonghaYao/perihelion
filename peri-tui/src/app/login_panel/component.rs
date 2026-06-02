use std::any::Any;

use ratatui::{layout::Rect, Frame};
use tui_textarea::Input;

use crate::i18n::LcRegistry;

use super::{
    super::{
        panel_component::PanelComponent,
        panel_manager::{EventResult, PanelContext, PanelKind},
    },
    App, LoginEditField, LoginPanel, LoginPanelMode,
};

impl PanelComponent for LoginPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Login
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match &self.mode {
            LoginPanelMode::Browse => match input {
                Input { key: Key::Esc, .. } => EventResult::ClosePanel,
                Input { key: Key::Up, .. } => {
                    self.move_cursor(-1);
                    EventResult::Consumed
                }
                Input { key: Key::Down, .. } => {
                    self.move_cursor(1);
                    EventResult::Consumed
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    let selected_name = self
                        .providers
                        .get(self.cursor())
                        .map(|p| p.display_name().to_string())
                        .unwrap_or_default();
                    let Some(cfg) = ctx.services.peri_config.as_mut() else {
                        return EventResult::Consumed;
                    };
                    self.select_provider(cfg);
                    if !selected_name.is_empty() {
                        ctx.session_mgr.current_mut().messages.push_system_note(
                            ctx.services.lc.tr_args(
                                "app-provider-activated",
                                &[("name".into(), selected_name.into())],
                            ),
                        );
                    }
                    if let Err(e) =
                        App::save_config(cfg, ctx.services.config_path_override.as_deref())
                    {
                        ctx.session_mgr.current_mut().messages.push_system_note(
                            ctx.services.lc.tr_args(
                                "app-config-save-failed",
                                &[("error".into(), e.to_string().into())],
                            ),
                        );
                    }
                    if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                        ctx.services.provider_name = p.display_name().to_string();
                        ctx.services.model_name = p.model_name().to_string();
                    }
                    ctx.sync_acp_config();
                    EventResult::ClosePanel
                }
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } => {
                    self.enter_edit();
                    EventResult::Consumed
                }
                Input {
                    key: Key::Char('n'),
                    ctrl: true,
                    ..
                } => {
                    self.enter_new();
                    EventResult::Consumed
                }
                Input {
                    key: Key::Char('d'),
                    ctrl: true,
                    ..
                } => {
                    self.request_delete();
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            },
            LoginPanelMode::Edit | LoginPanelMode::New => {
                let is_type_field = self.edit_field == LoginEditField::Type;
                match input {
                    Input { key: Key::Esc, .. } => {
                        self.mode = LoginPanelMode::Browse;
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Char('v'),
                        ctrl: true,
                        ..
                    } => {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                self.paste_text(&text);
                            }
                        }
                        EventResult::Consumed
                    }
                    Input { key: Key::Up, .. } => {
                        self.field_prev();
                        EventResult::Consumed
                    }
                    Input { key: Key::Down, .. } => {
                        self.field_next();
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Tab,
                        shift: false,
                        ..
                    } => {
                        self.field_next();
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Tab,
                        shift: true,
                        ..
                    } => {
                        self.field_prev();
                        EventResult::Consumed
                    }
                    Input { key: Key::Left, .. }
                    | Input {
                        key: Key::Right, ..
                    } if is_type_field => {
                        self.cycle_type();
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Char(' '),
                        ..
                    } => {
                        if is_type_field {
                            self.cycle_type();
                        } else if let Some((buf, cursor)) = self.active_field() {
                            crate::app::handle_edit_key(
                                buf,
                                cursor,
                                Input {
                                    key: Key::Char(' '),
                                    ctrl: false,
                                    alt: false,
                                    shift: false,
                                },
                            );
                        }
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Enter, ..
                    } => {
                        let edit_name = self.buf_name.clone();
                        let is_new = self.mode == LoginPanelMode::New;
                        let Some(cfg) = ctx.services.peri_config.as_mut() else {
                            return EventResult::Consumed;
                        };
                        if !self.apply_edit(cfg) {
                            ctx.session_mgr
                                .current_mut()
                                .messages
                                .push_system_note(ctx.services.lc.tr("app-provider-name-empty"));
                            return EventResult::Consumed;
                        }
                        let display = if edit_name.is_empty() {
                            "Provider".to_string()
                        } else {
                            edit_name
                        };
                        self.select_provider(cfg);
                        let key = if is_new {
                            "app-provider-created"
                        } else {
                            "app-provider-saved"
                        };
                        ctx.session_mgr.current_mut().messages.push_system_note(
                            ctx.services
                                .lc
                                .tr_args(key, &[("name".into(), display.into())]),
                        );
                        if let Err(e) =
                            App::save_config(cfg, ctx.services.config_path_override.as_deref())
                        {
                            ctx.session_mgr.current_mut().messages.push_system_note(
                                ctx.services.lc.tr_args(
                                    "app-config-save-failed",
                                    &[("error".into(), e.to_string().into())],
                                ),
                            );
                        }
                        if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                            ctx.services.provider_name = p.display_name().to_string();
                            ctx.services.model_name = p.model_name().to_string();
                        }
                        ctx.sync_acp_config();
                        EventResult::ClosePanel
                    }
                    _ => {
                        if !is_type_field {
                            if let Some((buf, cursor)) = self.active_field() {
                                crate::app::handle_edit_key(buf, cursor, input);
                            }
                        }
                        EventResult::Consumed
                    }
                }
            }
            LoginPanelMode::ConfirmDelete => match input {
                Input {
                    key: Key::Enter, ..
                } => {
                    let Some(cfg) = ctx.services.peri_config.as_mut() else {
                        return EventResult::Consumed;
                    };
                    let deleted_name = self
                        .providers
                        .get(self.cursor())
                        .map(|p| p.display_name().to_string())
                        .unwrap_or_default();
                    self.confirm_delete(cfg);
                    if !deleted_name.is_empty() {
                        ctx.session_mgr.current_mut().messages.push_system_note(
                            ctx.services.lc.tr_args(
                                "app-provider-deleted",
                                &[("name".into(), deleted_name.into())],
                            ),
                        );
                    }
                    if let Err(e) =
                        App::save_config(cfg, ctx.services.config_path_override.as_deref())
                    {
                        ctx.session_mgr.current_mut().messages.push_system_note(
                            ctx.services.lc.tr_args(
                                "app-config-save-failed",
                                &[("error".into(), e.to_string().into())],
                            ),
                        );
                    }
                    if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                        ctx.services.provider_name = p.display_name().to_string();
                        ctx.services.model_name = p.model_name().to_string();
                    }
                    ctx.sync_acp_config();
                    EventResult::Consumed
                }
                Input { key: Key::Esc, .. } => {
                    self.cancel_delete();
                    EventResult::Consumed
                }
                _ => {
                    self.cancel_delete();
                    EventResult::Consumed
                }
            },
        }
    }

    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.paste_text(text);
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        if matches!(self.mode, LoginPanelMode::Browse) {
            self.browse_list.handle_scroll(lines, 10);
            EventResult::Consumed
        } else {
            EventResult::NotConsumed
        }
    }

    fn set_scroll_offset(&mut self, offset: u16) {
        if matches!(self.mode, LoginPanelMode::Browse) {
            self.browse_list.set_scroll_offset(offset);
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: ratatui::crossterm::event::MouseEvent,
        area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if matches!(self.mode, LoginPanelMode::Browse) =>
            {
                if self
                    .browse_list
                    .handle_mouse_click(mouse.row, mouse.column, area, 1)
                {
                    return EventResult::Consumed;
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        match self.mode {
            LoginPanelMode::Browse => 14,
            LoginPanelMode::Edit | LoginPanelMode::New => 20,
            LoginPanelMode::ConfirmDelete => 14,
        }
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::login::render_login_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, lc: &LcRegistry) -> Vec<(String, String)> {
        match self.mode {
            LoginPanelMode::Browse => vec![
                ("\u{2191}\u{2193}".to_string(), lc.tr("hint-login-browse")),
                ("Enter".to_string(), lc.tr("hint-login-activate")),
                ("Tab".to_string(), lc.tr("hint-login-edit")),
                ("Ctrl+N".to_string(), lc.tr("hint-login-new")),
                ("Ctrl+D".to_string(), lc.tr("hint-login-delete")),
                ("Esc".to_string(), lc.tr("hint-login-close")),
            ],
            LoginPanelMode::Edit | LoginPanelMode::New => vec![
                ("\u{2191}\u{2193}".to_string(), lc.tr("hint-login-field")),
                ("Enter".to_string(), lc.tr("hint-login-save")),
                ("Ctrl+V".to_string(), lc.tr("hint-login-paste")),
                ("Space".to_string(), lc.tr("hint-login-toggle")),
                ("Esc".to_string(), lc.tr("hint-login-back")),
            ],
            LoginPanelMode::ConfirmDelete => vec![
                ("Enter".to_string(), lc.tr("login-confirm-delete")),
                ("Esc".to_string(), lc.tr("key-cancel")),
            ],
        }
    }
}
