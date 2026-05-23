// ── Event module ──────────────────────────────────────────────────────────────
// Split from the original monolithic event.rs (1447 lines) into:
//   mouse.rs   — mouse coordinate helpers + clipboard functions
//   keyboard.rs — key event handler
//   macros.rs  — panel dispatch macros (with_global_panels!, with_session_panels!)
//   mod.rs     — Action, event loop, dispatcher, OAuth handling

pub mod keyboard;
mod macros;
pub mod mouse;

use crate::with_global_panels;
use crate::with_session_panels;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, MouseButton, MouseEventKind};
use std::time::Duration;
use tui_textarea::{Input, Key};

use crate::app::panel_manager::{EventResult, PanelKind};
use crate::app::App;

// ── Action ──────────────────────────────────────────────────────────────────

pub enum Action {
    Quit,
    Submit(String),
    Redraw,
}

// ── Event loop ──────────────────────────────────────────────────────────────

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    // Quit-pending state auto-expires after 1s; trigger redraw so the shortcut bar
    // returns to normal
    if let Some(since) = app.global_ui.quit_pending_since {
        if since.elapsed() >= std::time::Duration::from_secs(1) {
            app.global_ui.quit_pending_since = None;
            return Ok(Some(Action::Redraw));
        }
    }

    // Mouse-availability probe: on first user input after startup, determine
    // whether the terminal supports mouse events.
    if app.global_ui.mouse_available.is_none() {
        // Wait for the first event (up to 1 s); this is not counted as normal poll timeout
        if event::poll(Duration::from_secs(1))? {
            let ev = event::read()?;
            if matches!(ev, Event::Mouse(_)) {
                app.global_ui.mouse_available = Some(true);
            } else {
                // Received keyboard/resize etc. but not mouse → terminal likely
                // does not support mice (mouse-capable terminals almost always trigger
                // scroll/move within 1 s)
                app.global_ui.mouse_available = Some(false);
            }
            return handle_event(app, ev).await;
        } else {
            // No event within 1 s → no mouse
            app.global_ui.mouse_available = Some(false);
            return Ok(None);
        }
    }

    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    // Scroll/Drag event coalescing: drain queued mouse events to avoid
    // redundant redraws during rapid scrolling or scrollbar dragging.
    let ev = coalesce_mouse_events(ev);

    handle_event(app, ev).await
}

// ── Mouse event coalescing ───────────────────────────────────────────────

/// Coalesces rapid-fire mouse scroll/drag events from the crossterm queue.
///
/// When a Scroll or Drag(Left) mouse event is the initial event, drains any
/// additional queued events using a non-blocking poll, keeping only the last
/// coalesceable event. This trades scroll precision for CPU: N scroll events
/// within one poll cycle (~50ms) result in only ±3 lines moved instead of N×3.
/// Drag(Left) is unaffected since only the final position matters.
///
/// Non-coalesceable events (click, keypress, etc.) terminate the drain and
/// replace the pending scroll as the returned event (not dropped).
fn coalesce_mouse_events(ev: Event) -> Event {
    // Only activate coalescing for scroll and drag mouse events
    match &ev {
        Event::Mouse(m) => match m.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::Drag(MouseButton::Left) => {}
            _ => return ev,
        },
        _ => return ev,
    }

    let mut last_ev = ev;

    // Drain all queued scroll/drag events, keeping only the last one.
    // Non-scroll/drag events terminate the drain and become the result
    // so they are not lost.
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let next = match event::read() {
            Ok(e) => e,
            Err(_) => break,
        };
        match &next {
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::Drag(MouseButton::Left) => {
                    last_ev = next;
                }
                // Other mouse events (click, release, move): stop draining,
                // return this event instead so it's handled normally
                _ => {
                    last_ev = next;
                    break;
                }
            },
            // Non-mouse events: stop draining, return this event
            _ => {
                last_ev = next;
                break;
            }
        }
    }

    last_ev
}

// ── Event dispatcher ────────────────────────────────────────────────────────

