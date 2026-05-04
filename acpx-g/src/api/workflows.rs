use std::sync::{Arc, RwLock};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::db::{
    ListRunsResponse, NodeRun, NodeRunResponse, SubmitWorkflowRequest, SubmitWorkflowResponse,
    WorkflowRun, WorkflowRunResponse,
};
use crate::runner;

/// Maximum YAML content size (1 MB). Prevents OOM from oversized payloads.
const MAX_YAML_SIZE: usize = 1024 * 1024;

/// Query parameters for list_workflows pagination.
#[derive(Debug, serde::Deserialize)]
pub struct ListWorkflowsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    50
}

/// A workflow template discovered in the watch directory.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowTemplate {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub node_count: usize,
    pub file_path: String,
    pub nodes: Vec<TemplateNodeInfo>,
    #[serde(default)]
    pub inputs: std::collections::HashMap<String, TemplateInputDef>,
}

/// Input definition exposed to the frontend for rendering input forms.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TemplateInputDef {
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub required: bool,
}

/// Lightweight node info for template preview rendering.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TemplateNodeInfo {
    pub id: String,
    pub node_type: String,
    pub depends: Vec<String>,
}

pub struct AppState {
    pub pool: Arc<SqlitePool>,
    pub templates: Arc<RwLock<Vec<WorkflowTemplate>>>,
    pub cancellation_tokens: runner::CancelRegistry,
}

// ─── GET /health ────────────────────────────────────────────────────

pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Verify DB connectivity
    let db_ok = sqlx::query("SELECT 1").execute(&*state.pool).await.is_ok();

    if db_ok {
        (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "degraded", "error": "database unavailable"})),
        )
    }
}

