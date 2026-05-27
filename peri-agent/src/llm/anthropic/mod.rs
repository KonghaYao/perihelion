mod cache;
mod invoke;
mod stream;

/// Build a reqwest client with connection pool limits to prevent TLS session
/// accumulation. Default pool is unbounded — each idle connection holds
/// ~50-100 KB of TLS state that is never released.
fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// ChatAnthropic - Anthropic Messages API 实现
pub struct ChatAnthropic {
    pub api_key: String,
    pub model: String,
    pub extended_thinking: bool,
    pub thinking_budget: u32,
    /// 思考强度 "low" / "medium" / "high"（output_config.effort）
    pub thinking_effort: String,
    /// 是否开启 Prompt Caching（anthropic-beta: prompt-caching-2024-07-31），默认开启
    pub enable_cache: bool,
    /// 自定义 base URL（代理场景），不含末尾 /
    pub base_url: Option<String>,
    /// 最大输出 token 数，默认 32000
    pub max_tokens: u32,
    client: reqwest::Client,
}

impl ChatAnthropic {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            extended_thinking: false,
            thinking_budget: 10000,
            thinking_effort: "medium".to_string(),
            enable_cache: true,
            base_url: None,
            max_tokens: 32000,
            client: build_reqwest_client(),
        }
    }

    /// 设置自定义 base URL（用于代理或兼容 API）
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        self.base_url = if url.is_empty() { None } else { Some(url) };
        self
    }

    /// 开启 Extended Thinking（claude-3-7-sonnet 及以上）
    pub fn with_extended_thinking(mut self, budget_tokens: u32, effort: impl Into<String>) -> Self {
        self.extended_thinking = true;
        self.thinking_budget = budget_tokens;
        self.thinking_effort = effort.into();
        self
    }

    /// 关闭 Prompt Caching
    pub fn without_cache(mut self) -> Self {
        self.enable_cache = false;
        self
    }

    /// 设置最大输出 token 数
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        let model = std::env::var("ANTHROPIC_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
        let mut s = Self::new(api_key, model);
        if let Ok(url) = std::env::var("ANTHROPIC_BASE_URL") {
            s = s.with_base_url(url);
        }
        Some(s)
    }

    // ─── Thin wrappers（兼容测试文件中的关联函数调用语法）───

    #[cfg(test)]
    fn messages_to_anthropic(
        messages: &[crate::messages::BaseMessage],
    ) -> (Vec<serde_json::Value>, Vec<cache::SystemPromptBlock>) {
        invoke::messages_to_anthropic(messages)
    }

    #[cfg(test)]
    fn parse_content_blocks(
        raw_blocks: &[serde_json::Value],
    ) -> (
        Vec<crate::messages::ContentBlock>,
        Vec<crate::messages::ToolCallRequest>,
    ) {
        invoke::parse_content_blocks(raw_blocks)
    }

    #[cfg(test)]
    fn build_system_blocks_json(blocks: &[cache::SystemPromptBlock]) -> Vec<serde_json::Value> {
        invoke::build_system_blocks_json(blocks)
    }
}

#[cfg(test)]
#[path = "../anthropic_test.rs"]
mod tests;
