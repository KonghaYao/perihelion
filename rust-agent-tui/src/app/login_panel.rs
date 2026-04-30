use crate::config::{ProviderConfig, ProviderModels, ZenConfig};

// ─── 默认模型名常量表 ─────────────────────────────────────────────────────────

/// (provider_type, opus, sonnet, haiku)
const DEFAULT_MODELS: &[(&str, &str, &str, &str)] = &[
    (
        "anthropic",
        "claude-opus-4-7",
        "claude-sonnet-4-6",
        "claude-haiku-4-5",
    ),
    ("openai", "gpt-4o", "gpt-4o-mini", "gpt-3.5-turbo"),
];

/// provider_type 循环切换列表
const PROVIDER_TYPES: &[&str] = &["openai", "anthropic"];

// ─── 枚举 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LoginPanelMode {
    Browse,
    Edit,
    New,
    ConfirmDelete,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginEditField {
    Name,
    Type,
    BaseUrl,
    ApiKey,
    OpusModel,
    SonnetModel,
    HaikuModel,
}

impl LoginEditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Name => Self::Type,
            Self::Type => Self::BaseUrl,
            Self::BaseUrl => Self::ApiKey,
            Self::ApiKey => Self::OpusModel,
            Self::OpusModel => Self::SonnetModel,
            Self::SonnetModel => Self::HaikuModel,
            Self::HaikuModel => Self::Name,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Name => Self::HaikuModel,
            Self::Type => Self::Name,
            Self::BaseUrl => Self::Type,
            Self::ApiKey => Self::BaseUrl,
            Self::OpusModel => Self::ApiKey,
            Self::SonnetModel => Self::OpusModel,
            Self::HaikuModel => Self::SonnetModel,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Name => "Name        ",
            Self::Type => "Type        ",
            Self::BaseUrl => "Base URL    ",
            Self::ApiKey => "API Key     ",
            Self::OpusModel => "Opus Model  ",
            Self::SonnetModel => "Sonnet Model",
            Self::HaikuModel => "Haiku Model ",
        }
    }
}

// ─── LoginPanel ─────────────────────────────────────────────────────────────────

pub struct LoginPanel {
    /// provider 列表快照（从 ZenConfig 获取）
    pub providers: Vec<ProviderConfig>,
    /// 当前模式
    pub mode: LoginPanelMode,
    /// 光标位置（Browse 模式下标记选中行）
    pub cursor: usize,
    /// 正在编辑的字段（Edit/New 模式下）
    pub edit_field: LoginEditField,
    /// 编辑缓冲区
    pub buf_name: String,
    pub buf_type: String,
    pub buf_base_url: String,
    pub buf_api_key: String,
    pub buf_opus_model: String,
    pub buf_sonnet_model: String,
    pub buf_haiku_model: String,
    /// 内容滚动偏移
    pub scroll_offset: u16,
}