/// Create run + node records in DB inside a transaction, then start async execution.
/// Shared by submit_workflow, run_template, and watcher.
pub async fn create_and_start_run(
    pool: &SqlitePool,
    cancellation_tokens: &runner::CancelRegistry,
    wf: &crate::schema::Workflow,
    expanded_wf: crate::schema::Workflow,
    yaml_content: String,
) -> anyhow::Result<String> {
    let run_id = Uuid::now_v7().to_string();

    // Use a transaction to ensure atomicity: either all records are created or none
    let mut tx = pool.begin().await?;

    let run = WorkflowRun {
        id: run_id.clone(),
        workflow_name: wf.name.clone(),
        workflow_version: wf.version.clone(),
        yaml_content,
        status: "pending".to_string(),
        node_count: expanded_wf.nodes.len() as i64,
        started_at: None,
        finished_at: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        error_message: None,
    };

    // Insert workflow run within transaction
    sqlx::query(
        "INSERT INTO workflow_runs (id, workflow_name, workflow_version, yaml_content, status, node_count, started_at, finished_at, created_at, error_message)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&run.id)
    .bind(&run.workflow_name)
    .bind(&run.workflow_version)
    .bind(&run.yaml_content)
    .bind(&run.status)
    .bind(run.node_count)
    .bind(&run.started_at)
    .bind(&run.finished_at)
    .bind(&run.created_at)
    .bind(&run.error_message)
    .execute(&mut *tx)
    .await?;

    for node in &expanded_wf.nodes {
        let deps = runner::node_depends(node);
        let node_run_id = Uuid::now_v7().to_string();
        let node_id = runner::node_id(node).to_string();
        let node_type = runner::node_type_name(node).to_string();
        let depends_json = if deps.is_empty() {
            None
        } else {
            Some(serde_json::to_string(deps).unwrap())
        };

        sqlx::query(
            "INSERT INTO node_runs (id, run_id, node_id, node_type, status, attempt, started_at, finished_at, exit_code, stdout, stderr, error_message, outputs, depends)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&node_run_id)
        .bind(&run_id)
        .bind(&node_id)
        .bind(&node_type)
        .bind("pending")
        .bind(0i64)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(Option::<i64>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(&depends_json)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let root_inputs = expanded_wf
        .reference_inputs
        .get("__root__")
        .cloned()
        .unwrap_or_default();

    // Create and register cancellation token
    let cancel_token = CancellationToken::new();
    cancellation_tokens
        .write()
        .await
        .insert(run_id.clone(), cancel_token.clone());

    runner::run_workflow(
        Arc::new(pool.clone()),
        run_id.clone(),
        expanded_wf,
        root_inputs,
        cancel_token,
        cancellation_tokens.clone(),
    )
    .await;

    Ok(run_id)
}

// ─── GET /api/v1/docs ────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub description: String,
    pub params: Vec<ApiParam>,
    pub curl: String,
    pub response: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
}

pub async fn list_api_docs() -> impl IntoResponse {
    let endpoints = vec![
        ApiEndpoint {
            method: "GET".into(),
            path: "/health".into(),
            description: "Health check endpoint. Verifies database connectivity.".into(),
            params: vec![],
            curl: "curl http://$HOST/health".into(),
            response: r#"{ "status": "ok" }"#.into(),
            category: Some("system".into()),
        },
        ApiEndpoint {
            method: "DELETE".into(),
            path: "/api/v1/workflows/{run_id}".into(),
            description: "Delete a completed workflow run and all its node runs. Cannot delete running/pending runs.".into(),
            params: vec![],
            curl: "curl -X DELETE http://$HOST/api/v1/workflows/{run_id}".into(),
            response: r#"{ "deleted": "019..." }"#.into(),
            category: Some("workflows".into()),
        },
        ApiEndpoint {
            method: "POST".into(),
            path: "/api/v1/workflows/{run_id}/cancel".into(),
            description: "Cancel a running or pending workflow run. Running nodes are terminated, pending nodes are skipped.".into(),
            params: vec![],
            curl: "curl -X POST http://$HOST/api/v1/workflows/{run_id}/cancel".into(),
            response: r#"{ "status": "cancelled", "run_id": "019..." }"#.into(),
            category: Some("workflows".into()),
        },
        ApiEndpoint {
            method: "POST".into(),
            path: "/api/v1/workflows".into(),
            description: "Submit a workflow YAML for execution. Returns the created run ID.".into(),
            params: vec![
                ApiParam { name: "yaml".into(), param_type: "string".into(), description: "Required. Workflow YAML content.".into() },
                ApiParam { name: "inputs".into(), param_type: "object".into(), description: "Optional. Key-value inputs for template variables.".into() },
            ],
            curl: r#"curl -X POST http://$HOST/api/v1/workflows \
  -H 'Content-Type: application/json' \
  -d '{"yaml": "name: hello\nversion: \"1.0\"\nnodes:\n  - id: greet\n    type: shell\n    run: echo hello"}'"#.into(),
            response: r#"{ "run_id": "019...", "status": "pending" }"#.into(),
            category: Some("workflows".into()),
        },
        ApiEndpoint {
            method: "GET".into(),
            path: "/api/v1/workflows".into(),
            description: "List workflow runs with pagination.".into(),
            params: vec![
                ApiParam { name: "page".into(), param_type: "integer".into(), description: "Page number (default: 1)".into() },
                ApiParam { name: "per_page".into(), param_type: "integer".into(), description: "Items per page, 1-100 (default: 50)".into() },
            ],
            curl: "curl http://$HOST/api/v1/workflows".into(),
            response: r#"{ "runs": [{ "id": "...", "workflow_name": "hello", "status": "success", ... }] }"#.into(),
            category: Some("workflows".into()),
        },
        ApiEndpoint {
            method: "GET".into(),
            path: "/api/v1/workflows/{run_id}".into(),
            description: "Get a single run with all node details, status, stdout, and stderr.".into(),
            params: vec![],
            curl: "curl http://$HOST/api/v1/workflows/{run_id}".into(),
            response: r#"{ "id": "...", "status": "success", "nodes": [{ "node_id": "greet", "status": "success", "stdout": "hello\n" }] }"#.into(),
            category: Some("workflows".into()),
        },
        ApiEndpoint {
            method: "GET".into(),
            path: "/api/v1/workflows/{run_id}/nodes/{node_id}/logs".into(),
            description: "Get logs for a specific node in a run. Uses the node's business ID (not DB id).".into(),
            params: vec![],
            curl: "curl http://$HOST/api/v1/workflows/{run_id}/nodes/greet/logs".into(),
            response: r#"{ "node_id": "greet", "status": "success", "stdout": "hello\n", "stderr": null, "exit_code": 0 }"#.into(),
            category: Some("workflows".into()),
        },
        ApiEndpoint {
            method: "GET".into(),
            path: "/api/v1/templates".into(),
            description: "List all workflow templates discovered from the watched directory.".into(),
            params: vec![],
            curl: "curl http://$HOST/api/v1/templates".into(),
            response: r#"{ "templates": [{ "name": "ci-pipeline", "version": "1.0", "node_count": 4, "inputs": {...} }] }"#.into(),
            category: Some("templates".into()),
        },
        ApiEndpoint {
            method: "POST".into(),
            path: "/api/v1/templates/{name}/run".into(),
            description: "Run a template by name. Optionally pass inputs for template variables.".into(),
            params: vec![
                ApiParam { name: "inputs".into(), param_type: "object".into(), description: "Optional. Key-value inputs matching the template's declared inputs.".into() },
            ],
            curl: r#"curl -X POST http://$HOST/api/v1/templates/{name}/run \
  -H 'Content-Type: application/json' \
  -d '{"inputs": {}}'"#.into(),
            response: r#"{ "run_id": "019...", "status": "pending", "template": "ci-pipeline" }"#.into(),
            category: Some("templates".into()),
        },
    ];
    (
        StatusCode::OK,
        Json(serde_json::json!({ "endpoints": endpoints })),
    )
}

// ─── POST /api/v1/workflows ──────────────────────────────────────

pub async fn submit_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitWorkflowRequest>,
) -> impl IntoResponse {
    // Reject empty YAML early
    if req.yaml.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "yaml field must not be empty"})),
        );
    }

    // Reject oversized YAML to prevent OOM
    if req.yaml.len() > MAX_YAML_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!("yaml content too large: {} bytes (max {} bytes)", req.yaml.len(), MAX_YAML_SIZE)
            })),
        );
    }

    // Parse to get metadata
    let wf = match crate::schema::parse_workflow(&req.yaml) {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid workflow YAML: {e}")})),
            );
        }
    };

    // Validate and apply inputs
    let inputs = match validate_inputs(&wf.inputs, &req.inputs) {
        Ok(i) => i,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid inputs: {e}")})),
            );
        }
    };

    // Load and expand references
    let expanded_wf = match runner::load_workflow_from_content(&req.yaml, inputs).await {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("failed to load workflow: {e}")})),
            );
        }
    };

    match create_and_start_run(
        &state.pool,
        &state.cancellation_tokens,
        &wf,
        expanded_wf,
        req.yaml.clone(),
    )
    .await
    {
        Ok(run_id) => (
            StatusCode::CREATED,
            Json(serde_json::json!(SubmitWorkflowResponse {
                run_id,
                status: "pending".to_string(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("failed to create run: {e}")})),
        ),
    }
}

