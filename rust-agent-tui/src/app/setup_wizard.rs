/// 向导步骤
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SetupStep {
    /// 选择来源
    Choose,
    /// 合并表单：多 Provider + API Key + Model Aliases
    Form,
    /// 确认完成
    Done,
}

/// 配置来源选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SetupSource {
    /// 手动输入 Custom API
    CustomApi,
    /// 从 Claude Code 迁移
    MigrateClaudeCode,
}

impl SetupSource {
    pub const ALL: [Self; 2] = [Self::CustomApi, Self::MigrateClaudeCode];

    pub fn label(&self, lc: &crate::i18n::LcRegistry) -> String {
        match self {
            Self::CustomApi => lc.tr("setup-source-custom-api"),
            Self::MigrateClaudeCode => lc.tr("setup-source-migrate"),
        }
    }

    pub fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        match self {
            Self::CustomApi => lc.tr("setup-source-custom-desc"),
            Self::MigrateClaudeCode => lc.tr("setup-source-migrate-desc"),
        }
    }
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

    pub fn type_str(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAiCompatible => "openai",
        }
    }

    pub fn cycle(&mut self) {
        *self = match self {
            Self::Anthropic => Self::OpenAiCompatible,
            Self::OpenAiCompatible => Self::Anthropic,
        };
    }

    pub fn default_provider_id(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAiCompatible => "openai",
        }
    }

    pub fn default_base_url(&self) -> &str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenAiCompatible => "https://api.openai.com/v1",
        }
    }

    pub fn default_model_ids(&self) -> [&str; 3] {
        match self {
            Self::Anthropic => [
                "claude-opus-4-6",
                "claude-sonnet-4-6",
                "claude-haiku-4-5-20251001",
            ],
            Self::OpenAiCompatible => ["gpt-5.5", "gpt-4o", "gpt-4o-mini"],
        }
    }
}

/// 单个别名的配置
#[derive(Debug, Clone)]
pub struct AliasConfig {
    pub model_id: String,
    pub cursor: usize,
}

/// 单个 Provider 的完整表单数据
#[derive(Debug, Clone)]
pub struct MigratedProvider {
    pub provider_type: ProviderType,
    pub provider_id: String,
    pub cur_provider_id: usize,
    pub base_url: String,
    pub cur_base_url: usize,
    pub api_key: String,
    pub cur_api_key: usize,
    pub aliases: [AliasConfig; 3],
    /// 勾选框状态：是否包含在最终保存中
    pub selected: bool,
}

impl MigratedProvider {
    /// 创建指定类型的默认 provider
    pub fn new(pt: ProviderType) -> Self {
        let pid = pt.default_provider_id().to_string();
        let burl = pt.default_base_url().to_string();
        Self {
            provider_type: pt,
            provider_id: pid.clone(),
            cur_provider_id: pid.chars().count(),
            base_url: burl.clone(),
            cur_base_url: burl.chars().count(),
            api_key: String::new(),
            cur_api_key: 0,
            aliases: pt.default_model_ids().map(|s| AliasConfig {
                model_id: s.to_string(),
                cursor: s.chars().count(),
            }),
            selected: true,
        }
    }

    /// 切换 Provider 类型后刷新默认值（保留 api_key）
    pub fn refresh_provider_defaults(&mut self) {
        self.provider_id = self.provider_type.default_provider_id().to_string();
        self.cur_provider_id = self.provider_id.chars().count();
        self.base_url = self.provider_type.default_base_url().to_string();
        self.cur_base_url = self.base_url.chars().count();
        self.aliases = self.provider_type.default_model_ids().map(|s| AliasConfig {
            model_id: s.to_string(),
            cursor: s.chars().count(),
        });
    }

    /// 字段是否完整（provider_id 和 api_key 非空）
    pub fn is_complete(&self) -> bool {
        !self.provider_id.trim().is_empty()
            && !self.api_key.trim().is_empty()
            && self.aliases.iter().all(|a| !a.model_id.trim().is_empty())
    }
}

/// Form 步骤的模式：浏览列表 vs 编辑详情
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormMode {
    /// 浏览列表：只读摘要，Space 勾选，Enter 进入编辑
    Browse,
    /// 编辑详情：可编辑字段，最后一个 Confirm 返回列表
    Edit,
}

/// 编辑模式下的可聚焦字段
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormField {
    ProviderType,
    ProviderId,
    BaseUrl,
    ApiKey,
    OpusModel,
    SonnetModel,
    HaikuModel,
    Confirm,
}