impl LoginPanel {
    /// 从 ZenConfig 初始化面板（Browse 模式，光标定位到 active_provider_id 对应的 Provider）
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let providers = cfg.config.providers.clone();
        let cursor = providers
            .iter()
            .position(|p| p.id == cfg.config.active_provider_id)
            .unwrap_or(0);
        Self {
            providers,
            mode: LoginPanelMode::Browse,
            cursor,
            edit_field: LoginEditField::Name,
            buf_name: String::new(),
            buf_type: String::new(),
            buf_base_url: String::new(),
            buf_api_key: String::new(),
            buf_opus_model: String::new(),
            buf_sonnet_model: String::new(),
            buf_haiku_model: String::new(),
            scroll_offset: 0,
        }
    }

    // ── Browse 模式操作 ──────────────────────────────────────────────────────

    /// 列表上下移动光标（循环）
    pub fn move_cursor(&mut self, delta: isize) {
        if self.providers.is_empty() {
            return;
        }
        let len = self.providers.len();
        self.cursor = ((self.cursor as isize + delta).rem_euclid(len as isize)) as usize;
    }

    /// 进入编辑模式（编辑光标处的 provider）
    pub fn enter_edit(&mut self) {
        if let Some(p) = self.providers.get(self.cursor) {
            self.buf_name = p.display_name().to_string();
            self.buf_type = p.provider_type.clone();
            self.buf_base_url = p.base_url.clone();
            self.buf_api_key = p.api_key.clone();
            self.buf_opus_model = p.models.opus.clone();
            self.buf_sonnet_model = p.models.sonnet.clone();
            self.buf_haiku_model = p.models.haiku.clone();
            self.edit_field = LoginEditField::Name;
            self.mode = LoginPanelMode::Edit;
        }
    }

    /// 进入新建模式（清空所有缓冲，type 默认 "openai"，模型名按 type 自动填充）
    pub fn enter_new(&mut self) {
        self.buf_name = String::new();
        self.buf_type = "openai".to_string();
        self.buf_base_url = String::new();
        self.buf_api_key = String::new();
        self.buf_opus_model = String::new();
        self.buf_sonnet_model = String::new();
        self.buf_haiku_model = String::new();
        self.auto_fill_models_for_type();
        self.edit_field = LoginEditField::Name;
        self.mode = LoginPanelMode::New;
    }

    /// 进入删除确认模式
    #[allow(dead_code)]
    pub fn request_delete(&mut self) {
        if !self.providers.is_empty() {
            self.mode = LoginPanelMode::ConfirmDelete;
        }
    }

    /// 选中（激活）光标处的 Provider，写入 cfg
    pub fn select_provider(&mut self, cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            cfg.config.active_provider_id = p.id.clone();
        }
    }

    /// 取消删除确认，回到浏览模式
    pub fn cancel_delete(&mut self) {
        self.mode = LoginPanelMode::Browse;
    }

    // ── Edit/New 模式操作 ────────────────────────────────────────────────────

    /// 字段导航：下一个字段
    pub fn field_next(&mut self) {
        self.edit_field = self.edit_field.next();
    }

    /// 字段导航：上一个字段
    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
    }

    /// 循环切换 provider_type（Space 键，仅在 edit_field == Type 时生效）
    /// 切换后自动调用 auto_fill_models_for_type 更新模型名默认值
    pub fn cycle_type(&mut self) {
        if self.edit_field == LoginEditField::Type {
            let cur = PROVIDER_TYPES
                .iter()
                .position(|&t| t == self.buf_type)
                .unwrap_or(0);
            self.buf_type = PROVIDER_TYPES[(cur + 1) % PROVIDER_TYPES.len()].to_string();
            self.auto_fill_models_for_type();
        }
    }

    /// 输入字符到当前活动字段（Type 字段不可直接输入，只能 cycle）
    pub fn push_char(&mut self, c: char) {
        match self.edit_field {
            LoginEditField::Name => self.buf_name.push(c),
            LoginEditField::Type => {} // 只能 cycle
            LoginEditField::BaseUrl => self.buf_base_url.push(c),
            LoginEditField::ApiKey => self.buf_api_key.push(c),
            LoginEditField::OpusModel => self.buf_opus_model.push(c),
            LoginEditField::SonnetModel => self.buf_sonnet_model.push(c),
            LoginEditField::HaikuModel => self.buf_haiku_model.push(c),
        }
    }

    /// 删除当前活动字段末字符（Backspace）
    pub fn pop_char(&mut self) {
        match self.edit_field {
            LoginEditField::Name => {
                self.buf_name.pop();
            }
            LoginEditField::Type => {}
            LoginEditField::BaseUrl => {
                self.buf_base_url.pop();
            }
            LoginEditField::ApiKey => {
                self.buf_api_key.pop();
            }
            LoginEditField::OpusModel => {
                self.buf_opus_model.pop();
            }
            LoginEditField::SonnetModel => {
                self.buf_sonnet_model.pop();
            }
            LoginEditField::HaikuModel => {
                self.buf_haiku_model.pop();
            }
        }
    }

    /// 粘贴文本到当前活动字段（过滤换行符，Type 字段忽略粘贴）
    pub fn paste_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        match self.edit_field {
            LoginEditField::Name => self.buf_name.push_str(&text),
            LoginEditField::Type => {}
            LoginEditField::BaseUrl => self.buf_base_url.push_str(&text),
            LoginEditField::ApiKey => self.buf_api_key.push_str(&text),
            LoginEditField::OpusModel => self.buf_opus_model.push_str(&text),
            LoginEditField::SonnetModel => self.buf_sonnet_model.push_str(&text),
            LoginEditField::HaikuModel => self.buf_haiku_model.push_str(&text),
        }
    }

    // ── Type 切换自动填充 ────────────────────────────────────────────────────

    /// Type 切换时自动填充模型名默认值
    /// 规则：检测三个模型名字段是否为空或等于旧 provider_type 的默认值；若是则填入新 type 的默认值
    pub fn auto_fill_models_for_type(&mut self) {
        let new_defaults = DEFAULT_MODELS
            .iter()
            .find(|(t, _, _, _)| *t == self.buf_type);
        let (opus_default, sonnet_default, haiku_default) = match new_defaults {
            Some((_, o, s, h)) => (o.to_string(), s.to_string(), h.to_string()),
            None => return, // 未知 provider_type，不自动填充
        };

        // 收集所有 provider_type 的默认值作为"旧默认值"候选
        let all_defaults: Vec<(String, String, String)> = DEFAULT_MODELS
            .iter()
            .map(|(_, o, s, h)| (o.to_string(), s.to_string(), h.to_string()))
            .collect();

        let is_default_or_empty = |val: &str| -> bool {
            if val.is_empty() {
                return true;
            }
            all_defaults
                .iter()
                .any(|(o, s, h)| val == o || val == s || val == h)
        };

        if is_default_or_empty(&self.buf_opus_model) {
            self.buf_opus_model = opus_default;
        }
        if is_default_or_empty(&self.buf_sonnet_model) {
            self.buf_sonnet_model = sonnet_default;
        }
        if is_default_or_empty(&self.buf_haiku_model) {
            self.buf_haiku_model = haiku_default;
        }
    }

    // ── 保存/删除操作 ──────────────────────────────────────────────────────────

    /// 将编辑/新建的内容保存到 ZenConfig，并更新内部 providers 快照
    /// 返回 true 表示成功
    /// 新建 Provider 后，active_provider_id 为空时自动设置为新建的 Provider ID
    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) -> bool {
        let is_new = self.mode == LoginPanelMode::New;
        let id = if is_new {
            if self.buf_name.trim().is_empty() {
                return false;
            }
            self.buf_name.trim().to_lowercase().replace(' ', "_")
        } else {
            self.providers
                .get(self.cursor)
                .map(|p| p.id.clone())
                .unwrap_or_default()
        };

        if id.is_empty() {
            return false;
        }

        let mut p = ProviderConfig {
            id: id.clone(),
            provider_type: self.buf_type.clone(),
            api_key: self.buf_api_key.clone(),
            base_url: self.buf_base_url.clone(),
            name: if self.buf_name.trim().is_empty() {
                None
            } else {
                Some(self.buf_name.trim().to_string())
            },
            models: ProviderModels {
                opus: self.buf_opus_model.clone(),
                sonnet: self.buf_sonnet_model.clone(),
                haiku: self.buf_haiku_model.clone(),
            },
            extra: Default::default(),
        };

        // 编辑模式：保留原有的 extra 字段
        if self.mode == LoginPanelMode::Edit {
            if let Some(orig) = self.providers.get(self.cursor) {
                p.extra = orig.extra.clone();
            }
        }

        if is_new {
            cfg.config.providers.push(p);
            self.cursor = cfg.config.providers.len() - 1;
            // active_provider_id 为空时自动设置
            if cfg.config.active_provider_id.is_empty() {
                cfg.config.active_provider_id = id;
            }
        } else if let Some(existing) = cfg.config.providers.iter_mut().find(|x| x.id == id) {
            *existing = p;
        }

        self.providers = cfg.config.providers.clone();
        self.mode = LoginPanelMode::Browse;
        true
    }

    /// 确认删除光标处的 provider，写入 cfg
    pub fn confirm_delete(&mut self, cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            let id = p.id.clone();
            cfg.config.providers.retain(|x| x.id != id);
            self.providers = cfg.config.providers.clone();
            if self.cursor >= self.providers.len() && !self.providers.is_empty() {
                self.cursor = self.providers.len() - 1;
            }
            // 如果删除的是当前激活的 provider，清空 active_provider_id
            if cfg.config.active_provider_id == id {
                cfg.config.active_provider_id.clear();
            }
        }
        self.mode = LoginPanelMode::Browse;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_config() -> ZenConfig {
        let mut cfg = ZenConfig::default();
        cfg.config.active_provider_id = "anthropic".to_string();
        cfg.config.providers.push(ProviderConfig {
            id: "anthropic".to_string(),
            provider_type: "anthropic".to_string(),
            api_key: "sk-ant-123".to_string(),
            base_url: String::new(),
            name: Some("Anthropic".to_string()),
            models: ProviderModels {
                opus: "claude-opus-4-7".to_string(),
                sonnet: "claude-sonnet-4-6".to_string(),
                haiku: "claude-haiku-4-5".to_string(),
            },
            extra: Default::default(),
        });
        cfg.config.providers.push(ProviderConfig {
            id: "openrouter".to_string(),
            provider_type: "openai".to_string(),
            api_key: "or-123".to_string(),
            base_url: "https://openrouter.ai/v1".to_string(),
            name: Some("OpenRouter".to_string()),
            models: ProviderModels {
                opus: "gpt-4o".to_string(),
                sonnet: "gpt-4o-mini".to_string(),
                haiku: "gpt-3.5-turbo".to_string(),
            },
            extra: Default::default(),
        });
        cfg
    }

    #[test]
    fn test_login_panel_from_config_cursor_at_active_provider() {
        let cfg = make_test_config();
        let panel = LoginPanel::from_config(&cfg);
        assert_eq!(panel.cursor, 0); // anthropic is at index 0
    }

    #[test]
    fn test_login_panel_from_config_empty_providers_cursor_zero() {
        let cfg = ZenConfig::default();
        let panel = LoginPanel::from_config(&cfg);
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_login_panel_move_cursor_cycle() {
        let cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        assert_eq!(panel.cursor, 0);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, 1);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, 0); // cycle back
        panel.move_cursor(-1);
        assert_eq!(panel.cursor, 1); // cycle backwards
    }

    #[test]
    fn test_login_panel_enter_edit_fills_buffers() {
        let cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_edit();
        assert_eq!(panel.buf_opus_model, "claude-opus-4-7");
        assert_eq!(panel.buf_api_key, "sk-ant-123");
        assert_eq!(panel.mode, LoginPanelMode::Edit);
    }

    #[test]
    fn test_login_panel_enter_new_auto_fills_openai() {
        let cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_new();
        assert_eq!(panel.buf_type, "openai");
        assert_eq!(panel.buf_opus_model, "gpt-4o");
        assert_eq!(panel.buf_sonnet_model, "gpt-4o-mini");
        assert_eq!(panel.buf_haiku_model, "gpt-3.5-turbo");
    }

    #[test]
    fn test_login_panel_cycle_type_auto_fills_anthropic() {
        let cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_new();
        assert_eq!(panel.buf_type, "openai");
        panel.edit_field = LoginEditField::Type;
        panel.cycle_type();
        assert_eq!(panel.buf_type, "anthropic");
        assert_eq!(panel.buf_opus_model, "claude-opus-4-7");
        assert_eq!(panel.buf_sonnet_model, "claude-sonnet-4-6");
        assert_eq!(panel.buf_haiku_model, "claude-haiku-4-5");
    }

    #[test]
    fn test_login_panel_cycle_type_preserves_custom_model() {
        let cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_new();
        panel.buf_opus_model = "my-custom-model".to_string();
        panel.edit_field = LoginEditField::Type;
        panel.cycle_type();
        assert_eq!(panel.buf_opus_model, "my-custom-model");
    }

    #[test]
    fn test_login_panel_field_navigation() {
        let mut panel = LoginPanel::from_config(&ZenConfig::default());
        assert_eq!(panel.edit_field, LoginEditField::Name);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::Type);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::BaseUrl);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::ApiKey);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::OpusModel);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::SonnetModel);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::HaikuModel);
        panel.field_next();
        assert_eq!(panel.edit_field, LoginEditField::Name);
        panel.field_prev();
        assert_eq!(panel.edit_field, LoginEditField::HaikuModel);
    }

    #[test]
    fn test_login_panel_push_pop_char() {
        let mut panel = LoginPanel::from_config(&ZenConfig::default());
        panel.edit_field = LoginEditField::OpusModel;
        panel.push_char('x');
        panel.push_char('x');
        assert_eq!(panel.buf_opus_model, "xx");
        panel.pop_char();
        assert_eq!(panel.buf_opus_model, "x");
    }

    #[test]
    fn test_login_panel_push_char_ignored_for_type() {
        let mut panel = LoginPanel::from_config(&ZenConfig::default());
        let orig_type = panel.buf_type.clone();
        panel.edit_field = LoginEditField::Type;
        panel.push_char('a');
        assert_eq!(panel.buf_type, orig_type);
    }

    #[test]
    fn test_login_panel_paste_text_filters_newlines() {
        let mut panel = LoginPanel::from_config(&ZenConfig::default());
        panel.edit_field = LoginEditField::ApiKey;
        panel.paste_text("key\nval\r\nend");
        assert_eq!(panel.buf_api_key, "keyvalend");
    }

    #[test]
    fn test_login_panel_paste_text_ignored_for_type() {
        let mut panel = LoginPanel::from_config(&ZenConfig::default());
        let orig_type = panel.buf_type.clone();
        panel.edit_field = LoginEditField::Type;
        panel.paste_text("anthropic");
        assert_eq!(panel.buf_type, orig_type);
    }

    #[test]
    fn test_login_panel_apply_edit_new_provider() {
        let mut cfg = ZenConfig::default();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_new();
        panel.buf_name = "My Provider".to_string();
        panel.buf_api_key = "sk-123".to_string();
        panel.buf_opus_model = "gpt-4o".to_string();
        panel.buf_sonnet_model = "gpt-4o-mini".to_string();
        panel.buf_haiku_model = "gpt-3.5-turbo".to_string();
        let ok = panel.apply_edit(&mut cfg);
        assert!(ok);
        assert_eq!(cfg.config.providers.len(), 1);
        assert_eq!(cfg.config.providers[0].models.opus, "gpt-4o");
    }

    #[test]
    fn test_login_panel_apply_edit_new_provider_sets_active_id_when_empty() {
        let mut cfg = ZenConfig::default();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_new();
        panel.buf_name = "Test".to_string();
        panel.buf_api_key = "key".to_string();
        panel.apply_edit(&mut cfg);
        assert_eq!(cfg.config.active_provider_id, "test");
    }

    #[test]
    fn test_login_panel_apply_edit_existing_provider() {
        let mut cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_edit();
        panel.buf_api_key = "new-key".to_string();
        let ok = panel.apply_edit(&mut cfg);
        assert!(ok);
        assert_eq!(cfg.config.providers[0].api_key, "new-key");
    }

    #[test]
    fn test_login_panel_apply_edit_empty_name_returns_false() {
        let mut cfg = ZenConfig::default();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.enter_new();
        panel.buf_name = String::new();
        let ok = panel.apply_edit(&mut cfg);
        assert!(!ok);
    }

    #[test]
    fn test_login_panel_confirm_delete_removes_provider() {
        let mut cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.cursor = 1; // openrouter
        panel.mode = LoginPanelMode::ConfirmDelete;
        panel.confirm_delete(&mut cfg);
        assert_eq!(cfg.config.providers.len(), 1);
        assert_eq!(cfg.config.providers[0].id, "anthropic");
    }

    #[test]
    fn test_login_panel_confirm_delete_clears_active_provider_id() {
        let mut cfg = make_test_config();
        let mut panel = LoginPanel::from_config(&cfg);
        panel.cursor = 0; // anthropic (active)
        panel.mode = LoginPanelMode::ConfirmDelete;
        panel.confirm_delete(&mut cfg);
        assert!(cfg.config.active_provider_id.is_empty());
    }

    #[test]
    fn test_login_panel_request_delete_no_providers_noop() {
        let cfg = ZenConfig::default();
        let mut panel = LoginPanel::from_config(&cfg);
        assert_eq!(panel.mode, LoginPanelMode::Browse);
        panel.request_delete();
        assert_eq!(panel.mode, LoginPanelMode::Browse);
    }
}
