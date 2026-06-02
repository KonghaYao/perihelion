use tui_textarea::Input;

use crate::{
    app::{
        panel_manager::{EventResult, PanelKind},
        App,
    },
    with_global_panels, with_session_panels,
};

use super::super::Action;

/// PanelManager 分发：先处理 session panels，再处理 global panels
pub(super) fn handle_panels(app: &mut App, input: &Input) -> Option<Action> {
    // Session panels: Model, Agent, Hooks, Login, Config, ThreadBrowser
    let session_kind = app.session_mgr.current_mut().session_panels.active_kind();
    if matches!(
        session_kind,
        Some(PanelKind::Model)
            | Some(PanelKind::Agent)
            | Some(PanelKind::Hooks)
            | Some(PanelKind::Login)
            | Some(PanelKind::Config)
            | Some(PanelKind::ThreadBrowser)
    ) {
        with_session_panels!(app, |sp, ctx| {
            let result = sp.dispatch_key(input.clone(), &mut ctx);
            match result {
                EventResult::ClosePanel => {
                    sp.close();
                    app.session_mgr.current_mut().ui.panel_selection.clear();
                    app.session_mgr.current_mut().ui.panel_area = None;
                }
                EventResult::OpenThread(thread_id) => {
                    sp.close();
                    app.session_mgr.current_mut().ui.panel_selection.clear();
                    app.session_mgr.current_mut().ui.panel_area = None;
                    // with_session_panels! macro puts sp back at closure end,
                    // but OpenThread needs to put back first then call open_thread_with_feedback
                    app.session_mgr.current_mut().session_panels = sp;
                    // Early return prevents macro from putting back again
                    app.open_thread_with_feedback(thread_id);
                    return Some(Action::Redraw);
                }
                _ => {}
            }
            result
        });
        return Some(Action::Redraw);
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
        with_global_panels!(app, |pm, ctx| {
            let result = pm.dispatch_key(input.clone(), &mut ctx);
            match result {
                EventResult::ClosePanel => {
                    pm.close();
                    app.session_mgr.current_mut().ui.panel_selection.clear();
                    app.session_mgr.current_mut().ui.panel_area = None;
                }
                EventResult::OpenPanel(PanelKind::Memory) => {
                    app.global_panels = pm;
                    if let Err(e) = app.memory_panel_open_editor() {
                        tracing::error!("Failed to open editor: {}", e);
                    }
                    return Some(Action::Redraw);
                }
                _ => {}
            }
            result
        });
        return Some(Action::Redraw);
    }

    None
}