/// Core event-handling logic (extracted from `next_event` to avoid duplicating
/// the probe and normal paths).
async fn handle_event(app: &mut App, ev: Event) -> Result<Option<Action>> {
    match ev {
        Event::FocusGained => {
            app.focused = true;
            return Ok(Some(Action::Redraw));
        }
        Event::FocusLost => {
            app.focused = false;
            return Ok(Some(Action::Redraw));
        }
        Event::Resize(_, _) => {
            // Width sync is now driven by render_messages (compares cache.width vs text_area.width)
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .text_selection
                .clear();
        }
        Event::Key(key_event) => {
            return keyboard::handle_key_event(app, key_event);
        }
        Event::Paste(text) => {
            // Paste text handling
            // Some terminals (e.g. VSCode) use \r instead of \n as line separator in bracketed paste
            let text = text.replace('\r', "\n");

            // Setup wizard open — paste into active field
            if let Some(wizard) = &mut app.global_ui.setup_wizard {
                wizard.paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // ─── PanelManager paste dispatch ────────────────────────────
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
                    with_session_panels!(app, |sp, ctx| sp.dispatch_paste(&text, &mut ctx));
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
                    with_global_panels!(app, |pm, ctx| pm.dispatch_paste(&text, &mut ctx));
                    return Ok(Some(Action::Redraw));
                }
            }

            // Fallback: paste into textarea
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .insert_str(&text);
        }
        Event::Mouse(mouse) => match mouse.kind {
            // ── AskUser 弹窗鼠标交互（优先于面板/消息区） ────────────────────────
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                {
                    let sessions = &app.session_mgr.sessions;
                    let active = app.session_mgr.active;
                    if let Some(crate::app::InteractionPrompt::Questions(_)) =
                        sessions[active].agent.interaction_prompt
                    {
                        if let Some(area) = sessions[active].ui.panel_area {
                            if mouse::mouse_in_rect(&mouse, area) {
                                let delta = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                                    -3
                                } else {
                                    3
                                };
                                app.ask_user_scroll(delta);
                                return Ok(Some(Action::Redraw));
                            }
                        }
                    }
                }
                // 正常滚动处理
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_area;
                        if let Some(area) = panel_area {
                            if mouse::mouse_in_rect(&mouse, area) {
                                // Session panel takes priority
                                let sp = &app.session_mgr.sessions[app.session_mgr.active]
                                    .session_panels;
                                if sp.is_any_open() {
                                    let result = with_session_panels!(app, |sp, ctx| {
                                        sp.dispatch_scroll(-3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                                // Global panel
                                if app.global_panels.is_any_open() {
                                    let result = with_global_panels!(app, |pm, ctx| {
                                        pm.dispatch_scroll(-3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                            }
                        }
                        app.scroll_up();
                    }
                    MouseEventKind::ScrollDown => {
                        let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_area;
                        if let Some(area) = panel_area {
                            if mouse::mouse_in_rect(&mouse, area) {
                                // Session panel takes priority
                                let sp = &app.session_mgr.sessions[app.session_mgr.active]
                                    .session_panels;
                                if sp.is_any_open() {
                                    let result = with_session_panels!(app, |sp, ctx| {
                                        sp.dispatch_scroll(3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                                // Global panel
                                if app.global_panels.is_any_open() {
                                    let result = with_global_panels!(app, |pm, ctx| {
                                        pm.dispatch_scroll(3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                            }
                        }
                        app.scroll_down();
                    }
                    _ => unreachable!(),
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // ── AskUser 弹窗滚动条点击（优先于面板滚动条） ──────────────────
                {
                    let sessions = &app.session_mgr.sessions;
                    let active = app.session_mgr.active;
                    if let Some(crate::app::InteractionPrompt::Questions(ref p)) =
                        sessions[active].agent.interaction_prompt
                    {
                        if let Some(metrics) = p.scrollbar_metrics {
                            if mouse.column >= metrics.bar_area.x
                                && mouse.column < metrics.bar_area.x + metrics.bar_area.width
                                && mouse.row >= metrics.bar_area.y
                                && mouse.row < metrics.bar_area.bottom()
                                && metrics.max_offset > 0
                            {
                                let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
                                if bar_inner_height > 0 {
                                    let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1))
                                        .min(bar_inner_height);
                                    let new_offset = ((rel_y as f64 / bar_inner_height as f64)
                                        * metrics.max_offset as f64)
                                        as u16;
                                    let new_offset = new_offset.min(metrics.max_offset);
                                    if let Some(crate::app::InteractionPrompt::Questions(p)) =
                                        app.session_mgr.sessions[app.session_mgr.active]
                                            .agent
                                            .interaction_prompt
                                            .as_mut()
                                    {
                                        p.scroll_offset = new_offset;
                                    }
                                }
                                return Ok(Some(Action::Redraw));
                            }
                        }
                    }
                }
                // Panel scrollbar: ▲/▼ buttons and bar click/drag
                // Must be checked BEFORE dispatch_mouse so scrollbar clicks
                // aren't consumed by panel content area handlers.
                {
                    let session = &mut app.session_mgr.sessions[app.session_mgr.active];
                    if let Some(ref metrics) = session.ui.panel_scrollbar_metrics {
                        // ▼ button click (scroll to bottom)
                        if let Some(btn) = metrics.down_btn_area {
                            if mouse.column >= btn.x
                                && mouse.column < btn.x + btn.width
                                && mouse.row >= btn.y
                                && mouse.row < btn.y + btn.height
                            {
                                session
                                    .session_panels
                                    .dispatch_set_scroll_offset(metrics.max_offset);
                                session.ui.panel_scroll_offset = metrics.max_offset;
                                return Ok(Some(Action::Redraw));
                            }
                        }
                        // ▲ button click (scroll to top)
                        if let Some(btn) = metrics.up_btn_area {
                            if mouse.column >= btn.x
                                && mouse.column < btn.x + btn.width
                                && mouse.row >= btn.y
                                && mouse.row < btn.y + btn.height
                            {
                                session.session_panels.dispatch_set_scroll_offset(0);
                                session.ui.panel_scroll_offset = 0;
                                return Ok(Some(Action::Redraw));
                            }
                        }
                        // Scrollbar bar click (proportional jump + start drag)
                        if mouse.column == metrics.bar_area.x
                            && mouse.row >= metrics.bar_area.y
                            && mouse.row < metrics.bar_area.bottom()
                            && metrics.max_offset > 0
                        {
                            let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
                            if bar_inner_height > 0 {
                                let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1))
                                    .min(bar_inner_height);
                                let new_offset = ((rel_y as f64 / bar_inner_height as f64)
                                    * metrics.max_offset as f64)
                                    as u16;
                                let new_offset = new_offset.min(metrics.max_offset);
                                session
                                    .session_panels
                                    .dispatch_set_scroll_offset(new_offset);
                                session.ui.panel_scroll_offset = new_offset;
                                session.ui.panel_scrollbar_dragging = true;
                            }
                            return Ok(Some(Action::Redraw));
                        }
                    }
                }
                // Panel area: dispatch mouse click to panel content
                let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area;
                let mut click_consumed = false;
                if let Some(area) = panel_area {
                    if mouse::mouse_in_rect(&mouse, area) {
                        // Session panels
                        {
                            let sp =
                                &app.session_mgr.sessions[app.session_mgr.active].session_panels;
                            if sp.is_any_open() {
                                let result = with_session_panels!(app, |sp, ctx| {
                                    sp.dispatch_mouse(mouse, area, &mut ctx)
                                });
                                if result == EventResult::Consumed {
                                    click_consumed = true;
                                }
                            }
                        }
                        // Global panels
                        if !click_consumed && app.global_panels.is_any_open() {
                            let result = with_global_panels!(app, |pm, ctx| {
                                pm.dispatch_mouse(mouse, area, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                click_consumed = true;
                            }
                        }
                    }
                }
                if click_consumed {
                    return Ok(Some(Action::Redraw));
                }
                // Multi-session: clicking a non-active session column switches focus
                if app.session_mgr.sessions.len() > 1 {
                    for (i, area) in app.session_mgr.session_areas.iter().enumerate() {
                        if mouse.column >= area.x
                            && mouse.column < area.x + area.width
                            && mouse.row >= area.y
                            && mouse.row < area.y + area.height
                            && i != app.session_mgr.active
                        {
                            app.session_mgr.active = i;
                            return Ok(Some(Action::Redraw));
                        }
                    }
                }
                // Panel area: start panel selection
                let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area;
                if let Some(area) = panel_area {
                    if mouse::mouse_in_rect(&mouse, area) {
                        let content_row = mouse.row - area.y
                            + app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_scroll_offset;
                        let col = mouse.column - area.x;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .start_drag(content_row, col);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .clear();
                        // Don't process other-area selections
                        return Ok(Some(Action::Redraw));
                    }
                }
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .messages_area
                {
                    let scroll_offset = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .scroll_offset;
                    let scroll_follow = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .scroll_follow;

                    // Scroll-to-bottom button: bottom-right click when user has scrolled away
                    let btn_col_start = area.right().saturating_sub(2);
                    let btn_row_start = area.bottom().saturating_sub(2);
                    if !scroll_follow
                        && mouse.column >= btn_col_start
                        && mouse.column < area.right()
                        && mouse.row >= btn_row_start
                        && mouse.row < area.bottom()
                    {
                        app.scroll_to_bottom();
                        return Ok(Some(Action::Redraw));
                    }

                    // Scroll-to-top button: top-right click when user has scrolled up
                    if scroll_offset > 0
                        && mouse.column >= btn_col_start
                        && mouse.column < area.right()
                        && mouse.row >= area.y
                        && mouse.row < area.y.saturating_add(2)
                    {
                        app.scroll_to_top();
                        return Ok(Some(Action::Redraw));
                    }

                    // Scrollbar drag: click on the rightmost scrollbar column
                    // (▲/▼ buttons already handled above, so this catches the track area)
                    let scrollbar_col = area.right().saturating_sub(1);
                    if mouse.column == scrollbar_col
                        && mouse.row >= area.y
                        && mouse.row < area.bottom()
                    {
                        let track_height = area.height.saturating_sub(1);
                        if track_height > 0 {
                            let rel_y = (mouse.row.saturating_sub(area.y)).min(track_height);
                            let max_scroll = app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scrollbar_max_offset;
                            let new_offset =
                                ((rel_y as f64 / track_height as f64) * max_scroll as f64) as u16;
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset = new_offset.min(max_scroll);
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_follow = false;
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scrollbar_dragging = true;
                        }
                        return Ok(Some(Action::Redraw));
                    }

                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let visual_row = mouse.row - area.y
                            + app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset;
                        let visual_col = mouse.column - area.x;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .start_drag(visual_row, visual_col);
                    }
                }
                // Textarea area: start textarea selection
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let session = &app.session_mgr.sessions[app.session_mgr.active];
                        let (row, col) =
                            mouse::textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .start_selection();
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Scrollbar drag: update scroll offset from mouse Y
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .scrollbar_dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .messages_area
                    {
                        let track_height = area.height.saturating_sub(1);
                        if track_height > 0 {
                            let rel_y = (mouse.row.saturating_sub(area.y)).min(track_height);
                            let max_scroll = app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scrollbar_max_offset;
                            let new_offset =
                                ((rel_y as f64 / track_height as f64) * max_scroll as f64) as u16;
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset = new_offset.min(max_scroll);
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_follow = false;
                        }
                    }
                }
                // Panel scrollbar drag: update panel scroll offset from mouse Y
                {
                    let session = &mut app.session_mgr.sessions[app.session_mgr.active];
                    if session.ui.panel_scrollbar_dragging {
                        if let Some(ref metrics) = session.ui.panel_scrollbar_metrics {
                            let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
                            if bar_inner_height > 0 {
                                let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1))
                                    .min(bar_inner_height);
                                let new_offset = ((rel_y as f64 / bar_inner_height as f64)
                                    * metrics.max_offset as f64)
                                    as u16;
                                let new_offset = new_offset.min(metrics.max_offset);
                                session
                                    .session_panels
                                    .dispatch_set_scroll_offset(new_offset);
                                session.ui.panel_scroll_offset = new_offset;
                            }
                        }
                        return Ok(Some(Action::Redraw));
                    }
                }
                // Panel selection drag
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_selection
                    .dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_area
                    {
                        let content_row = mouse.row.saturating_sub(area.y).saturating_add(
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_scroll_offset,
                        );
                        let col = mouse.column.saturating_sub(area.x);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .update_drag(content_row, col);
                    }
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .text_selection
                    .dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .messages_area
                    {
                        let visual_row = mouse.row.saturating_sub(area.y).saturating_add(
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset,
                        );
                        let visual_col = mouse.column.saturating_sub(area.x);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .update_drag(visual_row, visual_col);
                    }
                }
                // Textarea area: extend textarea selection
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .is_selecting()
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea_area
                    {
                        if mouse.row >= area.y && mouse.row < area.y + area.height {
                            let session = &app.session_mgr.sessions[app.session_mgr.active];
                            let (row, col) =
                                mouse::textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .move_cursor(tui_textarea::CursorMove::Jump(
                                    row as u16, col as u16,
                                ));
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // End scrollbar drag
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .scrollbar_dragging = false;
                // End panel scrollbar drag
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_scrollbar_dragging = false;
                // Panel selection released
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_selection
                    .dragging
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_selection
                        .end_drag();
                    let sel = &app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_selection;
                    if let (Some(start), Some(end)) = (sel.start, sel.end) {
                        let text = crate::app::text_selection::extract_panel_text(
                            start,
                            end,
                            &app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_plain_lines,
                        );
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .set_selected_text(text);
                    }
                    mouse::copy_panel_selection_to_clipboard(app);
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .text_selection
                    .dragging
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .text_selection
                        .end_drag();
                    let ts = &app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .text_selection;
                    if let (Some(start), Some(end)) = (ts.start, ts.end) {
                        let usable_width = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .messages_area
                            .map(|a| a.width.saturating_sub(1))
                            .unwrap_or(0);
                        let cache = app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .render_cache
                            .read();
                        let text = crate::app::text_selection::extract_selected_text(
                            start,
                            end,
                            &cache.wrap_map,
                            usable_width,
                        );
                        drop(cache);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .set_selected_text(text);
                    }
                    mouse::copy_selection_to_clipboard(app);
                }
                // textarea selection on mouse up: no extra handling; tui_textarea maintains
                // its own selection state
            }
            _ => {}
        },
    }

    Ok(Some(Action::Redraw))
}

// ── OAuth prompt ────────────────────────────────────────────────────────────

fn handle_oauth_prompt(app: &mut App, input: Input) {
    use crate::app::handle_edit_key;
    let prompt = match app.global_ui.oauth_prompt.as_mut() {
        Some(p) => p,
        None => return,
    };
    match input {
        Input {
            key: Key::Enter, ..
        } => {
            if prompt.submit() {
                app.global_ui.oauth_prompt = None;
            }
        }
        Input {
            key: Key::Char('o'),
            ctrl: true,
            ..
        } => {
            let url = prompt.authorization_url.clone();
            #[cfg(unix)]
            let _ = std::process::Command::new("open").arg(&url).spawn();
            #[cfg(windows)]
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn();
        }
        Input { key: Key::Esc, .. } => {
            app.global_ui.oauth_prompt = None;
        }
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C in OAuth popup: ignore (no quit)
        }
        _ => {
            prompt.error_message = None;
            handle_edit_key(&mut prompt.input, &mut prompt.cursor, input);
        }
    }
}
