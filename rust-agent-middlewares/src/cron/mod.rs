pub mod middleware;
pub mod tools;

pub use middleware::CronMiddleware;
pub use tools::{CronListTool, CronRegisterTool, CronRemoveTool};

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::mpsc;
use uuid::Uuid;

/// 定时任务最大数量限制
pub const MAX_CRON_TASKS: usize = 20;

/// 定时任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    pub expression: String, // 标准 5 段 cron 表达式
    pub prompt: String,     // 触发时提交的用户输入
    pub next_fire: Option<DateTime<Utc>>, // 下次触发时间（UTC）
    pub enabled: bool,      // 是否启用
}

/// 触发事件（由 CronScheduler 发送到 App）
#[derive(Debug, Clone)]
pub struct CronTrigger {
    pub task_id: String,
    pub prompt: String,
}

/// 定时任务调度器（纯内存）
pub struct CronScheduler {
    tasks: HashMap<String, CronTask>,
    trigger_tx: mpsc::UnboundedSender<CronTrigger>,
}

impl CronScheduler {
    pub fn new(trigger_tx: mpsc::UnboundedSender<CronTrigger>) -> Self {
        Self {
            tasks: HashMap::new(),
            trigger_tx,
        }
    }

    /// 注册新任务
    pub fn register(&mut self, expression: &str, prompt: &str) -> Result<String, String> {
        // 解析 cron 表达式（验证）
        let _cron = croner::Cron::from_str(expression).map_err(|e| {
            format!("cron 表达式无效: {}", e)
        })?;

        // 检查上限
        if self.tasks.len() >= MAX_CRON_TASKS {
            return Err(format!("已达到定时任务上限（{}）", MAX_CRON_TASKS));
        }

        let id = Uuid::now_v7().to_string();
        let next_fire = Self::calculate_next_fire(expression, Utc::now());

        let task = CronTask {
            id: id.clone(),
            expression: expression.to_string(),
            prompt: prompt.to_string(),
            next_fire,
            enabled: true,
        };

        self.tasks.insert(id.clone(), task);
        Ok(id)
    }

    /// 删除任务
    pub fn remove(&mut self, id: &str) -> bool {
        self.tasks.remove(id).is_some()
    }

    /// 切换 enabled/disabled
    pub fn toggle(&mut self, id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(id) {
            task.enabled = !task.enabled;
            if task.enabled {
                task.next_fire = Self::calculate_next_fire(&task.expression, Utc::now());
            }
            true
        } else {
            false
        }
    }

    /// 每秒调用：检查是否有任务到时触发
    pub fn tick(&mut self) {
        let now = Utc::now();
        for task in self.tasks.values_mut() {
            if !task.enabled {
                continue;
            }
            if let Some(next) = task.next_fire {
                if now >= next {
                    let _ = self.trigger_tx.send(CronTrigger {
                        task_id: task.id.clone(),
                        prompt: task.prompt.clone(),
                    });
                    // 计算下次触发时间
                    task.next_fire = Self::calculate_next_fire(&task.expression, now);
                }
            }
        }
    }

