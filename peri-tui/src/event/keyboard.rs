use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use tui_textarea::{Input, Key};

use crate::app::panel_manager::{EventResult, PanelKind};
use crate::app::{App, MessageViewModel, PendingAttachment};
use peri_agent::messages::BaseMessage;

use super::Action;

// ---------------------------------------------------------------------------
// macOS key-binding compatibility layer
// ---------------------------------------------------------------------------
// On macOS, the Option (Alt) key acts as a character compose modifier.
// Terminals emit a composed Unicode character *without* any modifier flags.
// We maintain a central mapping table so each shortcut only needs to be
// defined once, keeping the macOS workaround auditable.
// ---------------------------------------------------------------------------

/// A cross-platform key-binding definition that accounts for macOS Option-key
/// character composition.
struct KeyBinding {
    /// Human-readable label (for status bar / docs).
    label: &'static str,
    /// Character produced on macOS when Option (+ optional Shift) is held.
    macos_char: Option<char>,
    /// Required modifiers on non-macOS terminals (Linux/Windows).
    modifiers: KeyModifiers,
    /// The primary key code (ignoring macOS compose).
    key: KeyCode,
}

impl KeyBinding {
    /// Returns `true` if `key_event` matches this binding on *any* platform.
    fn matches(&self, key_event: &ratatui::crossterm::event::KeyEvent) -> bool {
        // macOS path: terminal emits a composed char with no modifiers.
        if let Some(ch) = self.macos_char {
            if matches!(key_event.code, KeyCode::Char(c) if c == ch) {
                return true;
            }
        }
        // Standard path: check modifiers + key code.
        let mods_ok = key_event.modifiers.contains(self.resolved_modifiers());
        let key_ok = match (&self.key, &key_event.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => a.eq_ignore_ascii_case(b),
            (a, b) => a == b,
        };
        mods_ok && key_ok
    }

    /// Resolve the actual modifiers needed. bitflags `|` is not const,
    /// so multi-flag bindings store `KeyModifiers::empty()` and reconstruct here.
    fn resolved_modifiers(&self) -> KeyModifiers {
        match self.label {
            "Alt+Shift+M" => KeyModifiers::ALT | KeyModifiers::SHIFT,
            _ => self.modifiers,
        }
    }
}

/// Central shortcut registry.  Add new shortcuts here — the `matches()` call
/// in each handler block is the only site that needs updating.
static SHORTCUT_CYCLE_MODE: KeyBinding = KeyBinding {
    label: "Alt+M",
    macos_char: Some('µ'),
    modifiers: KeyModifiers::ALT,
    key: KeyCode::Char('m'),
};

static SHORTCUT_CYCLE_PROVIDER: KeyBinding = KeyBinding {
    label: "Alt+Shift+M",
    macos_char: Some('Â'),
    modifiers: KeyModifiers::empty(), // resolved_modifiers() returns ALT|SHIFT
    key: KeyCode::Char('m'),
};

