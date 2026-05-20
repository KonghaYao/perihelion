//! Session lifecycle management.
//!
//! Manages ACP session creation, loading, resumption, and closure.
//! Each session owns a ThreadStore entry, an Agent instance, and associated state.

pub mod event_sink;
pub mod executor;
pub mod state_builders;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use agent_client_protocol::schema::RequestId;
use chrono::Utc;
use dashmap::DashMap;
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadId, ThreadMeta, ThreadStore};
use peri_middlewares::agent_define::AgentOverrides;
use peri_middlewares::prelude::{PermissionMode, SharedPermissionMode};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::provider::config::{PeriConfig, ThinkingConfig};
use crate::provider::LlmProvider;

/// Entry in the pending requests table, keyed by JSON-RPC request ID.
#[derive(Debug)]
pub struct PendingRequestEntry {
    /// Channel to signal cancellation to the request awaiter.
    pub cancel_tx: oneshot::Sender<()>,
    /// Monotonic generation counter for this session's pending requests.
    pub generation: u64,
}

pub struct AcpSession {
    pub session_id: String,
    pub thread_id: ThreadId,
    pub cwd: String,
    pub cancel_token: CancellationToken,
    pub state_messages: Vec<BaseMessage>,
    pub created_at: chrono::DateTime<Utc>,
    /// 当前激活的 provider ID（对应 PeriConfig.config.providers 中的 id）
    pub provider_id: String,
    /// 当前激活的模型别名（"opus"/"sonnet"/"haiku"）
    pub model_alias: String,
    /// 每会话独立的权限模式
    pub permission_mode: Arc<SharedPermissionMode>,
    /// 每会话独立的 thinking 配置
    pub thinking: Option<ThinkingConfig>,
    /// Pending JSON-RPC requests that can be cancelled via $/cancel_request.
    /// Keyed by the request's `RequestId`.
    pub pending_requests: DashMap<RequestId, PendingRequestEntry>,
    /// Monotonic counter for assigning generations to pending requests.
    pub pending_gen: AtomicU64,
}

struct SessionManagerInner {
    sessions: DashMap<String, AcpSession>,
    thread_store: Arc<dyn ThreadStore>,
    provider: LlmProvider,
    peri_config: Arc<PeriConfig>,
    permission_mode: Arc<SharedPermissionMode>,
    /// Global agent overrides from CLI --agent flag (applied to all sessions)
    pub agent_overrides: Option<AgentOverrides>,
}

#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<SessionManagerInner>,
}

impl SessionManager {
    pub fn new(
        thread_store: Arc<dyn ThreadStore>,
        provider: LlmProvider,
        peri_config: Arc<PeriConfig>,
        permission_mode: Arc<SharedPermissionMode>,
        agent_overrides: Option<AgentOverrides>,
    ) -> Self {
        Self {
            inner: Arc::new(SessionManagerInner {
                sessions: DashMap::new(),
                thread_store,
                provider,
                peri_config,
                permission_mode,
                agent_overrides,
            }),
        }
    }

    /// 使用指定 session_id 创建会话（用于 session/load 和 session/resume）
    pub async fn new_session_with_id(&self, session_id: &str, cwd: &str) -> anyhow::Result<()> {
        if self.inner.sessions.contains_key(session_id) {
            return Ok(());
        }

        let thread_id = ThreadId::from(session_id.to_string());
        let session = self.build_session(session_id, thread_id, cwd);

        self.inner.sessions.insert(session_id.to_string(), session);
        Ok(())
    }

    pub async fn new_session(&self, cwd: &str) -> anyhow::Result<(String, ThreadId)> {
        let meta = ThreadMeta::new(cwd);
        let thread_id = self.inner.thread_store.create_thread(meta).await?;

        let session_id = thread_id.clone();

        let session = self.build_session(&session_id, thread_id.clone(), cwd);

        self.inner.sessions.insert(session_id.clone(), session);
        Ok((session_id, thread_id))
    }