impl FormField {
    pub fn next(&self) -> Self {
        match self {
            Self::ProviderType => Self::ProviderId,
            Self::ProviderId => Self::BaseUrl,
            Self::BaseUrl => Self::ApiKey,
            Self::ApiKey => Self::OpusModel,
            Self::OpusModel => Self::SonnetModel,
            Self::SonnetModel => Self::HaikuModel,
            Self::HaikuModel => Self::Confirm,
            Self::Confirm => Self::ProviderType,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::ProviderType => Self::Confirm,
            Self::ProviderId => Self::ProviderType,
            Self::BaseUrl => Self::ProviderId,
            Self::ApiKey => Self::BaseUrl,
            Self::OpusModel => Self::ApiKey,
            Self::SonnetModel => Self::OpusModel,
            Self::HaikuModel => Self::SonnetModel,
            Self::Confirm => Self::HaikuModel,
        }
    }

    /// 是否为文本输入字段（可编辑）
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            Self::ProviderId
                | Self::BaseUrl
                | Self::ApiKey
                | Self::OpusModel
                | Self::SonnetModel
                | Self::HaikuModel
        )
    }
}

/// Setup Wizard 全屏面板状态
pub struct SetupWizardPanel {
    pub step: SetupStep,
    /// Step 1: 来源选择
    pub source: SetupSource,
    pub choose_cursor: usize,
    /// Step 2: 多 provider 列表
    pub providers: Vec<MigratedProvider>,
    /// 当前聚焦的 provider 索引（Edit 模式下使用）
    pub active_provider: usize,
    /// Form 步骤模式
    pub form_mode: FormMode,
    /// Browse 模式下的光标（0..providers.len()=providers, providers.len()=Submit）
    pub browse_cursor: usize,
    /// Edit 模式下的聚焦字段
    pub form_focus: FormField,
}

impl Default for SetupWizardPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SetupWizardPanel {
    pub fn new() -> Self {
        Self {
            step: SetupStep::Choose,
            source: SetupSource::CustomApi,
            choose_cursor: 0,
            providers: vec![MigratedProvider::new(ProviderType::Anthropic)],
            active_provider: 0,
            form_mode: FormMode::Browse,
            browse_cursor: 0,
            form_focus: FormField::ProviderType,
        }
    }

    /// 粘贴文本到当前聚焦的字段
    pub fn paste_text(&mut self, text: &str) {
        let text = text.replace('\n', "");
        if self.step != SetupStep::Form || self.form_mode != FormMode::Edit {
            return;
        }
        let mp = match self.providers.get_mut(self.active_provider) {
            Some(p) => p,
            None => return,
        };
        match self.form_focus {
            FormField::ProviderId => {
                insert_at_cursor(&mut mp.provider_id, &mut mp.cur_provider_id, &text);
            }
            FormField::BaseUrl => {
                insert_at_cursor(&mut mp.base_url, &mut mp.cur_base_url, &text);
            }
            FormField::ApiKey => {
                insert_at_cursor(&mut mp.api_key, &mut mp.cur_api_key, &text);
            }
            FormField::OpusModel => {
                insert_at_cursor(
                    &mut mp.aliases[0].model_id,
                    &mut mp.aliases[0].cursor,
                    &text,
                );
            }
            FormField::SonnetModel => {
                insert_at_cursor(
                    &mut mp.aliases[1].model_id,
                    &mut mp.aliases[1].cursor,
                    &text,
                );
            }
            FormField::HaikuModel => {
                insert_at_cursor(
                    &mut mp.aliases[2].model_id,
                    &mut mp.aliases[2].cursor,
                    &text,
                );
            }
            _ => {}
        }
    }

