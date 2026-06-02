use super::cron_state::CronPanel;

impl crate::app::App {
    /// CronPanel: 光标上移
    pub fn cron_panel_move_up(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<CronPanel>() {
            panel.list.move_cursor(-1);
        }
    }

    /// CronPanel: 光标下移
    pub fn cron_panel_move_down(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<CronPanel>() {
            panel.list.move_cursor(1);
        }
    }

    /// CronPanel: 切换当前任务的 enabled 状态
    pub fn cron_panel_toggle(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<CronPanel>() {
            let idx = panel.cursor();
            if idx < panel.tasks().len() {
                let id = panel.tasks()[idx].id.clone();
                self.services.cron.scheduler.lock().toggle(&id);
                panel.refresh(&self.services.cron.scheduler);
            }
        }
    }

    /// CronPanel: 请求删除当前任务（进入确认状态）
    pub fn cron_panel_request_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<CronPanel>() {
            if panel.cursor() < panel.tasks().len() {
                panel.confirm_delete = true;
            }
        }
    }

    /// CronPanel: 确认删除当前任务
    pub fn cron_panel_confirm_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<CronPanel>() {
            panel.confirm_delete = false;
            let idx = panel.cursor();
            if idx < panel.tasks().len() {
                let prompt_preview: String = panel.tasks()[idx].prompt.chars().take(30).collect();
                let id = panel.tasks()[idx].id.clone();
                self.services.cron.scheduler.lock().remove(&id);
                panel.refresh(&self.services.cron.scheduler);
                self.session_mgr
                    .current_mut()
                    .messages
                    .push_system_note(self.services.lc.tr_args(
                        "app-cron-deleted",
                        &[("preview".into(), prompt_preview.into())],
                    ));
                // 列表为空时关闭面板，清理面板元数据
                if panel.tasks().is_empty() {
                    self.global_panels.close();
                    self.session_mgr.current_mut().ui.panel_selection.clear();
                    self.session_mgr.current_mut().ui.panel_area = None;
                }
            }
        }
    }

    /// CronPanel: 取消删除确认
    pub fn cron_panel_cancel_delete(&mut self) {
        if let Some(panel) = self.global_panels.get_mut::<CronPanel>() {
            panel.confirm_delete = false;
        }
    }

    /// CronPanel: 关闭面板
    pub fn cron_panel_close(&mut self) {
        self.global_panels.close();
    }
}