// ─── GET /api/v1/workflows ───────────────────────────────────────

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListWorkflowsQuery>,
) -> impl IntoResponse {
    let page = params.page.clamp(1, 100_000);
    let per_page = params.per_page.clamp(1, 100);
    let offset = (page - 1).saturating_mul(per_page);

    match (
        WorkflowRun::list(&state.pool, per_page, offset).await,
        WorkflowRun::count(&state.pool).await,
    ) {
        (Ok(runs), Ok(total)) => {
            let response_runs: Vec<WorkflowRunResponse> =
                runs.into_iter().map(WorkflowRunResponse::from).collect();
            (
                StatusCode::OK,
                Json(serde_json::json!(ListRunsResponse {
                    runs: response_runs,
                    total,
                    page,
                    per_page,
                })),
            )
        }
        (Err(e), _) | (_, Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

// ─── GET /api/v1/workflows/:run_id ───────────────────────────────

pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    match WorkflowRun::find_by_id(&state.pool, &run_id).await {
        Ok(Some(run)) => {
            let nodes = NodeRun::find_by_run(&state.pool, &run_id)
                .await
                .unwrap_or_default();

            // depends info is now stored directly in node_runs table
            // (populated during workflow submission after reference expansion)
            let mut response = WorkflowRunResponse::from(run);
            response.nodes = nodes.into_iter().map(NodeRunResponse::from).collect();
            (StatusCode::OK, Json(serde_json::json!(response)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "workflow run not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

// ─── GET /api/v1/workflows/:run_id/nodes/:node_id/logs ───────────

pub async fn get_node_logs(
    State(state): State<Arc<AppState>>,
    Path((run_id, node_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match NodeRun::find_by_run_and_node(&state.pool, &run_id, &node_id).await {
        Ok(Some(node)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "node_id": node.node_id,
                "status": node.status,
                "attempt": node.attempt,
                "stdout": node.stdout,
                "stderr": node.stderr,
                "exit_code": node.exit_code,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "node not found in this run"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

// ─── DELETE /api/v1/workflows/:run_id ────────────────────────────────

pub async fn delete_workflow_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    match WorkflowRun::find_by_id(&state.pool, &run_id).await {
        Ok(Some(run)) => {
            if run.status == "running" || run.status == "pending" {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!("cannot delete run in '{}' status, wait for completion", run.status)
                    })),
                );
            }
            // Use cascade delete (WorkflowRun::delete handles node_runs too)
            match WorkflowRun::delete(&state.pool, &run_id).await {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({"deleted": run_id}))),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("{e}")})),
                ),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "workflow run not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

// ─── POST /api/v1/workflows/:run_id/cancel ────────────────────────────

pub async fn cancel_workflow_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    match WorkflowRun::find_by_id(&state.pool, &run_id).await {
        Ok(Some(run)) => {
            // Idempotent: already cancelled
            if run.status == "cancelled" {
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "status": "cancelled",
                        "run_id": run_id,
                        "message": "already cancelled"
                    })),
                );
            }

            // Can only cancel running or pending workflows
            if run.status != "running" && run.status != "pending" {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!("cannot cancel run in '{}' status", run.status)
                    })),
                );
            }

            // Signal cancellation via token
            let tokens = state.cancellation_tokens.read().await;
            if let Some(token) = tokens.get(&run_id) {
                token.cancel();
            }
            drop(tokens);

            // Update workflow status
            let _ = WorkflowRun::update_status(
                &state.pool,
                &run_id,
                "cancelled",
                Some("cancelled by user"),
            )
            .await;

            // Mark running nodes as cancelled, pending nodes as skipped
            let _ = NodeRun::mark_run_running_as_cancelled(&state.pool, &run_id).await;
            let _ = NodeRun::mark_run_pending_as_skipped(&state.pool, &run_id).await;

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "cancelled",
                    "run_id": run_id,
                })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "workflow run not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

