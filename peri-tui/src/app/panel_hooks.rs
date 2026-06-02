use super::*;

impl App {
    /// 打开 /hooks 面板（只读）
    pub fn open_hooks_panel(&mut self) {
        let mut hooks = self
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_hooks.clone())
            .unwrap_or_default();
        // 合并 settings.local.json 中的 hooks
        let global_hooks = peri_middlewares::hooks::loader::load_global_settings_hooks();
        hooks.extend(global_hooks);
        let local_hooks =
            peri_middlewares::hooks::loader::load_settings_local_hooks(&self.services.cwd);
        hooks.extend(local_hooks);
        let panel = HooksPanel::new(hooks);
        self.open_panel(PanelState::Hooks(panel));
    }

    /// 关闭 /hooks 面板
    pub fn close_hooks_panel(&mut self) {
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::Hooks);
    }

    /// 打开 setup 向导（全屏覆盖）
    pub fn open_setup_wizard(&mut self) {
        self.global_ui.setup_wizard =
            Some(super::setup_wizard::SetupWizardPanel::new_from_command());
    }
}