    /// 创建新会话并继承指定的 provider_id、model_alias 和 thinking 设置
    pub async fn new_session_with_settings(
        &self,
        cwd: &str,
        provider_id: String,
        model_alias: String,
        thinking: Option<ThinkingConfig>,
    ) -> anyhow::Result<(String, ThreadId)> {
        let meta = ThreadMeta::new(cwd);
        let thread_id = self.inner.thread_store.create_thread(meta).await?;

        let session_id = thread_id.clone();

        let session = AcpSession {
            session_id: session_id.clone(),
            thread_id: thread_id.clone(),
            cwd: cwd.to_string(),
            cancel_token: CancellationToken::new(),
            state_messages: Vec::new(),
            created_at: Utc::now(),
            provider_id,
            model_alias,
            permission_mode: SharedPermissionMode::new(PermissionMode::AutoMode),
            thinking,
            pending_requests: DashMap::new(),
            pending_gen: AtomicU64::new(0),
        };

        self.inner.sessions.insert(session_id.clone(), session);
        Ok((session_id, thread_id))
    }

    fn build_session(&self, session_id: &str, thread_id: ThreadId, cwd: &str) -> AcpSession {
        AcpSession {
            session_id: session_id.to_string(),
            thread_id,
            cwd: cwd.to_string(),
            cancel_token: CancellationToken::new(),
            state_messages: Vec::new(),
            created_at: Utc::now(),
            provider_id: self.inner.peri_config.config.active_provider_id.clone(),
            model_alias: self.inner.peri_config.config.active_alias.clone(),
            permission_mode: SharedPermissionMode::new(PermissionMode::AutoMode),
            thinking: self.inner.peri_config.config.thinking.clone(),
            pending_requests: DashMap::new(),
            pending_gen: AtomicU64::new(0),
        }
    }

    pub async fn close_session(&self, session_id: &str) -> anyhow::Result<()> {
        if let Some((_, session)) = self.inner.sessions.remove(session_id) {
            session.cancel_token.cancel();
        }
        Ok(())
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<ThreadMeta>> {
        self.inner.thread_store.list_threads().await
    }

    pub fn get_session(
        &self,
        session_id: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, AcpSession>> {
        self.inner.sessions.get(session_id)
    }

    pub fn inner_sessions(&self) -> &DashMap<String, AcpSession> {
        &self.inner.sessions
    }

    pub fn cancel_session(&self, session_id: &str) {
        if let Some(mut session) = self.inner.sessions.get_mut(session_id) {
            // Cancel the current token so all clones (held by link tasks,
            // permission loops) detect cancellation. Then replace with a fresh
            // token so subsequent prompts on the same session are not affected.
            // CancellationToken has no reset() — once cancelled it stays cancelled.
            session.cancel_token.cancel();
            session.cancel_token = CancellationToken::new();

            // Cancel all pending per-request operations so the
            // tokio::select! in the permission forwarding loop unblocks.
            let keys: Vec<RequestId> = session
                .pending_requests
                .iter()
                .map(|e| e.key().clone())
                .collect();
            for key in keys {
                if let Some((_, entry)) = session.pending_requests.remove(&key) {
                    let _ = entry.cancel_tx.send(());
                }
            }
        }
    }

    /// Handle a $/cancel_request notification by looking up the target
    /// request ID and cancelling it. Returns the session_id if found.
    pub fn cancel_pending_request(&self, request_id: &RequestId) -> Option<String> {
        for entry in self.inner.sessions.iter() {
            if let Some((_, pending)) = entry.pending_requests.remove(request_id) {
                let _ = pending.cancel_tx.send(());
                tracing::info!(
                    session_id = %entry.key(),
                    ?request_id,
                    generation = pending.generation,
                    "ACP per-request cancellation applied"
                );
                return Some(entry.key().clone());
            }
        }
        tracing::warn!(?request_id, "$/cancel_request: unknown request ID");
        None
    }

    pub fn provider(&self) -> &LlmProvider {
        &self.inner.provider
    }

    pub fn peri_config(&self) -> &Arc<PeriConfig> {
        &self.inner.peri_config
    }

    pub fn permission_mode(&self) -> &Arc<SharedPermissionMode> {
        &self.inner.permission_mode
    }

    pub fn thread_store(&self) -> &Arc<dyn ThreadStore> {
        &self.inner.thread_store
    }

    pub fn agent_overrides(&self) -> Option<&AgentOverrides> {
        self.inner.agent_overrides.as_ref()
    }

    pub async fn load_thread_messages(
        &self,
        thread_id: &ThreadId,
    ) -> anyhow::Result<Vec<BaseMessage>> {
        self.inner.thread_store.load_messages(thread_id).await
    }
}
