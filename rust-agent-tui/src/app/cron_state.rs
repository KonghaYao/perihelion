use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rust_agent_middlewares::cron::{CronScheduler, CronTask, CronTrigger};
use tokio::sync::mpsc;

/// CronPanel 面板状态
#[derive(Debug, Clone)]
pub struct CronPanel {
    pub tasks: Vec<CronTask>,
    pub cursor: usize,
    pub scroll_offset: u16,
}

impl CronPanel {
    pub fn new(tasks: Vec<CronTask>) -> Self {
        Self {
            tasks,
            cursor: 0,
            scroll_offset: 0,
        }
    }

    pub fn move_cursor(&mut self, delta: i32) {
        if self.tasks.is_empty() {
            return;
        }
        let max = self.tasks.len() - 1;
        let new = self.cursor as i32 + delta;
        self.cursor = new.clamp(0, max as i32) as usize;
    }

    pub fn refresh(&mut self, scheduler: &Mutex<CronScheduler>) {
        self.tasks = scheduler.lock().list_tasks().into_iter().cloned().collect();
        if self.cursor >= self.tasks.len() && !self.tasks.is_empty() {
            self.cursor = self.tasks.len() - 1;
        }
    }
}

/// Cron 状态（App 子结构体）
pub struct CronState {
    pub scheduler: Arc<Mutex<CronScheduler>>,
    pub trigger_rx: Option<mpsc::UnboundedReceiver<CronTrigger>>,
    pub cron_panel: Option<CronPanel>,
}

impl CronState {
    pub fn new() -> (Self, Arc<Mutex<CronScheduler>>) {
        let (trigger_tx, trigger_rx) = mpsc::unbounded_channel();
        let scheduler = CronScheduler::new(trigger_tx);
        let scheduler = Arc::new(Mutex::new(scheduler));

        let state = Self {
            scheduler: scheduler.clone(),
            trigger_rx: Some(trigger_rx),
            cron_panel: None,
        };
        (state, scheduler)
    }

    /// Spawn CronManager tick task
    pub fn spawn_tick_task(scheduler: Arc<Mutex<CronScheduler>>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                scheduler.lock().tick();
            }
        });
    }
}

impl Default for CronState {
    fn default() -> Self {
        let (state, _scheduler) = Self::new();
        state
    }
}
