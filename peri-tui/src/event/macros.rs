/// Executes a panel dispatch on `global_panels`, automatically handling
/// `mem::take` borrow avoidance.
///
/// Usage: `with_global_panels!(app, |pm, ctx| { ... })` — `pm` is `&mut PanelManager`.
#[macro_export]
macro_rules! with_global_panels {
    ($app:expr, |$pm:ident, $ctx:ident| $body:expr) => {{
        let mut $pm = std::mem::take(&mut $app.global_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
            acp_client: $app.acp_client.clone(),
        };
        let result = { $body };
        $app.global_panels = $pm;
        result
    }};
}

/// Executes a panel dispatch on the active session's `session_panels`,
/// automatically handling `mem::take` borrow avoidance.
///
/// Usage: `with_session_panels!(app, |sp, ctx| { ... })` — `sp` is `&mut PanelManager`.
#[macro_export]
macro_rules! with_session_panels {
    ($app:expr, |$sp:ident, $ctx:ident| $body:expr) => {{
        let mut $sp = std::mem::take(&mut $app.session_mgr.current_mut().session_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
            acp_client: $app.acp_client.clone(),
        };
        let result = { $body };
        $app.session_mgr.current_mut().session_panels = $sp;
        result
    }};
}
