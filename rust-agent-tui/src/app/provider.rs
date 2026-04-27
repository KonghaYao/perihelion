use rust_create_agent::llm::{BaseModel, ChatAnthropic, ChatOpenAI};
use crate::config::{ThinkingConfig, ZenConfig};

#[derive(Clone)]
pub enum LlmProvider {
    OpenAi {
        api_key: String,
        base_url: String,
        model: String,
        thinking: Option<ThinkingConfig>,
    },
    Anthropic {
        api_key: String,
        model: String,
        base_url: Option<String>,
        thinking: Option<ThinkingConfig>,
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
                Some(Self::Anthropic { api_key, model, base_url, thinking: None })
            }
            "openai" | "" => {
                if provider_hint.is_empty() {
                    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
                        let model = std::env::var("ANTHROPIC_MODEL")
                            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
                        let base_url = std::env::var("ANTHROPIC_BASE_URL").ok();
                        return Some(Self::Anthropic { api_key, model, base_url, thinking: None });
                    }
                }
                let api_key = std::env::var("OPENAI_API_KEY").ok()?;
                let base_url = std::env::var("OPENAI_API_BASE")
                    .or_else(|_| std::env::var("OPENAI_BASE_URL"))
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
                let model = std::env::var("OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-4o".to_string());
                Some(Self::OpenAi { api_key, base_url, model, thinking: None })
            }
            _ => {
                let api_key = std::env::var("OPENAI_API_KEY").ok()?;
                let base_url = std::env::var("OPENAI_API_BASE")
                    .or_else(|_| std::env::var("OPENAI_BASE_URL"))
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
                let model = std::env::var("OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-4o".to_string());
                Some(Self::OpenAi { api_key, base_url, model, thinking: None })
            }
        }
    }

    /// 从 ZenConfig 构造 LlmProvider（按 active_provider_id 查找 Provider，再按 active_alias 取模型名）
    pub fn from_config(cfg: &ZenConfig) -> Option<Self> {
        let app = &cfg.config;
        let provider = app.providers.iter().find(|p| p.id == app.active_provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let alias = app.active_alias.as_str();
        let model = provider.models.get_model(alias)
            .filter(|m| !m.is_empty())
            .map(|m| m.to_string())
            .unwrap_or_else(|| {
                match provider.provider_type.as_str() {
                    "anthropic" => "claude-sonnet-4-6".to_string(),
                    _ => "gpt-4o".to_string(),
                }
            });

        let thinking = app.thinking.clone().filter(|t| t.enabled);

        match provider.provider_type.as_str() {
            "anthropic" => Some(Self::Anthropic {
                api_key: provider.api_key.clone(),
                model,
                base_url: if provider.base_url.is_empty() { None } else { Some(provider.base_url.clone()) },
                thinking,
            }),
            _ => Some(Self::OpenAi {
                api_key: provider.api_key.clone(),
                base_url: if provider.base_url.is_empty() {
                    "https://api.openai.com/v1".to_string()
                } else {
                    provider.base_url.clone()
                },
                model,
                thinking,
            }),
        }
    }

    /// 从 ZenConfig 按指定 alias（如 "haiku"/"sonnet"/"opus"）构造 LlmProvider
    /// 大小写不敏感；未知 alias fallback 到默认模型
    pub fn from_config_for_alias(cfg: &ZenConfig, alias: &str) -> Option<Self> {
        let app = &cfg.config;
        let provider = app.providers.iter().find(|p| p.id == app.active_provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let model = provider.models.get_model(alias)
            .filter(|m| !m.is_empty())
            .map(|m| m.to_string())
            .unwrap_or_else(|| {
                match provider.provider_type.as_str() {
                    "anthropic" => "claude-sonnet-4-6".to_string(),
                    _ => "gpt-4o".to_string(),
                }
            });

        let thinking = app.thinking.clone().filter(|t| t.enabled);

        match provider.provider_type.as_str() {
            "anthropic" => Some(Self::Anthropic {
                api_key: provider.api_key.clone(),
                model,
                base_url: if provider.base_url.is_empty() { None } else { Some(provider.base_url.clone()) },
                thinking,
            }),
            _ => Some(Self::OpenAi {
                api_key: provider.api_key.clone(),
                base_url: if provider.base_url.is_empty() {
                    "https://api.openai.com/v1".to_string()
                } else {
                    provider.base_url.clone()
                },
                model,
                thinking,
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
            Self::OpenAi { api_key, base_url, model, thinking } => {
                let mut m = ChatOpenAI::new(api_key, model).with_base_url(base_url);
                if let Some(t) = thinking {
                    m = m.with_reasoning_effort(t.openai_effort());
                }
                Box::new(m)
            }
            Self::Anthropic { api_key, model, base_url, thinking } => {
                let mut m = ChatAnthropic::new(api_key, model);
                if let Some(url) = base_url {
                    m = m.with_base_url(url);
                }
                if let Some(t) = thinking {
                    m = m.with_extended_thinking(t.budget_tokens);
                }
                Box::new(m)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderModels, ZenConfig};

    fn make_config(alias: &str, provider_id: &str, model_id: &str, provider_type: &str) -> ZenConfig {
        let mut cfg = ZenConfig::default();
        cfg.config.active_alias = alias.to_string();
        cfg.config.active_provider_id = provider_id.to_string();
        cfg.config.providers.push(ProviderConfig {
            id: provider_id.to_string(),
            provider_type: provider_type.to_string(),
            api_key: "test-key".to_string(),
            models: ProviderModels {
                opus: if alias == "opus" { model_id.to_string() } else { String::new() },
                sonnet: if alias == "sonnet" { model_id.to_string() } else { String::new() },
                haiku: if alias == "haiku" { model_id.to_string() } else { String::new() },
            },
            ..Default::default()
        });
        cfg
    }

    #[test]
    fn test_from_config_opus_alias() {
        let cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
        let provider = LlmProvider::from_config(&cfg).expect("应成功解析");
        assert_eq!(provider.model_name(), "claude-opus-4-6");
    }

    #[test]
    fn test_from_config_sonnet_alias() {
        let cfg = make_config("sonnet", "openrouter", "gpt-5.4", "openai");
        let provider = LlmProvider::from_config(&cfg).expect("应成功解析");
        assert_eq!(provider.model_name(), "gpt-5.4");
    }

    #[test]
    fn test_from_config_empty_model_fallback_anthropic() {
        let cfg = make_config("opus", "anthropic", "", "anthropic");
        let provider = LlmProvider::from_config(&cfg).expect("空 model 不应 panic");
        assert_eq!(provider.model_name(), "claude-sonnet-4-6");
    }

    #[test]
    fn test_from_config_empty_model_fallback_openai() {
        let cfg = make_config("haiku", "openai", "", "openai");
        let provider = LlmProvider::from_config(&cfg).expect("空 model openai 不应 panic");
        assert_eq!(provider.model_name(), "gpt-4o");
    }

    #[test]
    fn test_from_config_unknown_alias_fallback() {
        let mut cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
        cfg.config.active_alias = "ultra".to_string();
        let provider = LlmProvider::from_config(&cfg).expect("未知别名应 fallback");
        assert_eq!(provider.model_name(), "claude-sonnet-4-6");
    }

    #[test]
    fn test_from_config_empty_api_key_returns_none() {
        let mut cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
        cfg.config.providers[0].api_key = String::new();
        let result = LlmProvider::from_config(&cfg);
        assert!(result.is_none(), "空 api_key 应返回 None");
    }

    #[test]
    fn test_from_config_provider_not_found_returns_none() {
        let mut cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
        cfg.config.active_provider_id = "nonexistent".to_string();
        let result = LlmProvider::from_config(&cfg);
        assert!(result.is_none(), "不存在的 provider 应返回 None");
    }

    // ── from_config_for_alias 测试 ─────────────────────────────────────────────

    #[test]
    fn test_from_config_for_alias_known() {
        let cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "opus").unwrap();
        assert_eq!(p.model_name(), "claude-opus-4-6");

        let cfg = make_config("sonnet", "openrouter", "gpt-5.4", "openai");
        let p = LlmProvider::from_config_for_alias(&cfg, "sonnet").unwrap();
        assert_eq!(p.model_name(), "gpt-5.4");

        let cfg = make_config("haiku", "anthropic", "claude-haiku-4", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "haiku").unwrap();
        assert_eq!(p.model_name(), "claude-haiku-4");
    }

    #[test]
    fn test_from_config_for_alias_unknown_returns_fallback() {
        let cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "turbo").unwrap();
        assert_eq!(p.model_name(), "claude-sonnet-4-6");
    }

    #[test]
    fn test_from_config_for_alias_case_insensitive() {
        let cfg = make_config("haiku", "anthropic", "claude-haiku-4", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "Haiku").unwrap();
        assert_eq!(p.model_name(), "claude-haiku-4");
        let p2 = LlmProvider::from_config_for_alias(&cfg, "HAIKU").unwrap();
        assert_eq!(p2.model_name(), "claude-haiku-4");
    }
}