    /// 从 Claude Code 配置迁移，生成多 provider 列表
    ///
    /// 读取 `~/.claude/settings.json` 的 `env` 字段，按前缀检测凭据：
    /// - `ANTHROPIC_` → Anthropic provider
    /// - `OPENAI_` / `CODEX_` → OpenAI Compatible provider
    ///
    /// 同步字段：API_KEY、BASE_URL、DEFAULT_OPUS/SONNET/HAIKU_MODEL
    pub fn migrate_from_claude_code(&mut self) -> bool {
        let claude_dir = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        let settings_path = claude_dir.join("settings.json");
        if !settings_path.exists() {
            return false;
        }
        let content = match std::fs::read_to_string(&settings_path) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let val: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let env = match val.get("env").and_then(|e| e.as_object()) {
            Some(e) => e,
            None => return false,
        };

        let mut detected: Vec<MigratedProvider> = Vec::new();

        // 定义要检测的前缀及其对应的 provider 类型和默认 provider id
        let prefixes: &[(&str, ProviderType, &str, &[&str])] = &[
            (
                "ANTHROPIC",
                ProviderType::Anthropic,
                "anthropic",
                &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
            ),
            (
                "OPENAI",
                ProviderType::OpenAiCompatible,
                "openai",
                &["OPENAI_API_KEY"],
            ),
        ];

        for &(prefix, pt, default_id, key_names) in prefixes {
            // 按优先级尝试多个 key 名
            let api_key = key_names
                .iter()
                .map(|k| env_get(env, k))
                .find(|v| !v.is_empty())
                .unwrap_or_default();
            let base_url = env_get(env, &format!("{}_BASE_URL", prefix));
            let opus = env_get(env, &format!("{}_DEFAULT_OPUS_MODEL", prefix));
            let sonnet = env_get(env, &format!("{}_DEFAULT_SONNET_MODEL", prefix));
            let haiku = env_get(env, &format!("{}_DEFAULT_HAIKU_MODEL", prefix));

            // 至少有 API key 或 base_url 才生成条目
            if api_key.is_empty() && base_url.is_empty() {
                continue;
            }

            let mut mp = MigratedProvider::new(pt);
            mp.provider_id = default_id.to_string();
            mp.cur_provider_id = default_id.chars().count();

            if !api_key.is_empty() {
                mp.cur_api_key = api_key.chars().count();
                mp.api_key = api_key;
            } else {
                // 无 API key → 默认不选中
                mp.selected = false;
            }

            if !base_url.is_empty() {
                mp.base_url = base_url;
                mp.cur_base_url = mp.base_url.chars().count();
            }

            if !opus.is_empty() {
                mp.aliases[0] = AliasConfig {
                    model_id: opus,
                    cursor: 0,
                };
                mp.aliases[0].cursor = mp.aliases[0].model_id.chars().count();
            }
            if !sonnet.is_empty() {
                mp.aliases[1] = AliasConfig {
                    model_id: sonnet,
                    cursor: 0,
                };
                mp.aliases[1].cursor = mp.aliases[1].model_id.chars().count();
            }
            if !haiku.is_empty() {
                mp.aliases[2] = AliasConfig {
                    model_id: haiku,
                    cursor: 0,
                };
                mp.aliases[2].cursor = mp.aliases[2].model_id.chars().count();
            }

            detected.push(mp);
        }

        if detected.is_empty() {
            return false;
        }

        self.providers = detected;
        self.active_provider = 0;
        self.form_mode = FormMode::Browse;
        self.browse_cursor = 0;
        self.form_focus = FormField::ProviderType;
        true
    }
}

