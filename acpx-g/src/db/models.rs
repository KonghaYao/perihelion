use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;

// ─── Workflow Run ─────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_name: String,
    pub workflow_version: String,
    pub yaml_content: String,
    pub status: String,
    pub node_count: i64,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub error_message: Option<String>,
}

impl WorkflowRun {
    pub async fn insert(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO workflow_runs (id, workflow_name, workflow_version, yaml_content, status, node_count, started_at, finished_at, created_at, error_message)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&self.id)
        .bind(&self.workflow_name)
        .bind(&self.workflow_version)
        .bind(&self.yaml_content)
        .bind(&self.status)
        .bind(self.node_count)
        .bind(&self.started_at)
        .bind(&self.finished_at)
        .bind(&self.created_at)
        .bind(&self.error_message)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let finished = if status == "success" || status == "failed" || status == "cancelled" {
            Some(now.clone())
        } else {
            None
        };
        sqlx::query(
            "UPDATE workflow_runs SET status = ?, error_message = ?, finished_at = COALESCE(?, finished_at), started_at = COALESCE(started_at, ?) WHERE id = ?",
        )
        .bind(status)
        .bind(error_message)
        .bind(finished)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_started(pool: &SqlitePool, id: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE workflow_runs SET status = 'running', started_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn find_by_id(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<WorkflowRun>> {
        let run = sqlx::query_as::<_, WorkflowRun>(
            "SELECT id, workflow_name, workflow_version, yaml_content, status, node_count, started_at, finished_at, created_at, error_message FROM workflow_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(run)
    }

    pub async fn list(
        pool: &SqlitePool,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<WorkflowRun>> {
        let runs = sqlx::query_as::<_, WorkflowRun>(
            "SELECT id, workflow_name, workflow_version, '' as yaml_content, status, node_count, started_at, finished_at, created_at, error_message FROM workflow_runs ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
        Ok(runs)
    }

    /// Count total workflow runs.
    pub async fn count(pool: &SqlitePool) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workflow_runs")
            .fetch_one(pool)
            .await?;
        Ok(row.0)
    }

    /// Delete a workflow run and all its node runs by ID (cascade delete).
    pub async fn delete(pool: &SqlitePool, id: &str) -> anyhow::Result<u64> {
        let mut tx = pool.begin().await?;
        let _node_result = sqlx::query("DELETE FROM node_runs WHERE run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let run_result = sqlx::query("DELETE FROM workflow_runs WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(run_result.rows_affected())
    }
}

// ─── Node Run ─────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct NodeRun {
    pub id: String,
    pub run_id: String,
    pub node_id: String,
    pub node_type: String,
    pub status: String,
    pub attempt: i64,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub error_message: Option<String>,
    pub outputs: Option<String>,
    /// JSON array of depends node IDs, stored after reference expansion.
    pub depends: Option<String>,
}

impl NodeRun {
    pub async fn insert(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO node_runs (id, run_id, node_id, node_type, status, attempt, started_at, finished_at, exit_code, stdout, stderr, error_message, outputs, depends)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&self.id)
        .bind(&self.run_id)
        .bind(&self.node_id)
        .bind(&self.node_type)
        .bind(&self.status)
        .bind(self.attempt)
        .bind(&self.started_at)
        .bind(&self.finished_at)
        .bind(self.exit_code)
        .bind(&self.stdout)
        .bind(&self.stderr)
        .bind(&self.error_message)
        .bind(&self.outputs)
        .bind(&self.depends)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_result(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        exit_code: Option<i64>,
        stdout: Option<&str>,
        stderr: Option<&str>,
        error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE node_runs SET status = ?, exit_code = ?, stdout = ?, stderr = ?, error_message = ?, finished_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(exit_code)
        .bind(stdout)
        .bind(stderr)
        .bind(error_message)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update outputs for a completed node.
    pub async fn update_outputs(
        pool: &SqlitePool,
        id: &str,
        outputs_json: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE node_runs SET outputs = ? WHERE id = ?")
            .bind(outputs_json)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn set_started(pool: &SqlitePool, id: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE node_runs SET status = 'running', started_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn find_by_run(pool: &SqlitePool, run_id: &str) -> anyhow::Result<Vec<NodeRun>> {
        let nodes = sqlx::query_as::<_, NodeRun>(
            "SELECT id, run_id, node_id, node_type, status, attempt, started_at, finished_at, exit_code, stdout, stderr, error_message, outputs, depends
             FROM node_runs WHERE run_id = ? ORDER BY node_id",
        )
        .bind(run_id)
        .fetch_all(pool)
        .await?;
        Ok(nodes)
    }

    pub async fn find_by_id(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<NodeRun>> {
        let node = sqlx::query_as::<_, NodeRun>(
            "SELECT id, run_id, node_id, node_type, status, attempt, started_at, finished_at, exit_code, stdout, stderr, error_message, outputs, depends
             FROM node_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(node)
    }

    pub async fn find_by_run_and_node(
        pool: &SqlitePool,
        run_id: &str,
        node_id: &str,
    ) -> anyhow::Result<Option<NodeRun>> {
        let node = sqlx::query_as::<_, NodeRun>(
            "SELECT id, run_id, node_id, node_type, status, attempt, started_at, finished_at, exit_code, stdout, stderr, error_message, outputs, depends
             FROM node_runs WHERE run_id = ? AND node_id = ?",
        )
        .bind(run_id)
        .bind(node_id)
        .fetch_optional(pool)
        .await?;
        Ok(node)
    }

    /// Mark all running nodes in a run as cancelled (used on workflow cancellation).
    pub async fn mark_run_running_as_cancelled(
        pool: &SqlitePool,
        run_id: &str,
    ) -> anyhow::Result<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE node_runs SET status = 'cancelled', error_message = 'cancelled by user', finished_at = ? WHERE run_id = ? AND status = 'running'",
        )
        .bind(&now)
        .bind(run_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Mark all pending nodes in a run as skipped (used when a workflow fails).
    pub async fn mark_run_pending_as_skipped(
        pool: &SqlitePool,
        run_id: &str,
    ) -> anyhow::Result<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE node_runs SET status = 'skipped', finished_at = ? WHERE run_id = ? AND status = 'pending'",
        )
        .bind(&now)
        .bind(run_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}

// ─── API Request Types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SubmitWorkflowRequest {
    /// YAML content of the workflow.
    pub yaml: String,
    /// Runtime input values for the workflow.
    #[serde(default)]
    pub inputs: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct RunTemplateRequest {
    /// Runtime input values for the workflow.
    #[serde(default)]
    pub inputs: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct SubmitWorkflowResponse {
    pub run_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct WorkflowRunResponse {
    pub id: String,
    pub workflow_name: String,
    pub workflow_version: String,
    pub status: String,
    pub node_count: i64,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub error_message: Option<String>,
    pub nodes: Vec<NodeRunResponse>,
}

#[derive(Debug, Serialize)]
pub struct NodeRunResponse {
    pub id: String,
    pub node_id: String,
    pub node_type: String,
    pub status: String,
    pub attempt: i64,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub error_message: Option<String>,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub outputs: Option<serde_json::Value>,
}

impl From<WorkflowRun> for WorkflowRunResponse {
    fn from(r: WorkflowRun) -> Self {
        Self {
            id: r.id,
            workflow_name: r.workflow_name,
            workflow_version: r.workflow_version,
            status: r.status,
            node_count: r.node_count,
            started_at: r.started_at,
            finished_at: r.finished_at,
            created_at: r.created_at,
            error_message: r.error_message,
            nodes: vec![],
        }
    }
}

impl From<NodeRun> for NodeRunResponse {
    fn from(n: NodeRun) -> Self {
        let outputs = n
            .outputs
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let depends = n
            .depends
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();
        Self {
            id: n.id,
            node_id: n.node_id,
            node_type: n.node_type,
            status: n.status,
            attempt: n.attempt,
            started_at: n.started_at,
            finished_at: n.finished_at,
            exit_code: n.exit_code,
            stdout: n.stdout,
            stderr: n.stderr,
            error_message: n.error_message,
            depends,
            outputs,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListRunsResponse {
    pub runs: Vec<WorkflowRunResponse>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
