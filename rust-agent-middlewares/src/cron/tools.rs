use async_trait::async_trait;
use parking_lot::Mutex;
use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::sync::Arc;

use super::CronScheduler;

pub struct CronRegisterTool {
    scheduler: Arc<Mutex<CronScheduler>>,
}

impl CronRegisterTool {
    pub fn new(scheduler: Arc<Mutex<CronScheduler>>) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl BaseTool for CronRegisterTool {
    fn name(&self) -> &str {
        "cron_register"
    }

    fn description(&self) -> &str {
        "Register a scheduled task that will automatically send a user message at the specified cron interval. The task runs in-memory only (lost on restart). Use standard 5-field cron expression."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Standard 5-field cron expression (e.g. '*/5 * * * *' for every 5 minutes)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The user message to send when the task triggers"
                }
            },
            "required": ["expression", "prompt"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let expression = input["expression"]
            .as_str()
            .ok_or_else(|| "missing expression field".to_string())?;
        let prompt = input["prompt"]
            .as_str()
            .ok_or_else(|| "missing prompt field".to_string())?
            .trim();
        if prompt.is_empty() {
            return Err("prompt 不能为空".into());
        }

        let mut scheduler = self.scheduler.lock();

        match scheduler.register(expression, prompt) {
            Ok(id) => Ok(format!(
                "已注册定时任务 {}（{}），prompt: {}",
                id, expression, prompt
            )),
            Err(e) => Err(e.into()),
        }
    }
}

pub struct CronListTool {
    scheduler: Arc<Mutex<CronScheduler>>,
}

impl CronListTool {
    pub fn new(scheduler: Arc<Mutex<CronScheduler>>) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl BaseTool for CronListTool {
    fn name(&self) -> &str {
        "cron_list"
    }

    fn description(&self) -> &str {
        "List all registered cron/scheduled tasks with their status, next fire time, and prompt."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn invoke(
        &self,
        _input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let scheduler = self.scheduler.lock();

        let tasks = scheduler.list_tasks();
        if tasks.is_empty() {
            return Ok("无定时任务".to_string());
        }

        let mut lines = Vec::new();
        for task in tasks {
            let status = if task.enabled { "启用" } else { "禁用" };
            let next = task
                .next_fire
                .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "N/A".to_string());
            lines.push(format!(
                "- {} [{}] {} | 下次触发: {} | prompt: {}",
                task.id.get(..8).unwrap_or(&task.id),
                status,
                task.expression,
                next,
                task.prompt
            ));
        }
        Ok(lines.join("\n"))
    }
}

pub struct CronRemoveTool {
    scheduler: Arc<Mutex<CronScheduler>>,
}

impl CronRemoveTool {
    pub fn new(scheduler: Arc<Mutex<CronScheduler>>) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl BaseTool for CronRemoveTool {
    fn name(&self) -> &str {
        "cron_remove"
    }

    fn description(&self) -> &str {
        "Remove/delete a registered cron task by its ID."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The cron task ID to remove"
                }
            },
            "required": ["id"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let id = input["id"]
            .as_str()
            .ok_or_else(|| "missing id field".to_string())?;

        let mut scheduler = self.scheduler.lock();

        if scheduler.remove(id) {
            Ok(format!("已删除定时任务 {}", id))
        } else {
            Err(format!("定时任务 {} 不存在", id).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn new_tools() -> (CronRegisterTool, CronListTool, CronRemoveTool) {
        let (tx, _rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(Mutex::new(CronScheduler::new(tx)));
        (
            CronRegisterTool::new(scheduler.clone()),
            CronListTool::new(scheduler.clone()),
            CronRemoveTool::new(scheduler),
        )
    }

    #[tokio::test]
    async fn test_register_rejects_empty_prompt() {
        let (reg, _, _) = new_tools();
        let result = reg
            .invoke(serde_json::json!({"expression": "* * * * *", "prompt": ""}))
            .await;
        assert!(result.is_err(), "空 prompt 应被拒绝");
    }

    #[tokio::test]
    async fn test_register_rejects_whitespace_prompt() {
        let (reg, _, _) = new_tools();
        let result = reg
            .invoke(serde_json::json!({"expression": "* * * * *", "prompt": "   "}))
            .await;
        assert!(result.is_err(), "纯空白 prompt 应被拒绝");
    }

    #[tokio::test]
    async fn test_register_success() {
        let (reg, list, _) = new_tools();
        let result = reg
            .invoke(serde_json::json!({"expression": "* * * * *", "prompt": "test task"}))
            .await
            .unwrap();
        assert!(result.contains("已注册"));

        let list_result = list.invoke(serde_json::json!({})).await.unwrap();
        assert!(list_result.contains("test task"));
    }
}
