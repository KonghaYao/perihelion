//! ACP Request dispatch — handles all ACP protocol request methods.
//! Extracted from original acp_server.rs (2026-05-20 split).

use std::collections::HashMap;

use serde_json::Value;
use tracing::{debug, info};

use peri_acp::transport::types::AcpError;
use peri_agent::thread::{ThreadId, ThreadMeta};

use agent_client_protocol::schema::{
    AgentCapabilities, CloseSessionResponse, ForkSessionResponse, InitializeResponse,
    ListSessionsResponse, LoadSessionResponse, NewSessionResponse, PromptCapabilities,
    ProtocolVersion, ResumeSessionResponse, SessionCapabilities, SessionCloseCapabilities,
    SessionForkCapabilities, SessionId, SessionInfo, SessionListCapabilities,
    SessionResumeCapabilities, SetSessionConfigOptionResponse, SetSessionModeResponse,
    SetSessionModelResponse,
};

use crate::app::agent::LlmProvider;

use super::notify::{
    extract_session_id, send_available_commands_update, send_config_option_update,
};
use super::{
    apply_thinking_effort, build_config_options, build_mode_state, build_model_state,
    parse_permission_mode, AcpServerConfig, SessionState,
};

pub(crate) async fn handle_request(
    method: &str,
    params: &Value,
    cfg: &AcpServerConfig,
    sessions: &mut HashMap<String, SessionState>,
    transport: &dyn peri_acp::transport::AcpTransport,
) -> Result<Value, AcpError> {
    match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            info!(protocol_version = %version, "ACP initialize");
            let caps = AgentCapabilities::new()
                .load_session(true)
                .prompt_capabilities(PromptCapabilities::new())
                .session_capabilities(
                    SessionCapabilities::new()
                        .list(SessionListCapabilities::new())
                        .close(SessionCloseCapabilities::new())
                        .resume(SessionResumeCapabilities::new())
                        .fork(SessionForkCapabilities::new()),
                );
            let resp = InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(caps);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/new" => {
            let cwd = params
                .get("cwd")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            let meta = ThreadMeta::new(&cwd);
            let thread_id = cfg
                .thread_store
                .create_thread(meta)
                .await
                .map_err(|e| AcpError::new(-32603, format!("Thread creation failed: {e}")))?;
            let session_id = thread_id.clone();
            sessions.insert(
                session_id.clone(),
                SessionState {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    cwd: cwd.clone(),
                    history: Vec::new(),
                    cancel_token: None,
                    frozen_system_prompt: None,
                    frozen_claude_md: None,
                    frozen_claude_local_md: None,
                    frozen_skill_summary: None,
                    frozen_date: None,
                },
            );

            // ── Freeze system prompt data at session creation ──
            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();

            let (frozen_claude_md, frozen_claude_local_md) =
                peri_middlewares::AgentsMdMiddleware::read_frozen_content(&cwd);

            let frozen_skill_summary = peri_middlewares::SkillsMiddleware::build_frozen_summary(
                &cwd,
                &cfg.plugin_skill_dirs,
            );

            let features = peri_acp::prompt::PromptFeatures::detect();
            let system_prompt = peri_acp::prompt::build_system_prompt(
                None,
                &cwd,
                features,
                &cfg.plugin_agent_dirs,
                Some(&frozen_date),
            );

            let state = sessions.get_mut(&session_id).unwrap();
            state.frozen_system_prompt = Some(system_prompt);
            state.frozen_claude_md = frozen_claude_md;
            state.frozen_claude_local_md = frozen_claude_local_md;
            state.frozen_skill_summary = frozen_skill_summary;
            state.frozen_date = Some(frozen_date);
            info!(session_id = %session_id, "ACP session created with ThreadStore");
            let modes = build_mode_state(&cfg.permission_mode);
            let models = {
                let p = cfg.provider.read();
                let c = cfg.peri_config.read();
                build_model_state(&p, &c)
            };
            let config_options = {
                let c = cfg.peri_config.read();
                let p = cfg.provider.read();
                build_config_options(&c, &p, cfg.permission_mode.load())
            };
            let resp = NewSessionResponse::new(SessionId::new(&*session_id))
                .modes(modes)
                .models(models)
                .config_options(config_options);
            send_available_commands_update(transport, &session_id).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_model" => {
            let model_id = params.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
            let session_id = extract_session_id(params, "");
            let new_provider = {
                let cfg = cfg.peri_config.read();
                LlmProvider::from_config_for_alias(&cfg, model_id)
            };
            if let Some(new_provider) = new_provider {
                info!(model_id = %model_id, model = %new_provider.model_name(), "Model changed");
                *cfg.provider.write() = new_provider;
            }
            let resp = SetSessionModelResponse::new();
            send_config_option_update(transport, session_id, cfg).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_mode" => {
            let mode_id = params
                .get("modeId")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let session_id = extract_session_id(params, "");
            let mode = parse_permission_mode(mode_id);
            cfg.permission_mode.store(mode);
            info!(mode_id = %mode_id, "Permission mode changed");
            let resp = SetSessionModeResponse::new();
            send_config_option_update(transport, session_id, cfg).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_config_option" => {
            let config_id = params
                .get("configId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let session_id = extract_session_id(params, "");
            let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");
            match config_id {
                "mode" => {
                    let mode = parse_permission_mode(value);
                    cfg.permission_mode.store(mode);
                    info!(mode = %value, "Permission mode changed via configOption");
                }
                "model" => {
                    let new_provider = {
                        let c = cfg.peri_config.read();
                        LlmProvider::from_config_for_alias(&c, value)
                    };
                    if let Some(new_provider) = new_provider {
                        info!(model_id = %value, model = %new_provider.model_name(), "Model changed via configOption");
                        *cfg.provider.write() = new_provider;
                    }
                }
                "thinking_effort" => {
                    apply_thinking_effort(&cfg.peri_config, value);
                    info!(effort = %value, "Thinking effort changed via configOption");
                }
                _ => {
                    debug!(config_id = %config_id, "Unknown config option");
                }
            }
            let config_options = {
                let c = cfg.peri_config.read();
                let p = cfg.provider.read();
                build_config_options(&c, &p, cfg.permission_mode.load())
            };
            let resp = SetSessionConfigOptionResponse::new(config_options);
            send_config_option_update(transport, session_id, cfg).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_thinking" => {
            let effort = params
                .get("effort")
                .and_then(|v| v.as_str())
                .unwrap_or("medium");
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let session_id = extract_session_id(params, "");
            apply_thinking_effort(&cfg.peri_config, effort);
            {
                let mut cfg_guard = cfg.peri_config.write();
                if let Some(ref mut thinking) = cfg_guard.config.thinking {
                    thinking.enabled = enabled;
                }
            }
            info!(effort = %effort, enabled = %enabled, "Thinking config changed");
            let config_options = {
                let c = cfg.peri_config.read();
                let p = cfg.provider.read();
                build_config_options(&c, &p, cfg.permission_mode.load())
            };
            let resp = SetSessionConfigOptionResponse::new(config_options);
            send_config_option_update(transport, session_id, cfg).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/load" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

            // Load history from ThreadStore
            let history = match cfg
                .thread_store
                .load_messages(&ThreadId::from(req_session_id.to_string()))
                .await
            {
                Ok(msgs) => msgs,
                Err(e) => {
                    tracing::warn!(session_id = %req_session_id, error = %e, "session/load: thread not found, creating empty session");
                    Vec::new()
                }
            };

            // Insert into sessions if not already present
            if let Some(state) = sessions.get_mut(req_session_id) {
                if state.history.is_empty() {
                    state.history = history;
                }
            } else {
                sessions.insert(
                    req_session_id.to_string(),
                    SessionState {
                        session_id: req_session_id.to_string(),
                        thread_id: req_session_id.to_string(),
                        cwd: cwd.to_string(),
                        history,
                        cancel_token: None,
                        frozen_system_prompt: None,
                        frozen_claude_md: None,
                        frozen_claude_local_md: None,
                        frozen_skill_summary: None,
                        frozen_date: None,
                    },
                );
            }

            let modes = build_mode_state(&cfg.permission_mode);
            let models = {
                let p = cfg.provider.read();
                let c = cfg.peri_config.read();
                build_model_state(&p, &c)
            };
            let config_options = {
                let c = cfg.peri_config.read();
                let p = cfg.provider.read();
                build_config_options(&c, &p, cfg.permission_mode.load())
            };
            let resp = LoadSessionResponse::new()
                .modes(modes)
                .models(models)
                .config_options(config_options);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/list" => {
            let threads = cfg
                .thread_store
                .list_threads()
                .await
                .map_err(|e| AcpError::new(-32603, format!("Failed to list sessions: {e}")))?;

            let cwd_filter = params.get("cwd").and_then(|v| v.as_str());

            let entries: Vec<SessionInfo> = threads
                .into_iter()
                .filter(|t| {
                    if let Some(cwd) = cwd_filter {
                        t.cwd == cwd
                    } else {
                        true
                    }
                })
                .map(|t| {
                    SessionInfo::new(
                        SessionId::new(t.id.clone()),
                        std::path::PathBuf::from(t.cwd.clone()),
                    )
                    .title(t.title.clone())
                    .updated_at(t.updated_at.to_rfc3339())
                })
                .collect();

            let resp = ListSessionsResponse::new(entries);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/close" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;

            if let Some(state) = sessions.remove(req_session_id) {
                if let Some(ref token) = state.cancel_token {
                    token.cancel();
                }
                info!(session_id = %req_session_id, "Session closed");
            }
            let resp = CloseSessionResponse::new();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/resume" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

            if !sessions.contains_key(req_session_id) {
                sessions.insert(
                    req_session_id.to_string(),
                    SessionState {
                        session_id: req_session_id.to_string(),
                        thread_id: req_session_id.to_string(),
                        cwd: cwd.to_string(),
                        history: Vec::new(),
                        cancel_token: None,
                        frozen_system_prompt: None,
                        frozen_claude_md: None,
                        frozen_claude_local_md: None,
                        frozen_skill_summary: None,
                        frozen_date: None,
                    },
                );
                info!(session_id = %req_session_id, "Session resumed (new)");
            } else {
                info!(session_id = %req_session_id, "Session resumed (existing)");
            }

            let resp = ResumeSessionResponse::new();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/fork" => {
            let source_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

            let source_history = sessions
                .get(source_id)
                .map(|s| s.history.clone())
                .ok_or_else(|| {
                    AcpError::new(-32602, format!("source session not found: {source_id}"))
                })?;

            let meta = ThreadMeta::new(cwd);
            let new_thread_id = cfg
                .thread_store
                .create_thread(meta)
                .await
                .map_err(|e| AcpError::new(-32603, format!("Thread creation failed: {e}")))?;

            if !source_history.is_empty() {
                if let Err(e) = cfg
                    .thread_store
                    .append_messages(&new_thread_id, &source_history)
                    .await
                {
                    tracing::warn!(error = %e, "session/fork: failed to copy messages to new thread");
                }
            }

            let new_session_id = new_thread_id.clone();
            sessions.insert(
                new_session_id.clone(),
                SessionState {
                    session_id: new_session_id.clone(),
                    thread_id: new_thread_id.clone(),
                    cwd: cwd.to_string(),
                    history: source_history,
                    cancel_token: None,
                    frozen_system_prompt: None,
                    frozen_claude_md: None,
                    frozen_claude_local_md: None,
                    frozen_skill_summary: None,
                    frozen_date: None,
                },
            );

            info!(source = %source_id, new = %new_session_id, "Session forked");
            let resp = ForkSessionResponse::new(SessionId::new(new_session_id));
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        _ => Err(AcpError::new(-32601, format!("Method not found: {method}"))),
    }
}
