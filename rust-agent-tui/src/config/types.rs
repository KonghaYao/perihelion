use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// 顶层包装（与 ~/.zen-code/settings.json 的 { "config": {...} } 对应）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ZenConfig {
    #[serde(default)]
    pub config: AppConfig,
}

/// Provider 内的三级别模型名映射
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderModels {
    #[serde(default)]
    pub opus: String,
    #[serde(default)]
    pub sonnet: String,
    #[serde(default)]
    pub haiku: String,
}

impl ProviderModels {
    /// 按 alias 名（大小写不敏感）获取对应模型名
    pub fn get_model(&self, alias: &str) -> Option<&str> {
        match alias.to_lowercase().as_str() {
            "opus" => Some(&self.opus),
            "sonnet" => Some(&self.sonnet),
            "haiku" => Some(&self.haiku),
            _ => None,
        }
    }
}

fn default_alias() -> String {
    "opus".to_string()
}

/// Thinking / 推理模式配置
///
/// 对两个 provider 的映射：
/// - Anthropic → `extended_thinking` + `budget_tokens`
/// - OpenAI    → `reasoning_effort`（"low"/"medium"/"high"，由 budget_tokens 区段决定）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThinkingConfig {
    /// 是否启用 thinking
    #[serde(default)]
    pub enabled: bool,
    /// 推理 token 预算（Anthropic 直接使用；OpenAI 按区段转换为 effort 等级）
    /// - OpenAI 映射：0 = "low", 1-7999 = "medium", ≥8000 = "high"
    /// - Anthropic：直接传给 extended_thinking.budget_tokens，最小值 1024（由 with_extended_thinking 保证）
    #[serde(default = "default_budget_tokens")]
    pub budget_tokens: u32,
}

fn default_budget_tokens() -> u32 {
    8000
}

impl ThinkingConfig {
    /// 将 budget_tokens 映射到 OpenAI reasoning_effort 字符串
    pub fn openai_effort(&self) -> &'static str {
        match self.budget_tokens {
            0 => "low",
            1..=7999 => "medium",
            _ => "high",
        }
    }
}