// ─── GET /api/v1/templates ───────────────────────────────────────

pub async fn list_templates(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let templates = state.templates.read().unwrap().clone();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "templates": templates })),
    )
}

// ─── POST /api/v1/templates/:name/run ────────────────────────────

pub async fn run_template(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: Option<Json<crate::db::RunTemplateRequest>>,
) -> impl IntoResponse {
    let inputs_opt = body.and_then(|Json(b)| b.inputs);
    let templates = state.templates.read().unwrap().clone();
    let template = match templates.iter().find(|t| t.name == name) {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "template not found"})),
            );
        }
    };

    let yaml_content = match std::fs::read_to_string(&template.file_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("failed to read template file: {e}")})),
            );
        }
    };

    let wf = match crate::schema::parse_workflow(&yaml_content) {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid workflow: {e}")})),
            );
        }
    };

    // Validate and apply inputs from request body
    let inputs = match validate_inputs(&wf.inputs, &inputs_opt) {
        Ok(i) => i,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid inputs: {e}")})),
            );
        }
    };

    // Load and expand references
    let expanded_wf = match runner::load_workflow(&template.file_path, inputs).await {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("failed to load workflow: {e}")})),
            );
        }
    };

    match create_and_start_run(
        &state.pool,
        &state.cancellation_tokens,
        &wf,
        expanded_wf,
        yaml_content,
    )
    .await
    {
        Ok(run_id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "run_id": run_id,
                "status": "pending",
                "template": name,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

// ─── Input Validation ─────────────────────────────────────────────

/// Validate provided inputs against declared InputDefs.
/// Returns a fully populated HashMap with defaults applied.
fn validate_inputs(
    declared: &std::collections::HashMap<String, crate::schema::InputDef>,
    provided: &Option<std::collections::HashMap<String, String>>,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let mut result = std::collections::HashMap::new();
    let provided = provided.as_ref();

    for (key, def) in declared {
        if let Some(val) = provided.and_then(|p| p.get(key)) {
            // Type validation
            match def.input_type {
                crate::schema::InputType::Number => {
                    if val.parse::<f64>().is_err() {
                        anyhow::bail!("input '{}' must be a number, got '{}'", key, val);
                    }
                }
                crate::schema::InputType::Boolean => {
                    let lower = val.to_lowercase();
                    if !matches!(lower.as_str(), "true" | "false" | "1" | "0" | "yes" | "no") {
                        anyhow::bail!(
                            "input '{}' must be a boolean (true/false), got '{}'",
                            key,
                            val
                        );
                    }
                }
                crate::schema::InputType::String => {}
            }
            result.insert(key.clone(), val.clone());
        } else if let Some(default) = &def.default {
            // Validate default value matches declared type
            match def.input_type {
                crate::schema::InputType::Number => {
                    if default.parse::<f64>().is_err() {
                        anyhow::bail!(
                            "input '{}' default value '{}' is not a valid number",
                            key,
                            default
                        );
                    }
                }
                crate::schema::InputType::Boolean => {
                    let lower = default.to_lowercase();
                    if !matches!(lower.as_str(), "true" | "false" | "1" | "0" | "yes" | "no") {
                        anyhow::bail!(
                            "input '{}' default value '{}' is not a valid boolean",
                            key,
                            default
                        );
                    }
                }
                crate::schema::InputType::String => {}
            }
            result.insert(key.clone(), default.clone());
        } else if def.required {
            anyhow::bail!("required input '{}' not provided", key);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{InputDef, InputType};
    use std::collections::HashMap;

    fn make_input_def(input_type: InputType, required: bool, default: Option<&str>) -> InputDef {
        InputDef {
            input_type,
            required,
            default: default.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_validate_inputs_string_ok() {
        let mut declared = HashMap::new();
        declared.insert(
            "name".to_string(),
            make_input_def(InputType::String, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("name".to_string(), "hello".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert_eq!(result.get("name").unwrap(), "hello");
    }

    #[test]
    fn test_validate_inputs_number_ok() {
        let mut declared = HashMap::new();
        declared.insert(
            "count".to_string(),
            make_input_def(InputType::Number, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("count".to_string(), "42".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert_eq!(result.get("count").unwrap(), "42");
    }

    #[test]
    fn test_validate_inputs_number_invalid() {
        let mut declared = HashMap::new();
        declared.insert(
            "count".to_string(),
            make_input_def(InputType::Number, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("count".to_string(), "abc".to_string());
        let err = validate_inputs(&declared, &Some(provided)).unwrap_err();
        assert!(err.to_string().contains("must be a number"));
    }

    #[test]
    fn test_validate_inputs_boolean_ok() {
        let mut declared = HashMap::new();
        declared.insert(
            "flag".to_string(),
            make_input_def(InputType::Boolean, true, None),
        );
        for val in &["true", "false", "yes", "no", "1", "0", "True", "FALSE"] {
            let mut provided = HashMap::new();
            provided.insert("flag".to_string(), val.to_string());
            assert!(validate_inputs(&declared, &Some(provided)).is_ok());
        }
    }

    #[test]
    fn test_validate_inputs_boolean_invalid() {
        let mut declared = HashMap::new();
        declared.insert(
            "flag".to_string(),
            make_input_def(InputType::Boolean, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("flag".to_string(), "maybe".to_string());
        let err = validate_inputs(&declared, &Some(provided)).unwrap_err();
        assert!(err.to_string().contains("must be a boolean"));
    }

    #[test]
    fn test_validate_inputs_required_missing() {
        let mut declared = HashMap::new();
        declared.insert(
            "tag".to_string(),
            make_input_def(InputType::String, true, None),
        );
        let err = validate_inputs(&declared, &None).unwrap_err();
        assert!(err.to_string().contains("required input 'tag'"));
    }

    #[test]
    fn test_validate_inputs_default_applied() {
        let mut declared = HashMap::new();
        declared.insert(
            "env".to_string(),
            make_input_def(InputType::String, false, Some("staging")),
        );
        let result = validate_inputs(&declared, &None).unwrap();
        assert_eq!(result.get("env").unwrap(), "staging");
    }

    #[test]
    fn test_validate_inputs_optional_not_provided() {
        let mut declared = HashMap::new();
        declared.insert(
            "extra".to_string(),
            make_input_def(InputType::String, false, None),
        );
        let result = validate_inputs(&declared, &None).unwrap();
        assert!(!result.contains_key("extra"));
    }

    #[test]
    fn test_validate_inputs_empty_declared() {
        let result = validate_inputs(&HashMap::new(), &None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_validate_inputs_extra_provided_ignored() {
        let declared = HashMap::new();
        let mut provided = HashMap::new();
        provided.insert("unknown_key".to_string(), "value".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_validate_inputs_negative_number() {
        let mut declared = HashMap::new();
        declared.insert(
            "val".to_string(),
            make_input_def(InputType::Number, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("val".to_string(), "-3.14".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert_eq!(result.get("val").unwrap(), "-3.14");
    }

    #[test]
    fn test_validate_inputs_default_number_invalid() {
        let mut declared = HashMap::new();
        declared.insert(
            "count".to_string(),
            make_input_def(InputType::Number, false, Some("not_a_number")),
        );
        let err = validate_inputs(&declared, &None).unwrap_err();
        assert!(err.to_string().contains("not a valid number"));
    }

    #[test]
    fn test_validate_inputs_default_boolean_invalid() {
        let mut declared = HashMap::new();
        declared.insert(
            "flag".to_string(),
            make_input_def(InputType::Boolean, false, Some("maybe")),
        );
        let err = validate_inputs(&declared, &None).unwrap_err();
        assert!(err.to_string().contains("not a valid boolean"));
    }

    #[test]
    fn test_validate_inputs_default_number_valid() {
        let mut declared = HashMap::new();
        declared.insert(
            "count".to_string(),
            make_input_def(InputType::Number, false, Some("42")),
        );
        let result = validate_inputs(&declared, &None).unwrap();
        assert_eq!(result.get("count").unwrap(), "42");
    }

    #[test]
    fn test_validate_inputs_default_boolean_valid() {
        let mut declared = HashMap::new();
        declared.insert(
            "flag".to_string(),
            make_input_def(InputType::Boolean, false, Some("true")),
        );
        let result = validate_inputs(&declared, &None).unwrap();
        assert_eq!(result.get("flag").unwrap(), "true");
    }

    #[test]
    fn test_validate_inputs_provided_overrides_default() {
        let mut declared = HashMap::new();
        declared.insert(
            "env".to_string(),
            make_input_def(InputType::String, false, Some("staging")),
        );
        let mut provided = HashMap::new();
        provided.insert("env".to_string(), "production".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert_eq!(result.get("env").unwrap(), "production");
    }

    #[test]
    fn test_validate_inputs_number_zero() {
        let mut declared = HashMap::new();
        declared.insert(
            "val".to_string(),
            make_input_def(InputType::Number, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("val".to_string(), "0".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert_eq!(result.get("val").unwrap(), "0");
    }

    #[test]
    fn test_validate_inputs_number_scientific() {
        let mut declared = HashMap::new();
        declared.insert(
            "val".to_string(),
            make_input_def(InputType::Number, true, None),
        );
        let mut provided = HashMap::new();
        provided.insert("val".to_string(), "1e10".to_string());
        let result = validate_inputs(&declared, &Some(provided)).unwrap();
        assert_eq!(result.get("val").unwrap(), "1e10");
    }

    #[test]
    fn test_list_workflows_query_defaults() {
        let query: ListWorkflowsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.page, 1);
        assert_eq!(query.per_page, 50);
    }

    #[test]
    fn test_list_workflows_query_custom() {
        let query: ListWorkflowsQuery =
            serde_json::from_str(r#"{"page": 3, "per_page": 20}"#).unwrap();
        assert_eq!(query.page, 3);
        assert_eq!(query.per_page, 20);
    }

    #[test]
    fn test_list_workflows_query_negative_page() {
        // Negative page is clamped to 1 by the handler
        let query: ListWorkflowsQuery =
            serde_json::from_str(r#"{"page": -5, "per_page": 10}"#).unwrap();
        assert_eq!(query.page, -5);
        // Handler clamps: page.max(1) = 1
    }

    #[test]
    fn test_yaml_size_limit_constant() {
        assert_eq!(MAX_YAML_SIZE, 1024 * 1024);
    }
}
