/// 向导步骤
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SetupStep {
    /// Provider + API Key 合并表单
    Provider,
    /// 模型别名配置
    ModelAlias,
    /// 确认完成
    Done,
}

/// Provider 类型选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProviderType {
    Anthropic,
    OpenAiCompatible,
}

impl ProviderType {
    pub fn label(&self) -> &str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::OpenAiCompatible => "OpenAI Compatible",
        }
    }

    pub fn cycle(&mut self) {
        *self = match self {
            Self::Anthropic => Self::OpenAiCompatible,
            Self::OpenAiCompatible => Self::Anthropic,
        };
    }

    /// 根据类型返回默认 Provider ID
    pub fn default_provider_id(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAiCompatible => "openai",
        }
    }

    /// 根据类型返回默认 Base URL
    pub fn default_base_url(&self) -> &str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenAiCompatible => "https://api.openai.com/v1",
        }
    }

    /// 三个别名级别的默认模型 ID
    pub fn default_model_ids(&self) -> [&str; 3] {
        match self {
            Self::Anthropic => [
                "claude-opus-4-0-20250514",
                "claude-sonnet-4-6-20250514",
                "claude-haiku-3-5-20241022",
            ],
            Self::OpenAiCompatible => ["o3", "gpt-4o", "gpt-4o-mini"],
        }
    }
}

/// 单个别名的配置
#[derive(Debug, Clone)]
pub struct AliasConfig {
    pub model_id: String,
}

/// Setup Wizard 全屏面板状态
pub struct SetupWizardPanel {
    pub step: SetupStep,
    /// Step 1: Provider 选择
    pub provider_type: ProviderType,
    pub provider_id: String,
    pub base_url: String,
    pub step1_focus: Step1Field,
    /// Step 2: API Key
    pub api_key: String,
    /// Step 3: 模型别名
    pub aliases: [AliasConfig; 3],
    pub step3_focus: usize,
    /// 是否正在显示跳过确认
    pub confirm_skip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step1Field {
    ProviderType,
    ProviderId,
    BaseUrl,
    ApiKey,
}

impl Step1Field {
    pub fn next(&self) -> Self {
        match self {
            Self::ProviderType => Self::ProviderId,
            Self::ProviderId => Self::BaseUrl,
            Self::BaseUrl => Self::ApiKey,
            Self::ApiKey => Self::ProviderType,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::ProviderType => Self::ApiKey,
            Self::ProviderId => Self::ProviderType,
            Self::BaseUrl => Self::ProviderId,
            Self::ApiKey => Self::BaseUrl,
        }
    }
}

impl SetupWizardPanel {
    pub fn new() -> Self {
        let pt = ProviderType::Anthropic;
        Self {
            step: SetupStep::Provider,
            provider_type: pt,
            provider_id: pt.default_provider_id().to_string(),
            base_url: pt.default_base_url().to_string(),
            step1_focus: Step1Field::ProviderType,
            api_key: String::new(),
            aliases: pt
                .default_model_ids()
                .map(|s| AliasConfig { model_id: s.to_string() }),
            step3_focus: 0,
            confirm_skip: false,
        }
    }

    /// 粘贴文本到当前聚焦的字段
    pub fn paste_text(&mut self, text: &str) {
        let text = text.replace('\n', "");
        match self.step {
            SetupStep::Provider => match self.step1_focus {
                Step1Field::ProviderId => self.provider_id.push_str(&text),
                Step1Field::BaseUrl => self.base_url.push_str(&text),
                Step1Field::ApiKey => self.api_key.push_str(&text),
                _ => {}
            },
            SetupStep::ModelAlias => {
                if self.step3_focus < 3 {
                    self.aliases[self.step3_focus].model_id.push_str(&text);
                }
            }
            SetupStep::Done => {}
        }
    }

