use std::sync::{Arc, RwLock};

use axum::{routing::delete, routing::get, routing::post, Router};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, EnvFilter};

use acpx_g::watcher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    // Parse CLI args
    let cli = parse_cli_args();

    // Database
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:acpx-g.db?mode=rwc".to_string());
    let pool = Arc::new(acpx_g::db::init(&database_url).await?);

    let templates = Arc::new(RwLock::new(Vec::new()));
    let cancellation_tokens = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

    let state = Arc::new(acpx_g::api::AppState {
        pool: pool.clone(),
        templates: templates.clone(),
        cancellation_tokens: cancellation_tokens.clone(),
    });

    // Cancellation token for graceful watcher shutdown
    let cancel_token = CancellationToken::new();
    let watcher_cancel = cancel_token.clone();

    // Start workflow directory watcher if configured
    let watcher_handle = if let Some(ref workflow_dir) = cli.workflow_dir {
        tracing::info!(dir = %workflow_dir, "starting workflow directory watcher");
        let pool_clone = pool.clone();
        let templates_clone = templates.clone();
        let cancel_tokens_clone = cancellation_tokens.clone();
        let dir = workflow_dir.clone();
        Some(tokio::spawn(async move {
            tokio::select! {
                _ = watcher::watch_directory(pool_clone, templates_clone, cancel_tokens_clone, dir) => {},
                _ = watcher_cancel.cancelled() => {
                    tracing::info!("watcher shutting down");
                }
            }
        }))
    } else {
        None
    };

    // Build CORS layer: configurable via ACPX_CORS_ORIGIN env var
    let cors = build_cors_layer();

    // Router
    let app = Router::new()
        // Health check (for load balancers / orchestrators)
        .route("/health", get(acpx_g::api::health_check))
        // API Docs
        .route("/api/v1/docs", get(acpx_g::api::list_api_docs))
        // Template API
        .route("/api/v1/templates", get(acpx_g::api::list_templates))
        .route(
            "/api/v1/templates/{name}/run",
            post(acpx_g::api::run_template),
        )
        // Workflow API
        .route("/api/v1/workflows", post(acpx_g::api::submit_workflow))
        .route("/api/v1/workflows", get(acpx_g::api::list_workflows))
        .route("/api/v1/workflows/{run_id}", get(acpx_g::api::get_workflow))
        .route(
            "/api/v1/workflows/{run_id}",
            delete(acpx_g::api::delete_workflow_run),
        )
        .route(
            "/api/v1/workflows/{run_id}/cancel",
            post(acpx_g::api::cancel_workflow_run),
        )
        .route(
            "/api/v1/workflows/{run_id}/nodes/{node_id}/logs",
            get(acpx_g::api::get_node_logs),
        )
        // Frontend: serve / as index.html, then all static assets
        .fallback_service(
            ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static"))
                .append_index_html_on_directories(true),
        )
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");

    tracing::info!("acpx-g http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Graceful shutdown: on Ctrl+C, stop watcher, acceptor, and
    // mark any running workflows as failed in the DB.
    let shutdown_pool = pool.clone();
    let shutdown = async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutting down gracefully...");

        // Cancel the watcher first
        cancel_token.cancel();

        // Atomically mark ALL running/pending workflows as failed in a single query.
        // This avoids the race condition where a workflow transitions from pending→running
        // between SELECT and UPDATE.
        let result = sqlx::query(
            "UPDATE workflow_runs SET status = 'failed', error_message = 'server shutdown', \
             finished_at = datetime('now'), started_at = COALESCE(started_at, datetime('now')) \
             WHERE status IN ('running', 'pending')",
        )
        .execute(&*shutdown_pool)
        .await
        .unwrap_or_default();
        tracing::info!(
            rows_affected = result.rows_affected(),
            "marked workflows as failed"
        );

        // Also mark any running node_runs
        let _ = sqlx::query(
            "UPDATE node_runs SET status = 'failed', error_message = 'server shutdown', \
             finished_at = datetime('now') WHERE status IN ('running', 'pending')",
        )
        .execute(&*shutdown_pool)
        .await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    // Wait for watcher to finish
    if let Some(handle) = watcher_handle {
        let _ = handle.await;
    }

    Ok(())
}

