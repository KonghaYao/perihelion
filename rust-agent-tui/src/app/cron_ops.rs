impl crate::app::App {
    /// CronPanel: 光标上移
    pub fn cron_panel_move_up(&mut self) {
        if let Some(ref mut panel) = self.cron.cron_panel {
            panel.move_cursor(-1);
        }
    }

    /// CronPanel: 光标下移
    pub fn cron_panel_move_down(&mut self) {
        if let Some(ref mut panel) = self.cron.cron_panel {
            panel.move_cursor(1);
        }
    }

    /// CronPanel: 切换当前任务的 enabled 状态
    pub fn cron_panel_toggle(&mut self) {
        if let Some(ref mut panel) = self.cron.cron_panel {
            let idx = panel.cursor;
            if idx < panel.tasks.len() {
                let id = panel.tasks[idx].id.clone();
                self.cron.scheduler.lock().toggle(&id);
                panel.refresh(&self.cron.scheduler);
            }
        }
    }

    /// CronPanel: 删除当前任务
    #[allow(dead_code)]
    pub fn cron_panel_delete(&mut self) {
        if let Some(ref mut panel) = self.cron.cron_panel {
            let idx = panel.cursor;
            if idx < panel.tasks.len() {
                let id = panel.tasks[idx].id.clone();
                self.cron.scheduler.lock().remove(&id);
                panel.refresh(&self.cron.scheduler);
                // 列表为空时关闭面板
                if panel.tasks.is_empty() {
                    self.cron.cron_panel = None;
                }
            }
        }
    }

    /// CronPanel: 关闭面板
    pub fn cron_panel_close(&mut self) {
        self.cron.cron_panel = None;
    }
}
