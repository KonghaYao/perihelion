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

    /// 从 ZenConfig 构造 LlmProvider（按 active_alias 查 model_aliases 表）
    pub fn from_config(cfg: &ZenConfig) -> Option<Self> {
        let app = &cfg.config;
        let alias = app.active_alias.as_str();
        let mapping = match alias {
            "opus"   => &app.model_aliases.opus,
            "sonnet" => &app.model_aliases.sonnet,
            "haiku"  => &app.model_aliases.haiku,
            _        => &app.model_aliases.opus,  // 未知别名 fallback
        };

        let provider = app.providers.iter().find(|p| p.id == mapping.provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let model = if !mapping.model_id.is_empty() {
            mapping.model_id.clone()
        } else {
            match provider.provider_type.as_str() {
                "anthropic" => "claude-sonnet-4-6".to_string(),
                _ => "gpt-4o".to_string(),
            }
        };

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
    /// 大小写不敏感；未知 alias 返回 None
    pub fn from_config_for_alias(cfg: &ZenConfig, alias: &str) -> Option<Self> {
        let app = &cfg.config;
        let mapping = app.model_aliases.get_alias(alias)?;

        let provider = app.providers.iter().find(|p| p.id == mapping.provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let model = if !mapping.model_id.is_empty() {
            mapping.model_id.clone()
        } else {
            match provider.provider_type.as_str() {
                "anthropic" => "claude-sonnet-4-6".to_string(),
                _ => "gpt-4o".to_string(),
            }
        };

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
    use crate::config::{ModelAliasConfig, ProviderConfig, ZenConfig};

    fn make_config_with_alias(alias: &str, provider_id: &str, model_id: &str, provider_type: &str) -> ZenConfig {
        let mut cfg = ZenConfig::default();
        cfg.config.active_alias = alias.to_string();
        cfg.config.providers.push(ProviderConfig {
            id: provider_id.to_string(),
            provider_type: provider_type.to_string(),
            api_key: "test-key".to_string(),
            ..Default::default()
        });
        let alias_cfg = ModelAliasConfig {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
        };
        match alias {
            "opus"   => cfg.config.model_aliases.opus = alias_cfg,
            "sonnet" => cfg.config.model_aliases.sonnet = alias_cfg,
            "haiku"  => cfg.config.model_aliases.haiku = alias_cfg,
            _ => {}
        }
        cfg
    }

    #[test]
    fn test_from_config_opus_alias() {
        let cfg = make_config_with_alias("opus", "anthropic", "claude-opus-4-6", "anthropic");
        let provider = LlmProvider::from_config(&cfg).expect("应成功解析");
        assert_eq!(provider.model_name(), "claude-opus-4-6");
    }

    #[test]
    fn test_from_config_sonnet_alias() {
        let cfg = make_config_with_alias("sonnet", "openrouter", "gpt-5.4", "openai");
        let provider = LlmProvider::from_config(&cfg).expect("应成功解析");
        assert_eq!(provider.model_name(), "gpt-5.4");
    }

    #[test]
    fn test_provider_default() {
        // 空 model_id 时回退到默认 model，不 panic
        let cfg = make_config_with_alias("opus", "anthropic", "", "anthropic");
        let provider = LlmProvider::from_config(&cfg).expect("空 model_id 不应 panic");
        assert_eq!(provider.model_name(), "claude-sonnet-4-6");
    }

    #[test]
    fn test_provider_default_openai() {
        let cfg = make_config_with_alias("haiku", "openai", "", "openai");
        let provider = LlmProvider::from_config(&cfg).expect("空 model_id openai 不应 panic");
        assert_eq!(provider.model_name(), "gpt-4o");
    }

    #[test]
    fn test_from_config_unknown_alias_fallback_to_opus() {
        // 未知 alias 应 fallback 到 opus
        let mut cfg = make_config_with_alias("opus", "anthropic", "claude-opus-4-6", "anthropic");
        cfg.config.active_alias = "ultra".to_string(); // 未知别名
        let provider = LlmProvider::from_config(&cfg).expect("未知别名应 fallback 到 opus");
        assert_eq!(provider.model_name(), "claude-opus-4-6");
    }

    #[test]
    fn test_from_config_empty_api_key_returns_none() {
        let mut cfg = make_config_with_alias("opus", "anthropic", "claude-opus-4-6", "anthropic");
        cfg.config.providers[0].api_key = String::new();
        let result = LlmProvider::from_config(&cfg);
        assert!(result.is_none(), "空 api_key 应返回 None");
    }

    // ── from_config_for_alias 测试 ─────────────────────────────────────────────

    #[test]
    fn test_from_config_for_alias_known_aliases() {
        // opus
        let cfg = make_config_with_alias("opus", "anthropic", "claude-opus-4-6", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "opus").unwrap();
        assert_eq!(p.model_name(), "claude-opus-4-6");

        // sonnet
        let cfg = make_config_with_alias("sonnet", "openrouter", "gpt-5.4", "openai");
        let p = LlmProvider::from_config_for_alias(&cfg, "sonnet").unwrap();
        assert_eq!(p.model_name(), "gpt-5.4");

        // haiku
        let cfg = make_config_with_alias("haiku", "anthropic", "claude-haiku-4", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "haiku").unwrap();
        assert_eq!(p.model_name(), "claude-haiku-4");
    }

    #[test]
    fn test_from_config_for_alias_unknown_returns_none() {
        let cfg = make_config_with_alias("opus", "anthropic", "claude-opus-4-6", "anthropic");
        let result = LlmProvider::from_config_for_alias(&cfg, "turbo");
        assert!(result.is_none(), "未知 alias 应返回 None");
    }

    #[test]
    fn test_from_config_for_alias_empty_api_key_returns_none() {
        let mut cfg = make_config_with_alias("haiku", "anthropic", "claude-haiku-4", "anthropic");
        cfg.config.providers[0].api_key = String::new();
        let result = LlmProvider::from_config_for_alias(&cfg, "haiku");
        assert!(result.is_none(), "空 api_key 应返回 None");
    }

    #[test]
    fn test_from_config_for_alias_case_insensitive() {
        let cfg = make_config_with_alias("haiku", "anthropic", "claude-haiku-4", "anthropic");
        let p = LlmProvider::from_config_for_alias(&cfg, "Haiku").unwrap();
        assert_eq!(p.model_name(), "claude-haiku-4");
        let p2 = LlmProvider::from_config_for_alias(&cfg, "HAIKU").unwrap();
        assert_eq!(p2.model_name(), "claude-haiku-4");
    }
}
