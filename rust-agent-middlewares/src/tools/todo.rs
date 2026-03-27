use async_trait::async_trait;
use rust_create_agent::tools::BaseTool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// ─── TodoStatus ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

// ─── TodoItem ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
}

// ─── TodoWriteTool ────────────────────────────────────────────────────────────

/// todo_write 工具：全量覆盖 todo 列表，并通过 channel 通知 TUI 侧
pub struct TodoWriteTool {
    todos: Arc<Mutex<Vec<TodoItem>>>,
    notify_tx: Option<mpsc::Sender<Vec<TodoItem>>>,
}

impl TodoWriteTool {
    pub fn new(notify_tx: mpsc::Sender<Vec<TodoItem>>) -> Self {
        Self {
            todos: Arc::new(Mutex::new(Vec::new())),
            notify_tx: Some(notify_tx),
        }
    }

    /// 获取当前 todo 列表的快照
    pub async fn snapshot(&self) -> Vec<TodoItem> {
        self.todos.lock().await.clone()
    }
}

#[async_trait]
impl BaseTool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Maintain a todo list for complex multi-step tasks. Call this to create or update your todo list with the complete current state. Each call fully replaces the previous list."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "Complete todo list (replaces previous state)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id":      { "type": "string", "description": "Unique identifier" },
                            "content": { "type": "string", "description": "Task description" },
                            "status":  {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Task status"
                            }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let items: Vec<TodoItem> = serde_json::from_value(
            input["todos"].clone(),
        )
        .map_err(|e| format!("todo_write: invalid input: {e}"))?;

        // 全量覆盖
        {
            let mut guard = self.todos.lock().await;
            *guard = items.clone();
        }

        // 通知 TUI；channel 关闭时说明 TUI 已退出，记录 warn 后继续（不影响工具返回值）
        if let Some(tx) = &self.notify_tx {
            if tx.send(items).await.is_err() {
                tracing::warn!("todo_write: notify channel closed, TUI may have disconnected");
            }
        }

        Ok("todo saved successfully".to_string())
    }
}