    /// 获取所有任务（按下次触发时间排序，无触发时间的排最后）
    pub fn list_tasks(&self) -> Vec<&CronTask> {
        let mut tasks: Vec<&CronTask> = self.tasks.values().collect();
        tasks.sort_by(|a, b| {
            match (&a.next_fire, &b.next_fire) {
                (Some(a_time), Some(b_time)) => a_time.cmp(b_time),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        tasks
    }

    /// 获取单个任务
    pub fn get_task(&self, id: &str) -> Option<&CronTask> {
        self.tasks.get(id)
    }

    /// 计算下次触发时间
    fn calculate_next_fire(expression: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let cron = croner::Cron::from_str(expression).ok()?;
        cron.iter_after(after).next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn new_scheduler() -> (CronScheduler, mpsc::UnboundedReceiver<CronTrigger>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (CronScheduler::new(tx), rx)
    }

    #[test]
    fn test_register_valid() {
        let (mut sched, _rx) = new_scheduler();
        let id = sched.register("* * * * *", "test prompt").unwrap();
        assert!(!id.is_empty());
        let task = sched.get_task(&id).unwrap();
        assert_eq!(task.expression, "* * * * *");
        assert_eq!(task.prompt, "test prompt");
        assert!(task.enabled);
        assert!(task.next_fire.is_some());
    }

    #[test]
    fn test_register_invalid_expression() {
        let (mut sched, _rx) = new_scheduler();
        let result = sched.register("invalid", "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cron 表达式无效"));
    }

    #[test]
    fn test_remove() {
        let (mut sched, _rx) = new_scheduler();
        let id = sched.register("* * * * *", "test").unwrap();
        assert!(sched.remove(&id));
        assert!(!sched.remove(&id));
        assert!(sched.get_task(&id).is_none());
    }

    #[test]
    fn test_toggle() {
        let (mut sched, _rx) = new_scheduler();
        let id = sched.register("* * * * *", "test").unwrap();
        assert!(sched.toggle(&id));
        let task = sched.get_task(&id).unwrap();
        assert!(!task.enabled);
        assert!(sched.toggle(&id));
        let task = sched.get_task(&id).unwrap();
        assert!(task.enabled);
        assert!(task.next_fire.is_some());
    }

    #[test]
    fn test_toggle_nonexistent() {
        let (mut sched, _rx) = new_scheduler();
        assert!(!sched.toggle("nonexistent"));
    }

    #[test]
    fn test_max_tasks() {
        let (mut sched, _rx) = new_scheduler();
        // croner 6-field format: use 5-field standard cron
        for i in 0..20 {
            let expr = format!("* * * * *");
            sched.register(&expr, &format!("task {}", i)).unwrap();
        }
        let result = sched.register("* * * * *", "overflow");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("上限"));
    }

    #[test]
    fn test_tick_fires_trigger() {
        let (mut sched, mut rx) = new_scheduler();
        // Register with a cron that already passed - we manually set next_fire to past
        let id = sched.register("* * * * *", "tick test").unwrap();
        // Force next_fire to the past
        let task = sched.tasks.get_mut(&id).unwrap();
        task.next_fire = Some(Utc::now() - chrono::Duration::seconds(10));

        sched.tick();

        let trigger = rx.try_recv().unwrap();
        assert_eq!(trigger.task_id, id);
        assert_eq!(trigger.prompt, "tick test");

        // next_fire should be updated to future
        let task = sched.get_task(&id).unwrap();
        assert!(task.next_fire.unwrap() > Utc::now() - chrono::Duration::seconds(5));
    }

    #[test]
    fn test_tick_skips_disabled() {
        let (mut sched, mut rx) = new_scheduler();
        let id = sched.register("* * * * *", "skip test").unwrap();
        sched.toggle(&id); // disable
        sched.tick();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_list_tasks() {
        let (mut sched, _rx) = new_scheduler();
        assert!(sched.list_tasks().is_empty());
        sched.register("* * * * *", "a").unwrap();
        sched.register("0 * * * *", "b").unwrap();
        assert_eq!(sched.list_tasks().len(), 2);
    }

    #[test]
    fn test_list_tasks_sorted_by_next_fire() {
        let (mut sched, _rx) = new_scheduler();
        let id1 = sched.register("0 0 1 1 *", "yearly").unwrap();
        let id2 = sched.register("* * * * *", "minutely").unwrap();
        let tasks = sched.list_tasks();
        // minutely 应排在 yearly 前面（next_fire 更早）
        assert_eq!(tasks[0].id, id2);
        assert_eq!(tasks[1].id, id1);
    }

    #[test]
    fn test_register_rejects_empty_prompt() {
        // 校验在 CronRegisterTool::invoke 层，scheduler.register 本身接受空 prompt
        // 此测试验证 scheduler 层不拒绝空 prompt（tools 层拒绝）
        let (mut sched, _rx) = new_scheduler();
        // scheduler.register 接受空字符串（tools 层校验 prompt 非空）
        let result = sched.register("* * * * *", "");
        assert!(result.is_ok(), "scheduler 层不应拒绝空 prompt");
    }
}