    /// 切换 Provider 类型后刷新默认值
    pub fn refresh_provider_defaults(&mut self) {
        self.provider_id = self.provider_type.default_provider_id().to_string();
        self.base_url = self.provider_type.default_base_url().to_string();
        self.aliases = self
            .provider_type
            .default_model_ids()
            .map(|s| AliasConfig { model_id: s.to_string() });
    }
}

/// 检测配置是否需要 Setup 向导
/// 条件 1：providers 列表为空
/// 条件 2：有 provider 但 api_key 为空且对应环境变量未设置
pub fn needs_setup(config: &crate::config::types::AppConfig) -> bool {
    // 条件 1：无任何 Provider
    if config.providers.is_empty() {
        return true;
    }
    // 条件 2：有 Provider 但 API Key 缺失
    for provider in &config.providers {
        if provider.api_key.is_empty() {
            let key_env = match provider.provider_type.as_str() {
                "anthropic" => "ANTHROPIC_API_KEY",
                _ => "OPENAI_API_KEY",
            };
            if std::env::var(key_env).unwrap_or_default().is_empty() {
                return true;
            }
        }
    }
    false
}

/// setup_wizard 按键处理的返回动作
pub enum SetupWizardAction {
    /// 无特殊动作，仅重绘
    Redraw,
    /// 保存配置并关闭向导
    SaveAndClose,
    /// 不保存，直接关闭向导（跳过）
    Skip,
}

/// Setup 向导按键分发
pub fn handle_setup_wizard_key(
    wizard: &mut SetupWizardPanel,
    input: ratatui_textarea::Input,
) -> Option<SetupWizardAction> {
    use ratatui_textarea::Key;

    // 跳过确认弹窗优先处理
    if wizard.confirm_skip {
        return handle_confirm_skip(wizard, input);
    }

    match wizard.step {
        SetupStep::Provider => handle_step_provider(wizard, input),
        SetupStep::ModelAlias => handle_step_model_alias(wizard, input),
        SetupStep::Done => handle_step_done(wizard, input),
    }
}

fn handle_confirm_skip(
    wizard: &mut SetupWizardPanel,
    input: ratatui_textarea::Input,
) -> Option<SetupWizardAction> {
    use ratatui_textarea::Key;
    match input {
        ratatui_textarea::Input {
            key: Key::Enter, ..
        } => Some(SetupWizardAction::Skip),
        ratatui_textarea::Input {
            key: Key::Esc, ..
        } => {
            wizard.confirm_skip = false;
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}

fn handle_step_provider(
    wizard: &mut SetupWizardPanel,
    input: ratatui_textarea::Input,
) -> Option<SetupWizardAction> {
    use ratatui_textarea::Key;
    match input {
        // Tab: 在四个字段间循环切换
        ratatui_textarea::Input {
            key: Key::Tab,
            shift: false,
            ..
        } => {
            wizard.step1_focus = wizard.step1_focus.next();
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Tab,
            shift: true,
            ..
        } => {
            wizard.step1_focus = wizard.step1_focus.prev();
            Some(SetupWizardAction::Redraw)
        }
        // ↑↓: 当 focus == ProviderType 时循环切换 Provider 类型
        ratatui_textarea::Input {
            key: Key::Up, ..
        } => {
            if wizard.step1_focus == Step1Field::ProviderType {
                wizard.provider_type.cycle();
                wizard.refresh_provider_defaults();
            }
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Down, ..
        } => {
            if wizard.step1_focus == Step1Field::ProviderType {
                wizard.provider_type.cycle();
                wizard.refresh_provider_defaults();
            }
            Some(SetupWizardAction::Redraw)
        }
        // Enter: 校验所有字段非空后进入 ModelAlias
        ratatui_textarea::Input {
            key: Key::Enter, ..
        } => {
            if !wizard.provider_id.trim().is_empty()
                && !wizard.api_key.trim().is_empty()
            {
                wizard.step = SetupStep::ModelAlias;
            }
            Some(SetupWizardAction::Redraw)
        }
        // Esc: 触发跳过确认
        ratatui_textarea::Input {
            key: Key::Esc, ..
        } => {
            wizard.confirm_skip = true;
            Some(SetupWizardAction::Redraw)
        }
        // Backspace: 删除当前字段末字符
        ratatui_textarea::Input {
            key: Key::Backspace,
            ..
        } => {
            match wizard.step1_focus {
                Step1Field::ProviderId => {
                    wizard.provider_id.pop();
                }
                Step1Field::BaseUrl => {
                    wizard.base_url.pop();
                }
                Step1Field::ApiKey => {
                    wizard.api_key.pop();
                }
                _ => {}
            }
            Some(SetupWizardAction::Redraw)
        }
        // 字符输入
        ratatui_textarea::Input {
            key: Key::Char(c),
            ctrl: false,
            alt: false,
            ..
        } => {
            match wizard.step1_focus {
                Step1Field::ProviderId => wizard.provider_id.push(c),
                Step1Field::BaseUrl => {
                    wizard.base_url.push(c);
                }
                Step1Field::ApiKey => wizard.api_key.push(c),
                _ => {}
            }
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}

fn handle_step_model_alias(
    wizard: &mut SetupWizardPanel,
    input: ratatui_textarea::Input,
) -> Option<SetupWizardAction> {
    use ratatui_textarea::Key;
    match input {
        ratatui_textarea::Input {
            key: Key::Tab,
            shift: false,
            ..
        } => {
            wizard.step3_focus = (wizard.step3_focus + 1) % 3;
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Tab,
            shift: true,
            ..
        } => {
            wizard.step3_focus = (wizard.step3_focus + 2) % 3;
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Enter, ..
        } => {
            if wizard.aliases.iter().all(|a| !a.model_id.trim().is_empty()) {
                wizard.step = SetupStep::Done;
            }
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Esc, ..
        } => {
            wizard.step = SetupStep::Provider;
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Backspace,
            ..
        } => {
            wizard.aliases[wizard.step3_focus].model_id.pop();
            Some(SetupWizardAction::Redraw)
        }
        ratatui_textarea::Input {
            key: Key::Char(c),
            ctrl: false,
            alt: false,
            ..
        } => {
            wizard.aliases[wizard.step3_focus].model_id.push(c);
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}

fn handle_step_done(
    wizard: &mut SetupWizardPanel,
    input: ratatui_textarea::Input,
) -> Option<SetupWizardAction> {
    use ratatui_textarea::Key;
    match input {
        ratatui_textarea::Input {
            key: Key::Enter, ..
        } => Some(SetupWizardAction::SaveAndClose),
        ratatui_textarea::Input {
            key: Key::Esc, ..
        } => {
            wizard.step = SetupStep::ModelAlias;
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}

/// 将 setup wizard 结果写入指定路径
pub fn save_setup_to(
    wizard: &SetupWizardPanel,
    path: &std::path::Path,
) -> anyhow::Result<crate::config::ZenConfig> {
    let mut cfg = crate::config::ZenConfig::default();
    let provider_type_str = match wizard.provider_type {
        ProviderType::Anthropic => "anthropic",
        ProviderType::OpenAiCompatible => "openai",
    };
    let provider = crate::config::types::ProviderConfig {
        id: wizard.provider_id.clone(),
        provider_type: provider_type_str.to_string(),
        api_key: wizard.api_key.clone(),
        base_url: wizard.base_url.clone(),
        ..Default::default()
    };
    cfg.config.providers.push(provider);
    cfg.config.active_alias = "opus".to_string();
    // 设置三个别名
    cfg.config.model_aliases.opus.provider_id = wizard.provider_id.clone();
    cfg.config.model_aliases.opus.model_id = wizard.aliases[0].model_id.clone();
    cfg.config.model_aliases.sonnet.provider_id = wizard.provider_id.clone();
    cfg.config.model_aliases.sonnet.model_id = wizard.aliases[1].model_id.clone();
    cfg.config.model_aliases.haiku.provider_id = wizard.provider_id.clone();
    cfg.config.model_aliases.haiku.model_id = wizard.aliases[2].model_id.clone();

    let content = serde_json::to_string_pretty(&cfg)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(cfg)
}

/// 将 setup wizard 结果写入默认配置路径
pub fn save_setup(wizard: &SetupWizardPanel) -> anyhow::Result<crate::config::ZenConfig> {
    let path = crate::config::store::config_path();
    let cfg = save_setup_to(wizard, &path)?;

    // 如果已有配置文件，合并而非覆盖（保留 extra 字段等）
    if let Ok(existing) = crate::config::load() {
        let mut merged = existing;
        // 确保新的 provider 不重复添加
        let new_provider = &cfg.config.providers[0];
        if !merged.config.providers.iter().any(|p| p.id == new_provider.id) {
            merged.config.providers.push(new_provider.clone());
        }
        merged.config.active_alias = cfg.config.active_alias;
        merged.config.model_aliases = cfg.config.model_aliases;
        crate::config::save(&merged)?;
        return Ok(merged);
    }

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{AppConfig, ProviderConfig};

    #[test]
    fn test_needs_setup_empty_providers() {
        let config = AppConfig::default();
        assert!(needs_setup(&config));
    }

    #[test]
    fn test_needs_setup_empty_api_key_no_env() {
        // Use anthropic provider type; check that needs_setup returns true
        // when api_key is empty and env var is not set
        let mut config = AppConfig::default();
        config.providers.push(ProviderConfig {
            id: "test".to_string(),
            provider_type: "anthropic".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            ..Default::default()
        });
        // Save and remove ANTHROPIC_API_KEY if set
        let saved = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");
        let result = needs_setup(&config);
        // Restore
        if let Some(val) = saved {
            std::env::set_var("ANTHROPIC_API_KEY", val);
        }
        assert!(result);
    }

    #[test]
    fn test_needs_setup_api_key_from_config() {
        let mut config = AppConfig::default();
        config.providers.push(ProviderConfig {
            id: "test".to_string(),
            provider_type: "openai".to_string(),
            api_key: "sk-test".to_string(),
            base_url: String::new(),
            ..Default::default()
        });
        assert!(!needs_setup(&config));
    }

    #[test]
    fn test_needs_setup_api_key_from_env() {
        let mut config = AppConfig::default();
        config.providers.push(ProviderConfig {
            id: "test".to_string(),
            provider_type: "openai".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            ..Default::default()
        });
        // Save and set env var temporarily
        let saved = std::env::var("OPENAI_API_KEY").ok();
        std::env::set_var("OPENAI_API_KEY", "sk-from-env");
        assert!(!needs_setup(&config));
        // Restore
        match saved {
            Some(val) => std::env::set_var("OPENAI_API_KEY", val),
            None => std::env::remove_var("OPENAI_API_KEY"),
        }
    }

    #[test]
    fn test_setup_wizard_new_defaults() {
        let wizard = SetupWizardPanel::new();
        assert_eq!(wizard.step, SetupStep::Provider);
        assert_eq!(wizard.provider_type, ProviderType::Anthropic);
        assert_eq!(wizard.provider_id, "anthropic");
        assert_eq!(wizard.base_url, "https://api.anthropic.com");
        assert!(wizard.api_key.is_empty());
        assert_eq!(wizard.step3_focus, 0);
        assert!(!wizard.confirm_skip);
        assert!(wizard.aliases[0].model_id.contains("claude-opus"));
        assert!(wizard.aliases[1].model_id.contains("claude-sonnet"));
        assert!(wizard.aliases[2].model_id.contains("claude-haiku"));
    }

    #[test]
    fn test_provider_type_cycle() {
        let mut pt = ProviderType::Anthropic;
        assert_eq!(pt, ProviderType::Anthropic);
        pt.cycle();
        assert_eq!(pt, ProviderType::OpenAiCompatible);
        pt.cycle();
        assert_eq!(pt, ProviderType::Anthropic);
    }

    #[test]
    fn test_refresh_provider_defaults() {
        let mut wizard = SetupWizardPanel::new();
        wizard.provider_type = ProviderType::OpenAiCompatible;
        wizard.refresh_provider_defaults();
        assert_eq!(wizard.provider_id, "openai");
        assert_eq!(wizard.base_url, "https://api.openai.com/v1");
        assert_eq!(wizard.aliases[0].model_id, "o3");
        assert_eq!(wizard.aliases[1].model_id, "gpt-4o");
        assert_eq!(wizard.aliases[2].model_id, "gpt-4o-mini");
    }

    #[test]
    fn test_step1_field_navigation() {
        assert_eq!(Step1Field::ProviderType.next(), Step1Field::ProviderId);
        assert_eq!(Step1Field::ProviderId.next(), Step1Field::BaseUrl);
        assert_eq!(Step1Field::BaseUrl.next(), Step1Field::ApiKey);
        assert_eq!(Step1Field::ApiKey.next(), Step1Field::ProviderType);

        assert_eq!(Step1Field::ProviderType.prev(), Step1Field::ApiKey);
        assert_eq!(Step1Field::ProviderId.prev(), Step1Field::ProviderType);
        assert_eq!(Step1Field::BaseUrl.prev(), Step1Field::ProviderId);
        assert_eq!(Step1Field::ApiKey.prev(), Step1Field::BaseUrl);
    }

    // ── Event handling tests ──

    use ratatui_textarea::{Input, Key};

    fn make_char(c: char) -> Input {
        Input { key: Key::Char(c), ctrl: false, alt: false, shift: false }
    }
    fn make_key(key: Key) -> Input {
        Input { key, ctrl: false, alt: false, shift: false }
    }
    fn type_text(wizard: &mut SetupWizardPanel, text: &str) {
        for c in text.chars() {
            let _ = handle_setup_wizard_key(wizard, make_char(c));
        }
    }

    #[test]
    fn test_handle_step_provider_tab_cycles_focus() {
        let mut wizard = SetupWizardPanel::new();
        assert_eq!(wizard.step1_focus, Step1Field::ProviderType);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step1_focus, Step1Field::ProviderId);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step1_focus, Step1Field::BaseUrl);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step1_focus, Step1Field::ApiKey);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step1_focus, Step1Field::ProviderType);
    }

    #[test]
    fn test_handle_step_provider_arrow_cycles_type() {
        let mut wizard = SetupWizardPanel::new();
        assert_eq!(wizard.provider_type, ProviderType::Anthropic);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Down));
        assert_eq!(wizard.provider_type, ProviderType::OpenAiCompatible);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Down));
        assert_eq!(wizard.provider_type, ProviderType::Anthropic);
    }

    #[test]
    fn test_handle_step_provider_enter_advances() {
        let mut wizard = SetupWizardPanel::new();
        assert!(!wizard.provider_id.is_empty());
        // Empty api_key → Enter blocked
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::Provider);
        // Set api_key → Enter advances to ModelAlias
        wizard.api_key = "sk-test".to_string();
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::ModelAlias);
    }

    #[test]
    fn test_handle_step_api_key_in_step1() {
        let mut wizard = SetupWizardPanel::new();
        // Tab to ApiKey field
        wizard.step1_focus = Step1Field::ApiKey;
        type_text(&mut wizard, "sk-test-key");
        assert_eq!(wizard.api_key, "sk-test-key");
        // Backspace
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
        assert_eq!(wizard.api_key, "sk-test-ke");
    }

    #[test]
    fn test_handle_step_model_alias_esc_back() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::ModelAlias;
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
        assert_eq!(wizard.step, SetupStep::Provider);
    }

    #[test]
    fn test_handle_step_model_alias_tab_cycles() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::ModelAlias;
        assert_eq!(wizard.step3_focus, 0);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step3_focus, 1);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step3_focus, 2);
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
        assert_eq!(wizard.step3_focus, 0);
    }

    #[test]
    fn test_handle_step_model_alias_enter_validates_all() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::ModelAlias;
        // All non-empty by default
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::Done);
    }

    #[test]
    fn test_handle_step_model_alias_enter_blocks_empty_model() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::ModelAlias;
        wizard.aliases[0].model_id.clear();
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::ModelAlias);
    }

    #[test]
    fn test_handle_step_done_enter_returns_save() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::Done;
        let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));
    }

    #[test]
    fn test_handle_step_done_esc_back() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::Done;
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
        assert_eq!(wizard.step, SetupStep::ModelAlias);
    }

    #[test]
    fn test_handle_confirm_skip_enter_skip() {
        let mut wizard = SetupWizardPanel::new();
        wizard.confirm_skip = true;
        let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert!(matches!(action, Some(SetupWizardAction::Skip)));
    }

    #[test]
    fn test_handle_confirm_skip_esc_cancel() {
        let mut wizard = SetupWizardPanel::new();
        wizard.confirm_skip = true;
        let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
        assert!(matches!(action, Some(SetupWizardAction::Redraw)));
        assert!(!wizard.confirm_skip);
    }

    #[test]
    fn test_save_setup_creates_valid_config() {
        let mut wizard = SetupWizardPanel::new();
        wizard.api_key = "sk-test-key".to_string();

        let temp_dir = std::env::temp_dir().join(format!("zen-setup-unit-{}", uuid::Uuid::now_v7()));
        let config_path = temp_dir.join("settings.json");
        let cfg = save_setup_to(&wizard, &config_path).expect("save_setup_to should succeed");

        assert_eq!(cfg.config.providers.len(), 1);
        assert_eq!(cfg.config.providers[0].provider_type, "anthropic");
        assert_eq!(cfg.config.providers[0].api_key, "sk-test-key");
        assert_eq!(cfg.config.active_alias, "opus");
        assert_eq!(cfg.config.model_aliases.opus.provider_id, "anthropic");
        assert!(cfg.config.model_aliases.opus.model_id.contains("claude-opus"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