/// 应用配置（只映射用到的字段，其余字段用 extra 保留）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 当前激活的模型别名（"opus" | "sonnet" | "haiku"）
    #[serde(default = "default_alias")]
    pub active_alias: String,
    /// 当前激活的 provider ID（直接指向 providers 列表中的某个 Provider）
    #[serde(default)]
    pub active_provider_id: String,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// 全局 skills 目录路径
    #[serde(default, alias = "skillsDir")]
    pub skills_dir: Option<String>,
    /// Thinking / 推理模式配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// 环境变量注入（扁平键值对）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Compact 系统配置（缺失时使用 CompactConfig::default()）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<rust_create_agent::agent::compact::CompactConfig>,
    /// 保留未知字段，写回时不丢失
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 单个 Provider 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    #[serde(default)]
    pub id: String,
    /// "openai" | "anthropic" 等
    #[serde(rename = "type", default)]
    pub provider_type: String,
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub models: ProviderModels,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ProviderConfig {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ThinkingConfig::openai_effort ─────────────────────────────────────────

    #[test]
    fn test_thinking_effort_low() {
        let c = ThinkingConfig { enabled: true, budget_tokens: 0 };
        assert_eq!(c.openai_effort(), "low");
    }

    #[test]
    fn test_thinking_effort_medium_boundary() {
        let c1 = ThinkingConfig { enabled: true, budget_tokens: 1 };
        let c2 = ThinkingConfig { enabled: true, budget_tokens: 7999 };
        assert_eq!(c1.openai_effort(), "medium");
        assert_eq!(c2.openai_effort(), "medium");
    }

    #[test]
    fn test_thinking_effort_high() {
        let c = ThinkingConfig { enabled: true, budget_tokens: 8000 };
        assert_eq!(c.openai_effort(), "high");
        let c2 = ThinkingConfig { enabled: true, budget_tokens: 100_000 };
        assert_eq!(c2.openai_effort(), "high");
    }

    // ── ThinkingConfig 序列化 / 反序列化 ─────────────────────────────────────

    #[test]
    fn test_thinking_config_serde_roundtrip() {
        let cfg = ThinkingConfig { enabled: true, budget_tokens: 5000 };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ThinkingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.enabled, true);
        assert_eq!(back.budget_tokens, 5000);
    }

    #[test]
    fn test_thinking_config_default_budget() {
        // 不传 budget_tokens 时应默认 8000
        let json = r#"{"enabled": false}"#;
        let cfg: ThinkingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.budget_tokens, 8000);
    }

    #[test]
    fn test_app_config_thinking_optional() {
        // thinking 字段缺失时应为 None（使用新格式字段）
        let json = r#"{"active_alias": "opus", "active_provider_id": "", "providers": []}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.thinking.is_none());
    }

    #[test]
    fn test_app_config_thinking_roundtrip() {
        let json = r#"{
            "active_alias": "opus",
            "providers": [],
            "thinking": {"enabled": true, "budget_tokens": 8000}
        }"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        let t = cfg.thinking.as_ref().unwrap();
        assert_eq!(t.enabled, true);
        assert_eq!(t.budget_tokens, 8000);

        // 序列化后 thinking 字段存在
        let out = serde_json::to_string(&cfg).unwrap();
        assert!(out.contains("\"thinking\""));
        // active_alias 字段正确序列化
        assert!(out.contains("\"active_alias\""));
    }

    #[test]
    fn test_app_config_thinking_skip_when_none() {
        let cfg = AppConfig::default(); // thinking = None
        let out = serde_json::to_string(&cfg).unwrap();
        // skip_serializing_if = "Option::is_none"，所以 thinking 字段不应出现
        assert!(!out.contains("thinking"), "thinking should be absent when None");
    }

    // ── ModelPanel thinking 缓冲逻辑（已迁移至 model_panel.rs）─────────────────

    // ── ProviderModels 测试 ───────────────────────────────────────────────────

    #[test]
    fn test_provider_models_get_model_known_aliases() {
        let models = ProviderModels { opus: "o".to_string(), sonnet: "s".to_string(), haiku: "h".to_string() };
        assert_eq!(models.get_model("opus"), Some("o"));
        assert_eq!(models.get_model("sonnet"), Some("s"));
        assert_eq!(models.get_model("haiku"), Some("h"));
    }

    #[test]
    fn test_provider_models_get_model_case_insensitive() {
        let models = ProviderModels { opus: "o".to_string(), sonnet: "s".to_string(), haiku: "h".to_string() };
        assert_eq!(models.get_model("Opus"), Some("o"));
        assert_eq!(models.get_model("SONNET"), Some("s"));
        assert_eq!(models.get_model("Haiku"), Some("h"));
    }

    #[test]
    fn test_provider_models_get_model_unknown_returns_none() {
        let models = ProviderModels { opus: "o".to_string(), sonnet: "s".to_string(), haiku: "h".to_string() };
        assert_eq!(models.get_model("turbo"), None);
    }

    #[test]
    fn test_provider_models_default() {
        let models = ProviderModels::default();
        assert!(models.opus.is_empty());
        assert!(models.sonnet.is_empty());
        assert!(models.haiku.is_empty());
    }

    #[test]
    fn test_provider_config_models_serde_roundtrip() {
        let p = ProviderConfig {
            id: "test".to_string(),
            provider_type: "anthropic".to_string(),
            api_key: "key".to_string(),
            base_url: String::new(),
            name: Some("Test".to_string()),
            models: ProviderModels {
                opus: "claude-opus-4-7".to_string(),
                sonnet: "claude-sonnet-4-6".to_string(),
                haiku: "claude-haiku-4-5".to_string(),
            },
            extra: Default::default(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.models.opus, "claude-opus-4-7");
        assert_eq!(back.models.sonnet, "claude-sonnet-4-6");
        assert_eq!(back.models.haiku, "claude-haiku-4-5");
    }

    #[test]
    fn test_app_config_active_provider_id_serde() {
        let json = r#"{"active_alias": "opus", "active_provider_id": "anthropic", "providers": []}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.active_provider_id, "anthropic");
    }

    #[test]
    fn test_app_config_old_fields_ignored() {
        let json = r#"{"provider_id": "old", "model_id": "old-model", "model_aliases": {"opus": {"provider_id": "x", "model_id": "y"}}, "providers": []}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        // 旧字段被 extra 吸收，active_provider_id 为默认空字符串
        assert_eq!(cfg.active_provider_id, "");
    }

    // ── AppConfig env 字段测试 ─────────────────────────────────────────────────

    #[test]
    fn test_app_config_env_serde_roundtrip() {
        let mut env = std::collections::HashMap::new();
        env.insert("ANTHROPIC_API_KEY".to_string(), "sk-ant-123".to_string());
        env.insert("RUST_LOG".to_string(), "debug".to_string());

        let cfg = AppConfig {
            env: Some(env),
            ..Default::default()
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();

        assert!(back.env.is_some());
        let env_back = back.env.unwrap();
        assert_eq!(env_back.get("ANTHROPIC_API_KEY"), Some(&"sk-ant-123".to_string()));
        assert_eq!(env_back.get("RUST_LOG"), Some(&"debug".to_string()));
    }

    #[test]
    fn test_app_config_env_optional() {
        // env 字段缺失时应为 None
        let json = r#"{"active_alias": "opus", "providers": []}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.env.is_none());
    }

    #[test]
    fn test_app_config_env_skip_when_none() {
        let cfg = AppConfig::default(); // env = None
        let out = serde_json::to_string(&cfg).unwrap();
        // skip_serializing_if = "Option::is_none"，所以 env 字段不应出现
        assert!(!out.contains("env"), "env should be absent when None");
    }

    // ── AppConfig compact 字段测试 ─────────────────────────────────────────────

    #[test]
    fn test_app_config_compact_serde_roundtrip() {
        let compact = rust_create_agent::agent::compact::CompactConfig {
            auto_compact_enabled: false,
            auto_compact_threshold: 0.9,
            ..Default::default()
        };
        let cfg = AppConfig {
            compact: Some(compact),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        let c = back.compact.unwrap();
        assert!(!c.auto_compact_enabled);
        assert!((c.auto_compact_threshold - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_app_config_compact_none_when_absent() {
        let json = r#"{"active_alias": "opus", "providers": []}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.compact.is_none());
    }

    #[test]
    fn test_app_config_compact_skip_when_none() {
        let cfg = AppConfig::default();
        let out = serde_json::to_string(&cfg).unwrap();
        assert!(!out.contains("compact"), "compact should be absent when None");
    }
}