/// Handles a single key event, dispatching to panels, prompts, textarea, or
/// application-level shortcuts. Returns an `Action` when a redraw or quit is
/// needed.
pub fn handle_key_event(
    app: &mut App,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Result<Option<Action>> {
    // Only process Press events; ignore Release (prevents double-fires)
    if key_event.kind == KeyEventKind::Release {
        return Ok(Some(Action::Redraw));
    }

    // Shift+Tab is reported as BackTab in crossterm;
    // ratatui-textarea's Key enum does not handle BackTab (maps to Null),
    // so we intercept it here and handle permission-mode cycling directly.
    if matches!(key_event.code, KeyCode::BackTab) {
        let _new_mode = app.services.permission_mode.cycle();
        app.global_ui.mode_highlight_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
        return Ok(Some(Action::Redraw));
    }

    // Alt+M cycles model aliases (opus → sonnet → haiku → opus)
    if SHORTCUT_CYCLE_MODE.matches(&key_event) {
        if let Some(cfg) = app.services.peri_config.as_mut() {
            let aliases = ["opus", "sonnet", "haiku"];
            let current = cfg.config.active_alias.as_str();
            let idx = aliases.iter().position(|&a| a == current).unwrap_or(0);
            let next = aliases[(idx + 1) % aliases.len()];
            cfg.config.active_alias = next.to_string();
            if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref()) {
                app.session_mgr.sessions[app.session_mgr.active]
                    .messages
                    .view_messages
                    .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
            }
            if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                app.services.provider_name = p.display_name().to_string();
                app.services.model_name = p.model_name().to_string();
            }
            app.services.sync_peri_config_to_acp();
            app.global_ui.model_highlight_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
        }
        return Ok(Some(Action::Redraw));
    }

    // Alt+Shift+M cycles providers
    if SHORTCUT_CYCLE_PROVIDER.matches(&key_event) {
        if let Some(cfg) = app.services.peri_config.as_mut() {
            let providers = &cfg.config.providers;
            if providers.len() > 1 {
                let current_id = cfg.config.active_provider_id.as_str();
                let idx = providers
                    .iter()
                    .position(|p| p.id == current_id)
                    .unwrap_or(0);
                let next_idx = (idx + 1) % providers.len();
                let next_provider = &providers[next_idx];
                cfg.config.active_provider_id = next_provider.id.clone();
                // 保持当前 alias，但需要确认新 provider 支持该 alias 的模型
                if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                    app.services.provider_name = p.display_name().to_string();
                    app.services.model_name = p.model_name().to_string();
                }
                if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref())
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .view_messages
                        .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                }
                app.services.sync_peri_config_to_acp();
                app.global_ui.provider_highlight_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
            }
        }
        return Ok(Some(Action::Redraw));
    }

    let input = Input::from(key_event);

    // Setup wizard: intercept all key events first
    if app.global_ui.setup_wizard.is_some() {
        // Ctrl+C: exit flow (matching normal-mode behaviour)
        if matches!(
            input,
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            }
        ) {
            if let Some(since) = app.global_ui.quit_pending_since {
                if since.elapsed() < std::time::Duration::from_secs(2) {
                    return Ok(Some(Action::Quit));
                } else {
                    app.global_ui.quit_pending_since = Some(std::time::Instant::now());
                }
            } else {
                app.global_ui.quit_pending_since = Some(std::time::Instant::now());
            }
            return Ok(Some(Action::Redraw));
        }
        let input_clone = input.clone();
        if let Some(ref mut wizard) = app.global_ui.setup_wizard {
            if let Some(action) =
                crate::app::setup_wizard::handle_setup_wizard_key(wizard, input_clone)
            {
                match action {
                    crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                        let wizard = app
                            .global_ui
                            .setup_wizard
                            .take()
                            .expect("setup_wizard must be Some (checked above)");
                        match crate::app::setup_wizard::save_setup(&wizard) {
                            Ok(cfg) => app.refresh_after_setup(cfg),
                            Err(e) => {
                                let msg = MessageViewModel::from_base_message(
                                    &BaseMessage::system(format!("配置保存失败: {}", e)),
                                    &[],
                                );
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .messages
                                    .view_messages
                                    .push(msg);
                                app.render_rebuild();
                            }
                        }
                    }
                    crate::app::setup_wizard::SetupWizardAction::Skip => {
                        let from_command = app
                            .global_ui
                            .setup_wizard
                            .as_ref()
                            .map(|w| w.from_command)
                            .unwrap_or(false);
                        app.global_ui.setup_wizard = None;
                        if !from_command {
                            return Ok(Some(Action::Quit));
                        }
                    }
                    crate::app::setup_wizard::SetupWizardAction::SetLanguage(lang) => {
                        let _ = app.services.lc.switch(&lang);
                    }
                    crate::app::setup_wizard::SetupWizardAction::Redraw => {}
                }
            }
        }
        return Ok(Some(Action::Redraw));
    }

    // ─── PanelManager dispatch ─────────────────────────────────────────────
    {
        // Session panels: Model, Agent, Hooks, Login, Config, ThreadBrowser
        let session_kind = app.session_mgr.sessions[app.session_mgr.active]
            .session_panels
            .active_kind();
        if matches!(
            session_kind,
            Some(PanelKind::Model)
                | Some(PanelKind::Agent)
                | Some(PanelKind::Hooks)
                | Some(PanelKind::Login)
                | Some(PanelKind::Config)
                | Some(PanelKind::ThreadBrowser)
        ) {
            crate::with_session_panels!(app, |sp, ctx| {
                let result = sp.dispatch_key(input, &mut ctx);
                let active_idx = app.session_mgr.active;
                match result {
                    EventResult::ClosePanel => {
                        sp.close();
                        app.session_mgr.sessions[active_idx]
                            .ui
                            .panel_selection
                            .clear();
                        app.session_mgr.sessions[active_idx].ui.panel_area = None;
                    }
                    EventResult::OpenThread(thread_id) => {
                        sp.close();
                        app.session_mgr.sessions[active_idx]
                            .ui
                            .panel_selection
                            .clear();
                        app.session_mgr.sessions[active_idx].ui.panel_area = None;
                        // with_session_panels! macro puts sp back at closure end,
                        // but OpenThread needs to put back first then call open_thread_with_feedback
                        app.session_mgr.sessions[active_idx].session_panels = sp;
                        // Early return prevents macro from putting back again
                        app.open_thread_with_feedback(thread_id);
                        return Ok(Some(Action::Redraw));
                    }
                    _ => {}
                }
                result
            });
            return Ok(Some(Action::Redraw));
        }

        // Global panels: Status, Memory, Mcp, Cron, Plugin
        let global_kind = app.global_panels.active_kind();
        if matches!(
            global_kind,
            Some(PanelKind::Status)
                | Some(PanelKind::Memory)
                | Some(PanelKind::Mcp)
                | Some(PanelKind::Cron)
                | Some(PanelKind::Plugin)
        ) {
            let active_idx = app.session_mgr.active;
            crate::with_global_panels!(app, |pm, ctx| {
                let result = pm.dispatch_key(input, &mut ctx);
                match result {
                    EventResult::ClosePanel => {
                        pm.close();
                        app.session_mgr.sessions[active_idx]
                            .ui
                            .panel_selection
                            .clear();
                        app.session_mgr.sessions[active_idx].ui.panel_area = None;
                    }
                    EventResult::OpenPanel(PanelKind::Memory) => {
                        app.global_panels = pm;
                        if let Err(e) = app.memory_panel_open_editor() {
                            tracing::error!("Failed to open editor: {}", e);
                        }
                        return Ok(Some(Action::Redraw));
                    }
                    _ => {}
                }
                result
            });
            return Ok(Some(Action::Redraw));
        }
    }

    // OAuth prompt takes priority
    if app.global_ui.oauth_prompt.is_some() {
        super::handle_oauth_prompt(app, input);
        return Ok(Some(Action::Redraw));
    }

    // AskUser batch popup
    if matches!(
        &app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .interaction_prompt,
        Some(crate::app::InteractionPrompt::Questions(_))
    ) {
        match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => return Ok(Some(Action::Quit)),
            // Tab / Shift+Tab cycle questions
            Input {
                key: Key::Tab,
                shift: false,
                ..
            } => app.ask_user_next_tab(),
            Input {
                key: Key::Tab,
                shift: true,
                ..
            } => app.ask_user_prev_tab(),
            // Enter submits all answers
            Input {
                key: Key::Enter, ..
            } => app.ask_user_confirm(),
            // Up/Down move option cursor within current question
            Input { key: Key::Up, .. } => app.ask_user_move(-1),
            Input { key: Key::Down, .. } => app.ask_user_move(1),
            // Space toggles selection
            Input {
                key: Key::Char(' '),
                ..
            } => app.ask_user_toggle(),
            // Text input (custom input mode) — use shared edit function
            _ => {
                app.ask_user_edit_key(input);
            }
        }
        return Ok(Some(Action::Redraw));
    }

    // HITL batch popup active — handle popup keys first
    if matches!(
        &app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .interaction_prompt,
        Some(crate::app::InteractionPrompt::Approval(_))
    ) {
        match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => return Ok(Some(Action::Quit)),

            // Up/Down move cursor
            Input { key: Key::Up, .. } => app.hitl_move(-1),
            Input { key: Key::Down, .. } => app.hitl_move(1),

            // Space: toggle current item
            Input {
                key: Key::Char(' '),
                ..
            } => app.hitl_toggle(),

            // Enter: confirm based on current selections
            Input {
                key: Key::Enter, ..
            } => app.hitl_confirm(),

            // Esc: reject all
            Input { key: Key::Esc, .. } => app.hitl_reject_all(),

            _ => {}
        }
        return Ok(Some(Action::Redraw));
    }

    match input {
        // Ctrl+C: interrupt agent / double-tap to quit
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                // Agent running: interrupt first, clear quit-pending state
                app.interrupt();
                app.global_ui.quit_pending_since = None;
            } else if let Some(since) = app.global_ui.quit_pending_since {
                // Not loading, second Ctrl+C within 2s → quit
                if since.elapsed() < std::time::Duration::from_secs(2) {
                    return Ok(Some(Action::Quit));
                } else {
                    // Timeout expired, restart timer
                    app.global_ui.quit_pending_since = Some(std::time::Instant::now());
                }
            } else {
                // First Ctrl+C, enter quit-pending state
                app.global_ui.quit_pending_since = Some(std::time::Instant::now());
            }
        }

        // ESC: no longer quits main window; only clears buffer while loading
        Input { key: Key::Esc, .. }
            if app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
        {
            if !app.session_mgr.sessions[app.session_mgr.active]
                .messages
                .pending_messages
                .is_empty()
            {
                app.session_mgr.sessions[app.session_mgr.active]
                    .messages
                    .pending_messages
                    .clear();
            }
        }

        // Up: hint navigation > history restore (only first row) > textarea cursor
        Input { key: Key::Up, .. }
            if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
        {
            let hint_count = app.hint_candidates_count();
            if hint_count > 0 {
                let cur = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor
                    .unwrap_or(0);
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor = if cur == 0 {
                    Some(hint_count - 1)
                } else {
                    Some(cur - 1)
                };
            } else {
                let (row, _col) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .cursor();
                if row == 0 {
                    app.history_up();
                } else {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .input(Input {
                            key: Key::Up,
                            ctrl: false,
                            alt: false,
                            shift: false,
                        });
                }
            }
        }

        // Down: hint navigation > history restore (only last row) > textarea cursor
        Input { key: Key::Down, .. }
            if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
        {
            let hint_count = app.hint_candidates_count();
            if hint_count > 0 {
                let cur = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor
                    .unwrap_or(hint_count - 1);
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor = if cur + 1 >= hint_count {
                    Some(0)
                } else {
                    Some(cur + 1)
                };
            } else if app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .history_index
                .is_some()
            {
                app.history_down();
            } else {
                let (row, _col) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .cursor();
                let last_row = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .lines()
                    .len()
                    .saturating_sub(1);
                if row >= last_row {
                    app.history_down();
                } else {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .input(Input {
                            key: Key::Down,
                            ctrl: false,
                            alt: false,
                            shift: false,
                        });
                }
            }
        }

        // Ctrl+V: try pasting clipboard image first, fallback to text paste
        Input {
            key: Key::Char('v'),
            ctrl: true,
            ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(img) = clipboard.get_image() {
                    let (w, h) = (img.width as u32, img.height as u32);
                    if let Ok((b64, sz)) = super::mouse::rgba_to_png_base64(w, h, &img.bytes) {
                        let n = app.session_mgr.sessions[app.session_mgr.active]
                            .metadata
                            .pending_attachments
                            .len()
                            + 1;
                        app.add_pending_attachment(PendingAttachment {
                            label: format!("clipboard_{}.png", n),
                            media_type: "image/png".to_string(),
                            base64_data: b64,
                            size_bytes: sz,
                        });
                    }
                } else if let Ok(text) = clipboard.get_text() {
                    let text = text.replace('\r', "\n");
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .insert_str(&text);
                }
            }
        }

        // Tab: hint overlay candidate navigation and completion
        Input {
            key: Key::Tab,
            shift: false,
            ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => {
            let count = app.hint_candidates_count();
            if count > 0 {
                match app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor
                {
                    Some(cur) if cur + 1 < count => {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = Some(cur + 1);
                    }
                    Some(_) => {
                        // Already at last, cycle to first
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = Some(0);
                    }
                    None => {
                        // First Tab press, select first
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = Some(0);
                    }
                }
            }
        }

        // Enter with hints available: confirm selection (defaults to first if none selected)
        Input {
            key: Key::Enter, ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
            && app.hint_candidates_count() > 0 =>
        {
            if app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .hint_cursor
                .is_none()
            {
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor = Some(0);
            }
            app.hint_complete();
        }

        // Alt+Enter: insert newline
        Input {
            key: Key::Enter,
            alt: true,
            ..
        } => {
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .input(Input {
                    key: Key::Enter,
                    ctrl: false,
                    alt: false,
                    shift: false,
                });
        }

        // Enter: submit (non-loading) or buffer (loading)
        Input {
            key: Key::Enter, ..
        } => {
            let text = app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .lines()
                .join("\n");
            let text = text.trim().to_string();
            if !text.is_empty() {
                if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                    // Loading state: buffer message
                    app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .pending_messages
                        .push(text);
                    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                        crate::app::build_textarea(false);
                    app.update_textarea_hint();
                } else if text.starts_with('/') {
                    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                        crate::app::build_textarea(false);
                    // SAFETY: command_registry is nested inside App; dispatch needs &mut App
                    // NOTE: session index must be saved before take because dispatch
                    // (e.g. /split) may change app.session_mgr.active
                    let session_idx = app.session_mgr.active;
                    let registry = std::mem::take(
                        &mut app.session_mgr.sessions[session_idx]
                            .commands
                            .command_registry,
                    );
                    let known = registry.dispatch(app, &text);
                    app.session_mgr.sessions[session_idx]
                        .commands
                        .command_registry = registry;
                    if known {
                        // Command matched, done
                    } else {
                        // Command not matched, try Skill matching
                        let skill_name: String = text
                            .trim_start_matches('/')
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                            .collect();
                        if let Some(_skill) = app.session_mgr.sessions[app.session_mgr.active]
                            .commands
                            .skills
                            .iter()
                            .find(|s| s.name == skill_name)
                        {
                            // Skill matched: submit full message to agent
                            return Ok(Some(Action::Submit(text)));
                        } else {
                            // Distinguish "prefix ambiguity" from "completely unknown"
                            let prefix = text.trim_start_matches('/').to_string();
                            let cmd_matches = app.session_mgr.sessions[app.session_mgr.active]
                                .commands
                                .command_registry
                                .match_prefix(&prefix, &app.services.lc);
                            let error_msg = if cmd_matches.len() > 1 {
                                let names: Vec<&str> =
                                    cmd_matches.iter().map(|(n, _)| n.as_str()).collect();
                                format!(
                                    "命令 '{}' 匹配多个: {}  （请输入完整命令名）",
                                    text,
                                    names
                                        .iter()
                                        .map(|n| format!("/{}", n))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            } else {
                                format!("未知命令或 Skill: {}  （输入 /help 查看可用命令）", text)
                            };
                            app.session_mgr.sessions[app.session_mgr.active]
                                .messages
                                .view_messages
                                .push(MessageViewModel::system(error_msg));
                        }
                    }
                } else {
                    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                        crate::app::build_textarea(false);
                    return Ok(Some(Action::Submit(text)));
                }
            }
        }

        // PageUp/PageDown: VS Code terminal maps Option+Backspace to PageUp
        // Detect VS Code terminal environment; perform word-delete when textarea has content
        Input {
            key: Key::PageUp, ..
        } if std::env::var("TERM_PROGRAM").as_deref() == Ok("vscode") => {
            let session = &mut app.session_mgr.sessions[app.session_mgr.active];
            let has_content = session
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if has_content {
                session.ui.textarea.delete_word();
            } else {
                for _ in 0..10 {
                    app.scroll_up();
                }
            }
        }
        Input {
            key: Key::PageDown, ..
        } => {
            for _ in 0..10 {
                app.scroll_down();
            }
        }

        // Ctrl+U / Ctrl+D: half-page scroll (no physical PageUp/PageDown needed; MacBook friendly)
        Input {
            key: Key::Char('u'),
            ctrl: true,
            ..
        } => {
            for _ in 0..20 {
                app.scroll_up();
            }
        }
        Input {
            key: Key::Char('d'),
            ctrl: true,
            ..
        } => {
            for _ in 0..20 {
                app.scroll_down();
            }
        }

        // Del: remove last pending attachment (consumes Delete first when attachments exist)
        Input {
            key: Key::Delete, ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
            && !app.session_mgr.sessions[app.session_mgr.active]
                .metadata
                .pending_attachments
                .is_empty() =>
        {
            app.pop_pending_attachment();
        }

        // Ctrl+N/P: cycle session focus
        Input {
            key: Key::Char('n'),
            ctrl: true,
            ..
        } => {
            app.switch_next_session();
        }
        Input {
            key: Key::Char('p'),
            ctrl: true,
            ..
        } => {
            app.switch_prev_session();
        }

        // Ctrl+W: close current session
        input @ Input {
            key: Key::Char('w'),
            ctrl: true,
            ..
        } => {
            if app.close_session().is_some() {
                // Session closed, stop processing
            } else {
                // Only one session, fallback to textarea
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .input(input);
            }
        }

        // Intercept plain Enter to avoid textarea default newline; allow input during loading
        input if input.key != Key::Enter => {
            // Exit history browsing
            if app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .history_index
                .is_some()
            {
                app.exit_history();
            }
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .input(input);
            // When input changes: reset cursor (don't pre-select; wait for user to press Tab/Up/Down)
            if !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor = None;
            }
        }

        _ => {
            // Any other key cancels quit-pending state
            app.global_ui.quit_pending_since = None;
        }
    }

    Ok(Some(Action::Redraw))
}
