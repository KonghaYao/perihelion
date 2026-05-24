//! Session-scoped agent component pool for reusing heavy objects across prompts.
//!
//! The biggest allocation win: `reqwest::Client` inside each LLM instance
//! is ~1-2 MB (connection pool + TLS session cache). Caching these across
//! prompts eliminates ~2-4 MB of transient allocation per turn.

use std::sync::Arc;

use peri_agent::llm::BaseModel;

use crate::provider::LlmProvider;

/// Session-scoped cached LLM instances.
///
/// Contains `reqwest::Client` with connection pool + TLS session cache.
/// Reusing across prompts eliminates transient per-turn allocations.
#[derive(Clone)]
pub struct CachedLlmInstances {
    /// compact_model LLM (used by CompactMiddleware for full compact).
    /// Contains reqwest Client with connection pool.
    pub compact_model: Arc<dyn BaseModel>,
    /// auto_classifier LLM (used by HITL HumanInTheLoopMiddleware).
    /// Contains a second reqwest Client.
    pub auto_classifier_model: Arc<tokio::sync::Mutex<Box<dyn BaseModel>>>,
    /// Provider fingerprint at time of creation (`"provider_name:model_name"`).
    pub fingerprint: String,
}

/// Session-scoped agent component pool.
///
/// Populated on first prompt, reused on subsequent prompts.
/// Invalidated when provider changes (model switch via `session/set_model`).
pub struct AgentPool {
    /// Cached LLM instances (biggest allocation win).
    cached_llm: Option<CachedLlmInstances>,
    /// Provider fingerprint for invalidation detection.
    fingerprint: String,
}

impl Default for AgentPool {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPool {
    pub fn new() -> Self {
        Self {
            cached_llm: None,
            fingerprint: String::new(),
        }
    }

    /// Whether the cached LLM instances are valid for this provider.
    pub fn has_valid_cache(&self, provider: &LlmProvider) -> bool {
        let fp = fingerprint(provider);
        self.cached_llm.is_some() && self.fingerprint == fp
    }

    /// Store LLM instances after building.
    pub fn store_llm(&mut self, instances: CachedLlmInstances) {
        self.fingerprint = instances.fingerprint.clone();
        self.cached_llm = Some(instances);
    }

    /// Get cached LLM instances (returns `None` if cache empty or invalid).
    pub fn get_cached_llm(&self) -> Option<&CachedLlmInstances> {
        self.cached_llm.as_ref()
    }

    /// Invalidate cache (on model change, session clear, etc.).
    pub fn invalidate(&mut self) {
        self.cached_llm = None;
        self.fingerprint.clear();
    }

    /// Current fingerprint (empty if no cache).
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

fn fingerprint(provider: &LlmProvider) -> String {
    format!("{}:{}", provider.display_name(), provider.model_name())
}

#[cfg(test)]
#[path = "agent_pool_test.rs"]
mod tests;
