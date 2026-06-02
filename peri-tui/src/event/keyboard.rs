use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use tui_textarea::Input;

use crate::app::App;

use super::Action;

// ── Submodule declarations ─────────────────────────────────────────────────
mod bar_focus;
mod normal_keys;
mod panels;
mod popups;
mod setup_wizard;
mod shortcuts;

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
pub(super) struct KeyBinding {
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
    pub(super) fn matches(&self, key_event: &ratatui::crossterm::event::KeyEvent) -> bool {
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
            "Ctrl+Shift+T" => KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            _ => self.modifiers,
        }
    }
}

/// Central shortcut registry.  Add new shortcuts here — the `matches()` call
/// in each handler block is the only site that needs updating.
pub(super) static SHORTCUT_CYCLE_MODE: KeyBinding = KeyBinding {
    label: "Alt+M",
    macos_char: Some('µ'),
    modifiers: KeyModifiers::ALT,
    key: KeyCode::Char('m'),
};

pub(super) static SHORTCUT_CYCLE_PROVIDER: KeyBinding = KeyBinding {
    label: "Alt+Shift+M",
    macos_char: Some('Â'),
    modifiers: KeyModifiers::empty(), // resolved_modifiers() returns ALT|SHIFT
    key: KeyCode::Char('m'),
};

// Ctrl+T / Ctrl+Shift+T: cross-platform model/provider cycling.
// Ctrl combos have no macOS composition issue, so macos_char is None.
pub(super) static SHORTCUT_CTRL_CYCLE_MODE: KeyBinding = KeyBinding {
    label: "Ctrl+T",
    macos_char: None,
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Char('t'),
};

pub(super) static SHORTCUT_CTRL_CYCLE_PROVIDER: KeyBinding = KeyBinding {
    label: "Ctrl+Shift+T",
    macos_char: None,
    modifiers: KeyModifiers::empty(), // resolved_modifiers() returns CONTROL|SHIFT
    key: KeyCode::Char('t'),
};

pub(super) static SHORTCUT_BG_BAR: KeyBinding = KeyBinding {
    label: "Ctrl+B",
    macos_char: None,
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Char('b'),
};

/// Returns the platform-appropriate label for the model-cycling shortcut.
pub fn cycle_model_label() -> &'static str {
    "Ctrl+T"
}

/// Returns the platform-appropriate label for the provider-cycling shortcut.
pub fn cycle_provider_label() -> &'static str {
    "Ctrl+Shift+T"
}

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

    // Stage 1-2: Bar focus / focused-only mode
    if let Some(action) = bar_focus::handle_bar_focus(app, &key_event) {
        return Ok(Some(action));
    }
    if let Some(action) = bar_focus::handle_focused_only(app, &key_event) {
        return Ok(Some(action));
    }

    // Stage 3-6: Shortcuts (BackTab, Ctrl+B, Ctrl+T, Ctrl+Shift+T)
    if let Some(action) = shortcuts::handle_shortcuts(app, &key_event) {
        return Ok(Some(action));
    }

    let input = Input::from(key_event);

    // Stage 7: Setup wizard
    if let Some(action) = setup_wizard::handle_setup_wizard(app, &input) {
        return Ok(Some(action));
    }

    // Stage 8-9: Panels
    if let Some(action) = panels::handle_panels(app, &input) {
        return Ok(Some(action));
    }

    // Stage 10-12: Popups (OAuth > AskUser > HITL)
    if let Some(action) = popups::handle_popups(app, &input) {
        return Ok(Some(action));
    }

    // Stage 13: Normal key handling (main match block)
    normal_keys::handle_normal_keys(app, input)
}

/// 检测 textarea 中 @ 提及模式，更新状态并触发异步搜索
/// 缓存命中时立即更新，否则 spawn 后台任务避免阻塞 UI 线程
pub(super) fn update_at_mention_detection(app: &mut App) {
    let textarea = &app.session_mgr.current_mut().ui.textarea;
    let text = textarea.lines().join("\n");
    let (row, col) = textarea.cursor();
    // 将 (row, col) 转为字节偏移
    let mut pos = 0usize;
    for (i, line) in textarea.lines().iter().enumerate() {
        if i == row {
            pos += line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
            break;
        }
        pos += line.len() + 1; // +1 for \n
    }

    let at = &mut app.session_mgr.current_mut().ui.at_mention;

    at.ensure_cwd(app.services.cwd.clone());

    if let Some((query, start)) = crate::app::AtMentionState::detect(&text, pos) {
        if at.active && at.query == query {
            return; // 未变化
        }
        at.activate(query.clone(), start);

        // 尝试从缓存获取结果（零 IO，立即更新）
        if let Some(cached) = at.try_filter_from_cache(&query) {
            at.update_candidates(cached);
            return;
        }

        // 节流：距离上次搜索不到 200ms 时，保留旧结果不搜索
        if !at.should_search_now() && !at.candidates.is_empty() {
            return;
        }

        // 搜索线程处理，不阻塞 UI
        at.start_search(query);
    } else if at.active {
        at.close();
    }
}

/// 将选中的 @ 提及路径注入 textarea
pub(super) fn inject_at_mention_path(app: &mut App) {
    let at = &app.session_mgr.current_mut().ui.at_mention;
    let candidate = match at.selected_candidate() {
        Some(c) => c.clone(),
        None => return,
    };
    let query_start = at.query_start;
    let query_len = at.query.len();

    let textarea = &app.session_mgr.current_mut().ui.textarea;
    let full_text: String = textarea.lines().join("\n");

    let needs_quotes = candidate.path.contains(' ');
    let replacement = if needs_quotes {
        format!("@\"{}\"", candidate.path)
    } else {
        format!("@{}", candidate.path)
    };

    // 替换从 query_start 到 query_start + 1(@) + query_len
    let mut new_text = String::with_capacity(full_text.len() + replacement.len());
    new_text.push_str(&full_text[..query_start]);
    new_text.push_str(&replacement);
    let after_end = query_start + 1 + query_len;
    if after_end < full_text.len() {
        new_text.push_str(&full_text[after_end..]);
    }

    let is_dir = candidate.is_dir;

    let mut new_ta = crate::app::build_textarea(false);
    new_ta.insert_str(&new_text);
    app.session_mgr.current_mut().ui.textarea = new_ta;

    if is_dir {
        app.session_mgr.current_mut().ui.textarea.insert_str("/");
        update_at_mention_detection(app);
    } else {
        app.session_mgr.current_mut().ui.textarea.insert_str(" ");
        app.session_mgr.current_mut().ui.at_mention.close();
    }
}
