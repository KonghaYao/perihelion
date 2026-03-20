use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 顶层包装（与 ~/.zen-code/settings.json 的 { "config": {...} } 对应）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ZenConfig {
    #[serde(default)]
    pub config: AppConfig,
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
    /// 0 = "low", 1-7999 = "medium", ≥8000 = "high"
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
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// 全局 skills 目录路径
    #[serde(default, alias = "skillsDir")]
    pub skills_dir: Option<String>,
    /// Thinking / 推理模式配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
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
        // thinking 字段缺失时应为 None
        let json = r#"{"provider_id": "x", "model_id": "y", "providers": []}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.thinking.is_none());
    }

    #[test]
    fn test_app_config_thinking_roundtrip() {
        let json = r#"{
            "provider_id": "x",
            "model_id": "y",
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
    }

    #[test]
    fn test_app_config_thinking_skip_when_none() {
        let cfg = AppConfig::default(); // thinking = None
        let out = serde_json::to_string(&cfg).unwrap();
        // skip_serializing_if = "Option::is_none"，所以 thinking 字段不应出现
        assert!(!out.contains("thinking"), "thinking should be absent when None");
    }

    // ── ModelPanel thinking 缓冲逻辑 ─────────────────────────────────────────

    #[test]
    fn test_model_panel_from_config_loads_thinking() {
        use crate::app::model_panel::ModelPanel;

        let mut cfg = ZenConfig::default();
        cfg.config.thinking = Some(ThinkingConfig { enabled: true, budget_tokens: 4000 });

        let panel = ModelPanel::from_config(&cfg);
        assert!(panel.buf_thinking_enabled);
        assert_eq!(panel.buf_thinking_budget, "4000");
    }

    #[test]
    fn test_model_panel_from_config_defaults_when_no_thinking() {
        use crate::app::model_panel::ModelPanel;

        let cfg = ZenConfig::default();
        let panel = ModelPanel::from_config(&cfg);
        assert!(!panel.buf_thinking_enabled);
        assert_eq!(panel.buf_thinking_budget, "8000");
    }

    #[test]
    fn test_model_panel_toggle_thinking() {
        use crate::app::model_panel::{EditField, ModelPanel};

        let cfg = ZenConfig::default();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.edit_field = EditField::ThinkingBudget;

        assert!(!panel.buf_thinking_enabled);
        panel.toggle_thinking();
        assert!(panel.buf_thinking_enabled);
        panel.toggle_thinking();
        assert!(!panel.buf_thinking_enabled);
    }

    #[test]
    fn test_model_panel_thinking_budget_input_only_digits() {
        use crate::app::model_panel::{EditField, ModelPanel};

        let cfg = ZenConfig::default();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.edit_field = EditField::ThinkingBudget;
        panel.buf_thinking_budget = String::new();

        panel.push_char('1');
        panel.push_char('a'); // 非数字，应忽略
        panel.push_char('2');
        panel.push_char('0');
        assert_eq!(panel.buf_thinking_budget, "120");

        panel.pop_char();
        assert_eq!(panel.buf_thinking_budget, "12");
    }

    #[test]
    fn test_model_panel_apply_edit_saves_thinking() {
        use crate::app::model_panel::{ModelPanel, ModelPanelMode};

        let mut cfg = ZenConfig::default();
        cfg.config.providers.push(ProviderConfig {
            id: "p1".to_string(),
            provider_type: "openai".to_string(),
            api_key: "key".to_string(),
            ..Default::default()
        });
        cfg.config.provider_id = "p1".to_string();

        let mut panel = ModelPanel::from_config(&cfg);
        panel.mode = ModelPanelMode::Edit;
        panel.buf_thinking_enabled = true;
        panel.buf_thinking_budget = "5000".to_string();

        let ok = panel.apply_edit(&mut cfg);
        assert!(ok);

        let t = cfg.config.thinking.as_ref().unwrap();
        assert_eq!(t.enabled, true);
        assert_eq!(t.budget_tokens, 5000);
    }
}
