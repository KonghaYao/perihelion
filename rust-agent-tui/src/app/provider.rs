use rust_create_agent::llm::{BaseModel, ChatAnthropic, ChatOpenAI};
use crate::config::ZenConfig;

#[derive(Clone)]
pub enum LlmProvider {
    OpenAi {
        api_key: String,
        base_url: String,
        model: String,
    },
    Anthropic {
        api_key: String,
        model: String,
        base_url: Option<String>,
    },
}

impl LlmProvider {
    pub fn from_env() -> Option<Self> {
        let provider_hint = std::env::var("MODEL_PROVIDER").unwrap_or_default();

        match provider_hint.to_lowercase().as_str() {
            "anthropic" => {
                let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
                let model = std::env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
                let base_url = std::env::var("ANTHROPIC_BASE_URL").ok();
                Some(Self::Anthropic { api_key, model, base_url })
            }
            "openai" | "" => {
                if provider_hint.is_empty() {
                    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
                        let model = std::env::var("ANTHROPIC_MODEL")
                            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
                        let base_url = std::env::var("ANTHROPIC_BASE_URL").ok();
                        return Some(Self::Anthropic { api_key, model, base_url });
                    }
                }
                let api_key = std::env::var("OPENAI_API_KEY").ok()?;
                let base_url = std::env::var("OPENAI_API_BASE")
                    .or_else(|_| std::env::var("OPENAI_BASE_URL"))
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
                let model = std::env::var("OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-4o".to_string());
                Some(Self::OpenAi { api_key, base_url, model })
            }
            _ => {
                let api_key = std::env::var("OPENAI_API_KEY").ok()?;
                let base_url = std::env::var("OPENAI_API_BASE")
                    .or_else(|_| std::env::var("OPENAI_BASE_URL"))
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
                let model = std::env::var("OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-4o".to_string());
                Some(Self::OpenAi { api_key, base_url, model })
            }
        }
    }

    /// 从 ZenConfig 构造 LlmProvider（按 provider_id 查找对应 ProviderConfig）
    pub fn from_config(cfg: &ZenConfig) -> Option<Self> {
        let app = &cfg.config;
        let provider = app.providers.iter().find(|p| p.id == app.provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let model = if !app.model_id.is_empty() {
            app.model_id.clone()
        } else {
            // fallback：根据 provider 类型给默认值
            match provider.provider_type.as_str() {
                "anthropic" => "claude-sonnet-4-6".to_string(),
                _ => "gpt-4o".to_string(),
            }
        };

        match provider.provider_type.as_str() {
            "anthropic" => Some(Self::Anthropic {
                api_key: provider.api_key.clone(),
                model,
                base_url: if provider.base_url.is_empty() { None } else { Some(provider.base_url.clone()) },
            }),
            _ => Some(Self::OpenAi {
                api_key: provider.api_key.clone(),
                base_url: if provider.base_url.is_empty() {
                    "https://api.openai.com/v1".to_string()
                } else {
                    provider.base_url.clone()
                },
                model,
            }),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::OpenAi { .. } => "OpenAI",
            Self::Anthropic { .. } => "Anthropic",
        }
    }

    pub fn model_name(&self) -> &str {
        match self {
            Self::OpenAi { model, .. } => model,
            Self::Anthropic { model, .. } => model,
        }
    }

    pub fn into_model(self) -> Box<dyn BaseModel> {
        match self {
            Self::OpenAi { api_key, base_url, model } => {
                Box::new(ChatOpenAI::new(api_key, model).with_base_url(base_url))
            }
            Self::Anthropic { api_key, model, base_url } => {
                let mut m = ChatAnthropic::new(api_key, model);
                if let Some(url) = base_url {
                    m = m.with_base_url(url);
                }
                Box::new(m)
            }
        }
    }
}
