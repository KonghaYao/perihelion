#[cfg(not(feature = "server"))]
compile_error!("relay-server binary requires the 'server' feature");

use std::env;
use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use rust_relay_server::auth;
use rust_relay_server::relay::{self, RelayState};
use rust_relay_server::static_files;

#[derive(Deserialize)]
struct AgentWsQuery {
    token: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct WebWsQuery {
    token: Option<String>,
    session: Option<String>,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RelayState>>,
    Query(params): Query<AgentWsQuery>,
) -> impl IntoResponse {
    if let Err(code) = auth::validate_token(params.token.as_deref(), &state.token) {
        return code.into_response();
    }
    // 软检查连接数上限（在 handle_agent_ws 中还有精确计数）
    use std::sync::atomic::Ordering;
    if state.active_agent_conns.load(Ordering::Relaxed) >= state.max_agent_conns {
        tracing::warn!(
            limit = state.max_agent_conns,
            "Relay: agent 连接数已达上限，返回 429"
        );
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    ws.on_upgrade(move |socket| relay::handle_agent_ws(socket, state, params.name))
}

async fn web_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RelayState>>,
    Query(params): Query<WebWsQuery>,
) -> impl IntoResponse {
    if let Err(code) = auth::validate_token(params.token.as_deref(), &state.token) {
        return code.into_response();
    }
    // 软检查 web 连接数上限
    use std::sync::atomic::Ordering;
    if state.active_web_conns.load(Ordering::Relaxed) >= state.max_web_conns {
        tracing::warn!(
            limit = state.max_web_conns,
            "Relay: web 连接数已达上限，返回 429"
        );
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    match params.session {
        Some(session_id) => ws
            .on_upgrade(move |socket| {
                relay::handle_web_session_ws(socket, state, session_id)
            })
            .into_response(),
        None => ws
            .on_upgrade(move |socket| relay::handle_web_management_ws(socket, state))
            .into_response(),
    }
}

async fn agents_handler(
    State(state): State<Arc<RelayState>>,
    Query(params): Query<TokenQuery>,
) -> impl IntoResponse {
    if let Err(code) = auth::validate_token(params.token.as_deref(), &state.token) {
        return code.into_response();
    }
    let agents = state.agents_list();
    axum::Json(agents).into_response()
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let token = env::var("RELAY_TOKEN").expect("RELAY_TOKEN environment variable is required");
    let port: u16 = env::var("RELAY_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("RELAY_PORT must be a valid port number");

    // 连接数限制（可通过环境变量调整，保守默认值适合单机部署场景）
    let max_agent_conns: usize = env::var("MAX_AGENT_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let max_web_conns: usize = env::var("MAX_WEB_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);

    tracing::info!(
        max_agent_conns,
        max_web_conns,
        max_web_conns_per_session = relay::MAX_WEB_CONNS_PER_SESSION,
        "Relay 连接限制配置"
    );

    let state = RelayState::new_with_limits(token, max_agent_conns, max_web_conns);

    // Start session cleanup task
    relay::spawn_session_cleanup(state.clone());

    let app = Router::new()
        .route("/agent/ws", get(agent_ws_handler))
        .route("/web/ws", get(web_ws_handler))
        .route("/agents", get(agents_handler))
        .route("/health", get(health_handler))
        .route("/web/", get(static_files::index_handler))
        .route("/web/{*path}", get(static_files::static_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");

    tracing::info!("Relay Server 已启动，监听 0.0.0.0:{}", port);

    axum::serve(listener, app).await.expect("Server error");
}
