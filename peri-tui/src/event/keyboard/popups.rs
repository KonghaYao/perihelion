use tui_textarea::{Input, Key};

use crate::app::App;

use super::super::Action;

/// 弹窗处理：OAuth > AskUser > Rewind > HITL（优先级链）
pub(super) fn handle_popups(app: &mut App, input: &Input) -> Option<Action> {
    // OAuth prompt takes priority
    if app.global_ui.oauth_prompt.is_some() {
        super::super::handle_oauth_prompt(app, input.clone());
        return Some(Action::Redraw);
    }

    // AskUser batch popup
    if matches!(
        &app.session_mgr.current_mut().agent.interaction_prompt,
        Some(crate::app::InteractionPrompt::Questions(_))
    ) {
        match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => return Some(Action::Quit),
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
            // Ctrl+U / Ctrl+D 页面滚动
            Input {
                key: Key::Char('u'),
                ctrl: true,
                ..
            } => app.ask_user_scroll(-10),
            Input {
                key: Key::Char('d'),
                ctrl: true,
                ..
            } => app.ask_user_scroll(10),
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
                app.ask_user_edit_key(input.clone());
            }
        }
        return Some(Action::Redraw);
    }

    // Rewind popup
    if matches!(
        &app.session_mgr.current_mut().agent.interaction_prompt,
        Some(crate::app::InteractionPrompt::Rewind(_))
    ) {
        match input {
            Input { key: Key::Up, .. } => {
                app.rewind_cursor_up();
                return Some(Action::Redraw);
            }
            Input { key: Key::Down, .. } => {
                app.rewind_cursor_down();
                return Some(Action::Redraw);
            }
            Input { key: Key::Tab, .. } => {
                app.rewind_toggle_files();
                return Some(Action::Redraw);
            }
            Input {
                key: Key::Enter, ..
            } => {
                app.rewind_confirm();
                return Some(Action::Redraw);
            }
            Input { key: Key::Esc, .. } => {
                app.cancel_rewind();
                return Some(Action::Redraw);
            }
            _ => return Some(Action::Redraw),
        }
    }

    // HITL batch popup active — handle popup keys first
    if matches!(
        &app.session_mgr.current_mut().agent.interaction_prompt,
        Some(crate::app::InteractionPrompt::Approval(_))
    ) {
        match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => return Some(Action::Quit),

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
        return Some(Action::Redraw);
    }

    None
}