/// Build CORS layer from environment configuration.
/// - `ACPX_CORS_ORIGIN` not set or "any" → allow all origins
/// - `ACPX_CORS_ORIGIN` = comma-separated list → allow specific origins
fn build_cors_layer() -> CorsLayer {
    let origin = std::env::var("ACPX_CORS_ORIGIN").unwrap_or_else(|_| "any".to_string());
    let cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);

    if origin == "any" {
        cors.allow_origin(Any)
    } else {
        let origins: Vec<_> = origin
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() {
                    None
                } else {
                    // Parse as header value string
                    s.parse().ok()
                }
            })
            .collect();
        if origins.is_empty() {
            tracing::warn!(
                "ACPX_CORS_ORIGIN set but no valid origins found, falling back to allow all"
            );
            cors.allow_origin(Any)
        } else {
            cors.allow_origin(origins)
        }
    }
}

struct CliArgs {
    workflow_dir: Option<String>,
}

fn parse_cli_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut workflow_dir = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workflow-dir" => {
                if i + 1 < args.len() {
                    workflow_dir = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--help" | "-h" => {
                eprintln!("acpx-g — DAG workflow engine\n");
                eprintln!("USAGE:");
                eprintln!("    acpx-g [OPTIONS]\n");
                eprintln!("OPTIONS:");
                eprintln!("    --workflow-dir <DIR>  Watch directory for workflow YAML files");
                eprintln!("    --help, -h            Show this help message\n");
                eprintln!("ENVIRONMENT:");
                eprintln!("    DATABASE_URL              SQLite connection string (default: sqlite:acpx-g.db?mode=rwc)");
                eprintln!("    PORT                      HTTP server port (default: 3000)");
                eprintln!("    RUST_LOG                  Log level (default: info)");
                eprintln!(
                    "    ACPX_MAX_CONCURRENT       Max parallel nodes per workflow (default: 16)"
                );
                eprintln!("    ACPX_MAX_CONCURRENT_RUNS  Max parallel workflow runs (default: 8)");
                eprintln!("    ACPX_CORS_ORIGIN          CORS origins: 'any' or comma-separated URLs (default: any)");
                std::process::exit(0);
            }
            _ => {
                if args[i].starts_with('-') {
                    tracing::warn!(arg = %args[i], "unknown argument, try --help");
                }
            }
        }
        i += 1;
    }
    CliArgs { workflow_dir }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_args_no_args() {
        // Simulate empty args (just binary name)
        let args = vec!["acpx-g".to_string()];
        let result = parse_cli_args_with(&args);
        assert!(result.workflow_dir.is_none());
    }

    #[test]
    fn test_parse_cli_args_workflow_dir() {
        let args = vec![
            "acpx-g".to_string(),
            "--workflow-dir".to_string(),
            "/tmp/wf".to_string(),
        ];
        let result = parse_cli_args_with(&args);
        assert_eq!(result.workflow_dir.as_deref(), Some("/tmp/wf"));
    }

    #[test]
    fn test_parse_cli_args_workflow_dir_missing_value() {
        let args = vec!["acpx-g".to_string(), "--workflow-dir".to_string()];
        let result = parse_cli_args_with(&args);
        assert!(result.workflow_dir.is_none());
    }

    fn parse_cli_args_with(args: &[String]) -> CliArgs {
        let mut workflow_dir = None;
        let mut i = 1;
        while i < args.len() {
            if args[i].as_str() == "--workflow-dir" && i + 1 < args.len() {
                workflow_dir = Some(args[i + 1].clone());
                i += 1;
            }
            i += 1;
        }
        CliArgs { workflow_dir }
    }
}