/// 从 env JSON 对象中读取字符串值，不存在或非字符串返回空串
fn env_get(env: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    env.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 在光标位置插入字符串并移动光标
fn insert_at_cursor(buf: &mut String, cursor: &mut usize, text: &str) {
    if *cursor > buf.len() {
        *cursor = buf.len();
    }
    let byte_pos = buf
        .char_indices()
        .nth(*cursor)
        .map(|(i, _)| i)
        .unwrap_or(buf.len());
    buf.insert_str(byte_pos, text);
    *cursor += text.chars().count();
}

/// 检测配置是否需要 Setup 向导
pub fn needs_setup(config: &crate::config::types::AppConfig) -> bool {
    if config.providers.is_empty() {
        return true;
    }
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
    Redraw,
    SaveAndClose,
    Skip,
}

/// Setup 向导按键分发
pub fn handle_setup_wizard_key(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    match wizard.step {
        SetupStep::Choose => handle_step_choose(wizard, input),
        SetupStep::Form => handle_step_form(wizard, input),
        SetupStep::Done => handle_step_done(wizard, input),
    }
}

fn handle_step_choose(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    use tui_textarea::Key;
    match input {
        tui_textarea::Input { key: Key::Up, .. } => {
            wizard.choose_cursor =
                (wizard.choose_cursor + SetupSource::ALL.len() - 1) % SetupSource::ALL.len();
            wizard.source = SetupSource::ALL[wizard.choose_cursor];
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input { key: Key::Down, .. } => {
            wizard.choose_cursor = (wizard.choose_cursor + 1) % SetupSource::ALL.len();
            wizard.source = SetupSource::ALL[wizard.choose_cursor];
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input {
            key: Key::Enter, ..
        }
        | tui_textarea::Input {
            key: Key::Char(' '),
            ..
        } => {
            if wizard.source == SetupSource::MigrateClaudeCode {
                if !wizard.migrate_from_claude_code() {
                    // 迁移失败（无可迁移数据），回退到 CustomApi
                    wizard.source = SetupSource::CustomApi;
                    wizard.choose_cursor = 0;
                    return Some(SetupWizardAction::Redraw);
                }
            } else {
                // CustomApi：确保只有一个默认空 provider
                wizard.providers = vec![MigratedProvider::new(ProviderType::Anthropic)];
                wizard.active_provider = 0;
            }
            wizard.step = SetupStep::Form;
            wizard.form_mode = FormMode::Browse;
            wizard.browse_cursor = 0;
            wizard.form_focus = FormField::ProviderType;
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input { key: Key::Esc, .. } => Some(SetupWizardAction::Skip),
        _ => None,
    }
}

fn handle_step_form(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    match wizard.form_mode {
        FormMode::Browse => handle_browse(wizard, input),
        FormMode::Edit => handle_edit(wizard, input),
    }
}

/// Browse 模式：只读列表，Space 勾选，Enter 进入编辑或提交
fn handle_browse(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    use tui_textarea::Key;
    let max_pos = wizard.providers.len(); // Submit 在最后
    match input {
        tui_textarea::Input { key: Key::Up, .. } => {
            if wizard.browse_cursor > 0 {
                wizard.browse_cursor -= 1;
            }
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input { key: Key::Down, .. } => {
            if wizard.browse_cursor < max_pos {
                wizard.browse_cursor += 1;
            }
            Some(SetupWizardAction::Redraw)
        }
        // Space: 勾选/取消勾选
        tui_textarea::Input {
            key: Key::Char(' '),
            ..
        } => {
            if wizard.browse_cursor < wizard.providers.len() {
                let mp = &mut wizard.providers[wizard.browse_cursor];
                mp.selected = !mp.selected;
                Some(SetupWizardAction::Redraw)
            } else {
                None
            }
        }
        // Enter: 进入编辑或提交
        tui_textarea::Input {
            key: Key::Enter, ..
        } => {
            if wizard.browse_cursor < wizard.providers.len() {
                // 进入编辑模式
                wizard.active_provider = wizard.browse_cursor;
                wizard.form_mode = FormMode::Edit;
                wizard.form_focus = FormField::ProviderType;
                Some(SetupWizardAction::Redraw)
            } else {
                // Submit：验证并进入 Done
                let has_valid = wizard
                    .providers
                    .iter()
                    .any(|p| p.selected && p.is_complete());
                if has_valid {
                    wizard.step = SetupStep::Done;
                }
                Some(SetupWizardAction::Redraw)
            }
        }
        // Esc: 返回 Choose
        tui_textarea::Input { key: Key::Esc, .. } => {
            wizard.step = SetupStep::Choose;
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}

/// Edit 模式：编辑字段，Confirm 返回 Browse
fn handle_edit(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    use tui_textarea::Key;
    match input {
        tui_textarea::Input { key: Key::Up, .. } => {
            wizard.form_focus = wizard.form_focus.prev();
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input { key: Key::Down, .. } => {
            wizard.form_focus = wizard.form_focus.next();
            Some(SetupWizardAction::Redraw)
        }
        // ←/→: ProviderType 切换或文本光标移动
        tui_textarea::Input { key: Key::Left, .. }
        | tui_textarea::Input {
            key: Key::Right, ..
        } => {
            if wizard.form_focus == FormField::ProviderType {
                let mp = &mut wizard.providers[wizard.active_provider];
                mp.provider_type.cycle();
                mp.refresh_provider_defaults();
                Some(SetupWizardAction::Redraw)
            } else if wizard.form_focus.is_text_input() {
                let mp = &mut wizard.providers[wizard.active_provider];
                let (buf, cursor) = provider_field_buf(mp, wizard.form_focus)?;
                if crate::app::handle_edit_key(buf, cursor, input) {
                    Some(SetupWizardAction::Redraw)
                } else {
                    None
                }
            } else {
                None
            }
        }
        // Space: ProviderType 切换或文本输入
        tui_textarea::Input {
            key: Key::Char(' '),
            ..
        } => {
            if wizard.form_focus == FormField::ProviderType {
                let mp = &mut wizard.providers[wizard.active_provider];
                mp.provider_type.cycle();
                mp.refresh_provider_defaults();
                Some(SetupWizardAction::Redraw)
            } else if wizard.form_focus.is_text_input() {
                let mp = &mut wizard.providers[wizard.active_provider];
                let (buf, cursor) = provider_field_buf(mp, wizard.form_focus)?;
                if crate::app::handle_edit_key(buf, cursor, input) {
                    Some(SetupWizardAction::Redraw)
                } else {
                    None
                }
            } else {
                None
            }
        }
        // Enter: Confirm 返回 Browse，其他字段无操作
        tui_textarea::Input {
            key: Key::Enter, ..
        } => {
            if wizard.form_focus == FormField::Confirm {
                wizard.form_mode = FormMode::Browse;
                Some(SetupWizardAction::Redraw)
            } else {
                None
            }
        }
        // Esc: 返回 Browse
        tui_textarea::Input { key: Key::Esc, .. } => {
            wizard.form_mode = FormMode::Browse;
            Some(SetupWizardAction::Redraw)
        }
        // 编辑按键
        _ => {
            if !wizard.form_focus.is_text_input() {
                return None;
            }
            let mp = &mut wizard.providers[wizard.active_provider];
            let (buf, cursor) = match provider_field_buf(mp, wizard.form_focus) {
                Some(pair) => pair,
                None => return None,
            };
            if crate::app::handle_edit_key(buf, cursor, input) {
                Some(SetupWizardAction::Redraw)
            } else {
                None
            }
        }
    }
}

/// 获取 provider 指定字段的可变引用
fn provider_field_buf(
    mp: &mut MigratedProvider,
    field: FormField,
) -> Option<(&mut String, &mut usize)> {
    match field {
        FormField::ProviderId => Some((&mut mp.provider_id, &mut mp.cur_provider_id)),
        FormField::BaseUrl => Some((&mut mp.base_url, &mut mp.cur_base_url)),
        FormField::ApiKey => Some((&mut mp.api_key, &mut mp.cur_api_key)),
        FormField::OpusModel => Some((&mut mp.aliases[0].model_id, &mut mp.aliases[0].cursor)),
        FormField::SonnetModel => Some((&mut mp.aliases[1].model_id, &mut mp.aliases[1].cursor)),
        FormField::HaikuModel => Some((&mut mp.aliases[2].model_id, &mut mp.aliases[2].cursor)),
        _ => None,
    }
}

fn handle_step_done(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    use tui_textarea::Key;
    match input {
        tui_textarea::Input {
            key: Key::Enter, ..
        } => Some(SetupWizardAction::SaveAndClose),
        tui_textarea::Input { key: Key::Esc, .. } => {
            wizard.step = SetupStep::Form;
            wizard.form_mode = FormMode::Browse;
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}

/// 将 setup wizard 结果写入指定路径
pub fn save_setup_to(
    wizard: &SetupWizardPanel,
    path: &std::path::Path,
) -> anyhow::Result<crate::config::PeriConfig> {
    let mut cfg = crate::config::PeriConfig::default();
    let mut first_id = String::new();

    for mp in &wizard.providers {
        if !mp.selected {
            continue;
        }
        if mp.provider_id.trim().is_empty() || mp.api_key.trim().is_empty() {
            continue;
        }
        let provider = crate::config::types::ProviderConfig {
            id: mp.provider_id.clone(),
            provider_type: mp.provider_type.type_str().to_string(),
            api_key: mp.api_key.clone(),
            base_url: mp.base_url.clone(),
            models: crate::config::types::ProviderModels {
                opus: mp.aliases[0].model_id.clone(),
                sonnet: mp.aliases[1].model_id.clone(),
                haiku: mp.aliases[2].model_id.clone(),
            },
            ..Default::default()
        };
        if first_id.is_empty() {
            first_id = provider.id.clone();
        }
        cfg.config.providers.push(provider);
    }

    if !first_id.is_empty() {
        cfg.config.active_alias = "opus".to_string();
        cfg.config.active_provider_id = first_id;
    }

    let content = serde_json::to_string_pretty(&cfg)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(cfg)
}

/// 将 setup wizard 结果写入默认配置路径
pub fn save_setup(wizard: &SetupWizardPanel) -> anyhow::Result<crate::config::PeriConfig> {
    let path = crate::config::store::config_path();
    let cfg = save_setup_to(wizard, &path)?;

    if let Ok(existing) = crate::config::load() {
        let mut merged = existing;
        for new_provider in &cfg.config.providers {
            if !merged
                .config
                .providers
                .iter()
                .any(|p| p.id == new_provider.id)
            {
                merged.config.providers.push(new_provider.clone());
            }
        }
        merged.config.active_alias = cfg.config.active_alias;
        merged.config.active_provider_id = cfg.config.active_provider_id;
        crate::config::save(&merged)?;
        return Ok(merged);
    }

    Ok(cfg)
}

#[cfg(test)]
#[path = "setup_wizard_test.rs"]
mod tests;
