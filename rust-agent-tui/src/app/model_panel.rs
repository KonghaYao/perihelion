use perihelion_widgets::{FormField, FormState};
use crate::config::{ModelAliasConfig, ProviderConfig, ThinkingConfig, ZenConfig};

// ─── AliasTab 枚举 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AliasTab {
    Opus,
    Sonnet,
    Haiku,
}

impl AliasTab {
    pub fn next(&self) -> Self {
        match self {
            Self::Opus   => Self::Sonnet,
            Self::Sonnet => Self::Haiku,
            Self::Haiku  => Self::Opus,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Opus   => Self::Haiku,
            Self::Sonnet => Self::Opus,
            Self::Haiku  => Self::Sonnet,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Opus   => "Opus",
            Self::Sonnet => "Sonnet",
            Self::Haiku  => "Haiku",
        }
    }

    pub fn to_key(&self) -> &str {
        match self {
            Self::Opus   => "opus",
            Self::Sonnet => "sonnet",
            Self::Haiku  => "haiku",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Self::Opus   => 0,
            Self::Sonnet => 1,
            Self::Haiku  => 2,
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "sonnet" => Self::Sonnet,
            "haiku"  => Self::Haiku,
            _        => Self::Opus,
        }
    }
}

/// 别名编辑区内的字段（Provider 选择 / Model ID 输入）
#[derive(Debug, Clone, PartialEq)]
pub enum AliasEditField {
    Provider,
    ModelId,
}

impl AliasEditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Provider => Self::ModelId,
            Self::ModelId  => Self::Provider,
        }
    }

    pub fn prev(&self) -> Self {
        self.next() // 只有两个字段，next == prev
    }
}

// ─── Provider 管理相关枚举 ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ModelPanelMode {
    /// 别名配置主界面
    AliasConfig,
    /// 浏览 provider 列表（provider 管理子面板）
    Browse,
    /// 编辑已有 provider
    Edit,
    /// 新建 provider
    New,
    /// 删除确认（等待 y/n）
    ConfirmDelete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditField {
    Name,
    ProviderType,
    ModelId,
    ApiKey,
    BaseUrl,
    ThinkingBudget,
}

impl FormField for EditField {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::ProviderType,
            Self::ProviderType => Self::ApiKey,
            Self::ModelId => Self::ApiKey,
            Self::ApiKey => Self::BaseUrl,
            Self::BaseUrl => Self::ThinkingBudget,
            Self::ThinkingBudget => Self::Name,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Name => Self::ThinkingBudget,
            Self::ProviderType => Self::Name,
            Self::ModelId => Self::ProviderType,
            Self::ApiKey => Self::ProviderType,
            Self::BaseUrl => Self::ApiKey,
            Self::ThinkingBudget => Self::BaseUrl,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name    ",
            Self::ProviderType => "Type    ",
            Self::ModelId => "Model ID",
            Self::ApiKey => "API Key ",
            Self::BaseUrl => "Base URL",
            Self::ThinkingBudget => "Thinking",
        }
    }
}

impl EditField {
    pub fn all() -> &'static [EditField] {
        &[
            Self::Name,
            Self::ProviderType,
            Self::ModelId,
            Self::ApiKey,
            Self::BaseUrl,
            Self::ThinkingBudget,
        ]
    }
}

/// provider_type 循环切换
pub const PROVIDER_TYPES: &[&str] = &["openai", "anthropic"];

// ─── ModelPanel ───────────────────────────────────────────────────────────────

pub struct ModelPanel {
    /// provider 列表快照（从 ZenConfig 获取）
    pub providers: Vec<ProviderConfig>,
    /// 当前模式（主界面 or provider 管理子面板）
    pub mode: ModelPanelMode,

    // ── 别名配置区字段（AliasConfig 模式）───────────────────────────────────

    /// 当前激活的 Tab（Opus/Sonnet/Haiku）
    pub active_tab: AliasTab,
    /// 三个 Tab 各自的 provider_id 缓冲（索引对应 opus/sonnet/haiku）
    pub buf_alias_provider: [String; 3],
    /// 三个 Tab 各自的 model_id 缓冲
    pub buf_alias_model: [String; 3],
    /// 别名编辑区当前聚焦字段
    pub alias_edit_field: AliasEditField,

    // ── Provider 管理区字段（Browse/Edit/New/ConfirmDelete 模式）─────────────

    /// 光标位置（provider 管理列表中）
    pub cursor: usize,
    /// 当前激活的 provider_id（旧字段，仍在 Browse 模式下展示信息用）
    pub active_id: String,
    /// 表单状态（管理 Name/ProviderType/ModelId/ApiKey/BaseUrl/ThinkingBudget 字段）
    pub form: FormState<EditField>,
    /// Thinking 配置缓冲（全局，不属于单个 provider，bool toggle 不在 FormState 中）
    pub buf_thinking_enabled: bool,
    /// 内容滚动偏移
    #[allow(dead_code)]
    pub scroll_offset: u16,
}

impl ModelPanel {
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let providers = cfg.config.providers.clone();
        let active_tab = AliasTab::from_key(&cfg.config.active_alias);
        let (thinking_enabled, thinking_budget) = match &cfg.config.thinking {
            Some(t) => (t.enabled, t.budget_tokens.to_string()),
            None => (false, "8000".to_string()),
        };

        let aliases = &cfg.config.model_aliases;
        let buf_alias_provider = [
            aliases.opus.provider_id.clone(),
            aliases.sonnet.provider_id.clone(),
            aliases.haiku.provider_id.clone(),
        ];
        let buf_alias_model = [
            aliases.opus.model_id.clone(),
            aliases.sonnet.model_id.clone(),
            aliases.haiku.model_id.clone(),
        ];

        // 旧字段 provider_id（现在用于 Browse 模式展示用，从 active_alias 推断）
        let active_id = {
            let idx = active_tab.index();
            buf_alias_provider[idx].clone()
        };
        let cursor = providers.iter().position(|p| p.id == active_id).unwrap_or(0);

        let mut form = FormState::new(EditField::all().iter().copied());
        form.set_active(EditField::Name);
        form.input_mut(EditField::ThinkingBudget).set_value(thinking_budget);

        Self {
            providers,
            mode: ModelPanelMode::AliasConfig,
            active_tab,
            buf_alias_provider,
            buf_alias_model,
            alias_edit_field: AliasEditField::Provider,
            cursor,
            active_id,
            form,
            buf_thinking_enabled: thinking_enabled,
            scroll_offset: 0,
        }
    }

    // ── EditField / FormState 访问方法 ─────────────────────────────────────────

    /// 获取当前编辑字段
    pub fn edit_field(&self) -> EditField {
        self.form.active_field()
    }

    /// 获取指定字段的值
    pub fn field_value(&self, field: EditField) -> &str {
        self.form.input(field).value()
    }

    /// 设置指定字段的值
    fn set_field_value(&mut self, field: EditField, value: String) {
        self.form.input_mut(field).set_value(value);
    }

    // ── AliasConfig 模式操作 ─────────────────────────────────────────────────

    pub fn tab_next(&mut self) {
        self.active_tab = self.active_tab.next();
    }

    pub fn tab_prev(&mut self) {
        self.active_tab = self.active_tab.prev();
    }

    pub fn alias_field_next(&mut self) {
        self.alias_edit_field = self.alias_edit_field.next();
    }

    pub fn alias_field_prev(&mut self) {
        self.alias_edit_field = self.alias_edit_field.prev();
    }

    /// 在 providers 列表中循环切换当前 Tab 的 provider（Space 键）
    pub fn cycle_alias_provider(&mut self) {
        if self.providers.is_empty() { return; }
        let idx = self.active_tab.index();
        let current = &self.buf_alias_provider[idx];
        let pos = self.providers.iter().position(|p| &p.id == current).unwrap_or(0);
        let next_pos = (pos + 1) % self.providers.len();
        self.buf_alias_provider[idx] = self.providers[next_pos].id.clone();
    }

    /// 写入当前 Tab 的 model_id 缓冲（字符输入）
    pub fn push_alias_char(&mut self, c: char) {
        if self.alias_edit_field == AliasEditField::ModelId {
            let idx = self.active_tab.index();
            self.buf_alias_model[idx].push(c);
        }
    }

    /// 删除当前 Tab 的 model_id 缓冲末字符（Backspace）
    pub fn pop_alias_char(&mut self) {
        if self.alias_edit_field == AliasEditField::ModelId {
            let idx = self.active_tab.index();
            self.buf_alias_model[idx].pop();
        }
    }

    /// 将当前 Tab 的缓冲写回 cfg.config.model_aliases 对应字段
    pub fn apply_alias_edit(&self, cfg: &mut ZenConfig) {
        let write_alias = |alias: &mut ModelAliasConfig, provider_id: &str, model_id: &str| {
            alias.provider_id = provider_id.to_string();
            alias.model_id = model_id.to_string();
        };
        write_alias(&mut cfg.config.model_aliases.opus,   &self.buf_alias_provider[0], &self.buf_alias_model[0]);
        write_alias(&mut cfg.config.model_aliases.sonnet, &self.buf_alias_provider[1], &self.buf_alias_model[1]);
        write_alias(&mut cfg.config.model_aliases.haiku,  &self.buf_alias_provider[2], &self.buf_alias_model[2]);
    }

    /// 将 active_tab 写入 cfg.config.active_alias 并返回（调用方负责保存）
    pub fn activate_current_tab(&self, cfg: &mut ZenConfig) {
        cfg.config.active_alias = self.active_tab.to_key().to_string();
    }

    // ── Browse 模式操作 ──────────────────────────────────────────────────────

    pub fn move_cursor(&mut self, delta: isize) {
        if self.providers.is_empty() { return; }
        let len = self.providers.len();
        self.cursor = ((self.cursor as isize + delta).rem_euclid(len as isize)) as usize;
    }

    /// 进入编辑模式（编辑光标处的 provider）
    pub fn enter_edit(&mut self) {
        if let Some(p) = self.providers.get(self.cursor).cloned() {
            self.set_field_value(EditField::Name, p.display_name().to_string());
            self.set_field_value(EditField::ProviderType, p.provider_type);
            self.set_field_value(EditField::ModelId, String::new());
            self.set_field_value(EditField::ApiKey, p.api_key);
            self.set_field_value(EditField::BaseUrl, p.base_url);
            self.form.set_active(EditField::Name);
            self.mode = ModelPanelMode::Edit;
        }
    }

    /// 进入新建模式
    pub fn enter_new(&mut self) {
        self.set_field_value(EditField::Name, String::new());
        self.set_field_value(EditField::ProviderType, "openai".to_string());
        self.set_field_value(EditField::ModelId, String::new());
        self.set_field_value(EditField::ApiKey, String::new());
        self.set_field_value(EditField::BaseUrl, String::new());
        self.form.set_active(EditField::Name);
        self.mode = ModelPanelMode::New;
    }

    /// 切换 thinking enabled（空格键，当 edit_field == ThinkingBudget 时）
    pub fn toggle_thinking(&mut self) {
        if self.edit_field() == EditField::ThinkingBudget {
            self.buf_thinking_enabled = !self.buf_thinking_enabled;
        }
    }

    /// 进入删除确认模式
    pub fn request_delete(&mut self) {
        if !self.providers.is_empty() {
            self.mode = ModelPanelMode::ConfirmDelete;
        }
    }

    /// 取消删除确认，回到浏览
    pub fn cancel_delete(&mut self) {
        self.mode = ModelPanelMode::Browse;
    }

    // ── 编辑模式操作 ──────────────────────────────────────────────────────────

    pub fn field_next(&mut self) {
        self.form.next_field();
    }

    pub fn field_prev(&mut self) {
        self.form.prev_field();
    }

    /// 循环切换 provider_type（空格键）
    pub fn cycle_type(&mut self) {
        if self.edit_field() == EditField::ProviderType {
            let cur = PROVIDER_TYPES.iter().position(|&t| t == self.field_value(EditField::ProviderType)).unwrap_or(0);
            let next = PROVIDER_TYPES[(cur + 1) % PROVIDER_TYPES.len()].to_string();
            self.set_field_value(EditField::ProviderType, next);
        }
    }

    pub fn push_char(&mut self, c: char) {
        match self.edit_field() {
            EditField::Name => self.form.input_mut(EditField::Name).insert(c),
            EditField::ProviderType => {} // 只能 cycle，不能直接输入
            EditField::ModelId => self.form.input_mut(EditField::ModelId).insert(c),
            EditField::ApiKey => self.form.input_mut(EditField::ApiKey).insert(c),
            EditField::BaseUrl => self.form.input_mut(EditField::BaseUrl).insert(c),
            EditField::ThinkingBudget => {
                if c.is_ascii_digit() {
                    self.form.input_mut(EditField::ThinkingBudget).insert(c);
                }
            }
        }
    }

    pub fn pop_char(&mut self) {
        match self.edit_field() {
            EditField::Name => { self.form.input_mut(EditField::Name).backspace(); }
            EditField::ProviderType => {}
            EditField::ModelId => { self.form.input_mut(EditField::ModelId).backspace(); }
            EditField::ApiKey => { self.form.input_mut(EditField::ApiKey).backspace(); }
            EditField::BaseUrl => { self.form.input_mut(EditField::BaseUrl).backspace(); }
            EditField::ThinkingBudget => { self.form.input_mut(EditField::ThinkingBudget).backspace(); }
        }
    }

    /// 粘贴文本到当前活动字段（Edit/New 模式追加到当前字段；AliasConfig 模式追加到 model_id）
    pub fn paste_text(&mut self, text: &str) {
        // 过滤换行符，字段均为单行
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        match self.mode {
            ModelPanelMode::AliasConfig => {
                if self.alias_edit_field == AliasEditField::ModelId {
                    let idx = self.active_tab.index();
                    self.buf_alias_model[idx].push_str(&text);
                }
            }
            ModelPanelMode::Edit | ModelPanelMode::New => {
                match self.edit_field() {
                    EditField::Name => self.form.input_mut(EditField::Name).paste(&text),
                    EditField::ProviderType => {}
                    EditField::ModelId => self.form.input_mut(EditField::ModelId).paste(&text),
                    EditField::ApiKey => self.form.input_mut(EditField::ApiKey).paste(&text),
                    EditField::BaseUrl => self.form.input_mut(EditField::BaseUrl).paste(&text),
                    EditField::ThinkingBudget => {
                        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
                        self.form.input_mut(EditField::ThinkingBudget).paste(&digits);
                    }
                }
            }
            _ => {}
        }
    }

    // ── 保存操作 ──────────────────────────────────────────────────────────────

    /// 将编辑/新建的内容应用到 ZenConfig，并更新内部 providers 快照
    /// 返回 true 表示成功
    /// 新建 provider 时，会自动关联到当前激活的 alias
    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) -> bool {
        let is_new = self.mode == ModelPanelMode::New;
        let id = if is_new {
            if self.field_value(EditField::Name).trim().is_empty() {
                return false;
            }
            self.field_value(EditField::Name).trim().to_lowercase().replace(' ', "_")
        } else {
            self.providers.get(self.cursor).map(|p| p.id.clone()).unwrap_or_default()
        };

        if id.is_empty() { return false; }

        let mut p = ProviderConfig {
            id: id.clone(),
            provider_type: self.field_value(EditField::ProviderType).to_string(),
            api_key: self.field_value(EditField::ApiKey).to_string(),
            base_url: self.field_value(EditField::BaseUrl).to_string(),
            name: if self.field_value(EditField::Name).trim().is_empty() { None } else { Some(self.field_value(EditField::Name).trim().to_string()) },
            extra: Default::default(),
        };

        // 保留原有的 extra 字段
        if self.mode == ModelPanelMode::Edit {
            if let Some(orig) = self.providers.get(self.cursor) {
                p.extra = orig.extra.clone();
            }
        }

        if is_new {
            cfg.config.providers.push(p);
            self.cursor = cfg.config.providers.len() - 1;
        } else if let Some(existing) = cfg.config.providers.iter_mut().find(|x| x.id == id) {
            *existing = p;
        }

        // 保存 thinking 配置（全局，不属于单个 provider）
        let budget_tokens = self.field_value(EditField::ThinkingBudget).trim().parse::<u32>().unwrap_or(8000);
        cfg.config.thinking = Some(ThinkingConfig {
            enabled: self.buf_thinking_enabled,
            budget_tokens,
        });

        // 新建 provider 时，自动关联到当前激活的 alias
        if is_new {
            let idx = self.active_tab.index();
            self.buf_alias_provider[idx] = id.clone();
        }

        self.providers = cfg.config.providers.clone();
        self.mode = ModelPanelMode::AliasConfig;
        true
    }

    /// 确认选中当前 provider（仅在 Browse 模式下更新显示，不写 provider_id 到 cfg）
    pub fn confirm_select(&mut self, _cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            self.active_id = p.id.clone();
        }
    }

    /// 删除光标处的 provider，写入 cfg
    pub fn confirm_delete(&mut self, cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            let id = p.id.clone();
            cfg.config.providers.retain(|x| x.id != id);
            self.providers = cfg.config.providers.clone();
            if self.cursor >= self.providers.len() && !self.providers.is_empty() {
                self.cursor = self.providers.len() - 1;
            }
        }
        self.mode = ModelPanelMode::Browse;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ZenConfig};

    fn make_cfg_with_providers() -> ZenConfig {
        let mut cfg = ZenConfig::default();
        cfg.config.providers.push(ProviderConfig {
            id: "anthropic".to_string(),
            provider_type: "anthropic".to_string(),
            api_key: "key1".to_string(),
            ..Default::default()
        });
        cfg.config.providers.push(ProviderConfig {
            id: "openrouter".to_string(),
            provider_type: "openai".to_string(),
            api_key: "key2".to_string(),
            ..Default::default()
        });
        cfg.config.active_alias = "opus".to_string();
        cfg.config.model_aliases.opus.provider_id = "anthropic".to_string();
        cfg.config.model_aliases.opus.model_id = "claude-opus-4-6".to_string();
        cfg.config.model_aliases.sonnet.provider_id = "anthropic".to_string();
        cfg.config.model_aliases.sonnet.model_id = "claude-sonnet-4-6".to_string();
        cfg.config.model_aliases.haiku.provider_id = "openrouter".to_string();
        cfg.config.model_aliases.haiku.model_id = "gpt-4o-mini".to_string();
        cfg
    }

    #[test]
    fn test_model_panel_from_config_loads_alias_buffers() {
        let cfg = make_cfg_with_providers();
        let panel = ModelPanel::from_config(&cfg);

        assert_eq!(panel.active_tab, AliasTab::Opus);
        assert_eq!(panel.buf_alias_provider[0], "anthropic");
        assert_eq!(panel.buf_alias_model[0], "claude-opus-4-6");
        assert_eq!(panel.buf_alias_provider[1], "anthropic");
        assert_eq!(panel.buf_alias_model[1], "claude-sonnet-4-6");
        assert_eq!(panel.buf_alias_provider[2], "openrouter");
        assert_eq!(panel.buf_alias_model[2], "gpt-4o-mini");
    }

    #[test]
    fn test_tab_switching() {
        let cfg = make_cfg_with_providers();
        let mut panel = ModelPanel::from_config(&cfg);

        assert_eq!(panel.active_tab, AliasTab::Opus);
        panel.tab_next();
        assert_eq!(panel.active_tab, AliasTab::Sonnet);
        panel.tab_next();
        assert_eq!(panel.active_tab, AliasTab::Haiku);
        panel.tab_next();
        assert_eq!(panel.active_tab, AliasTab::Opus); // wrap around

        panel.tab_prev();
        assert_eq!(panel.active_tab, AliasTab::Haiku);
    }

    #[test]
    fn test_cycle_alias_provider() {
        let cfg = make_cfg_with_providers();
        let mut panel = ModelPanel::from_config(&cfg);

        // 初始 opus provider = anthropic
        assert_eq!(panel.buf_alias_provider[0], "anthropic");
        panel.cycle_alias_provider();
        assert_eq!(panel.buf_alias_provider[0], "openrouter");
        panel.cycle_alias_provider();
        assert_eq!(panel.buf_alias_provider[0], "anthropic"); // wrap
    }

    #[test]
    fn test_push_pop_alias_char() {
        let cfg = make_cfg_with_providers();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.alias_edit_field = AliasEditField::ModelId;
        panel.buf_alias_model[0] = String::new();

        panel.push_alias_char('g');
        panel.push_alias_char('p');
        panel.push_alias_char('t');
        assert_eq!(panel.buf_alias_model[0], "gpt");

        panel.pop_alias_char();
        assert_eq!(panel.buf_alias_model[0], "gp");
    }

    #[test]
    fn test_apply_alias_edit_writes_to_cfg() {
        let mut cfg = make_cfg_with_providers();
        let mut panel = ModelPanel::from_config(&cfg);

        panel.buf_alias_model[0] = "claude-opus-5-0".to_string();
        panel.apply_alias_edit(&mut cfg);

        assert_eq!(cfg.config.model_aliases.opus.model_id, "claude-opus-5-0");
    }

    #[test]
    fn test_activate_current_tab() {
        let mut cfg = make_cfg_with_providers();
        let mut panel = ModelPanel::from_config(&cfg);

        panel.tab_next(); // → Sonnet
        panel.activate_current_tab(&mut cfg);
        assert_eq!(cfg.config.active_alias, "sonnet");
    }

    // ── 保持与 types.rs 中旧测试的接口兼容 ──────────────────────────────────

    #[test]
    fn test_model_panel_from_config_loads_thinking() {
        use crate::config::ThinkingConfig;

        let mut cfg = ZenConfig::default();
        cfg.config.thinking = Some(ThinkingConfig { enabled: true, budget_tokens: 4000 });

        let panel = ModelPanel::from_config(&cfg);
        assert!(panel.buf_thinking_enabled);
        assert_eq!(panel.field_value(EditField::ThinkingBudget), "4000");
    }

    #[test]
    fn test_model_panel_from_config_defaults_when_no_thinking() {
        let cfg = ZenConfig::default();
        let panel = ModelPanel::from_config(&cfg);
        assert!(!panel.buf_thinking_enabled);
        assert_eq!(panel.field_value(EditField::ThinkingBudget), "8000");
    }

    #[test]
    fn test_model_panel_toggle_thinking() {
        let cfg = ZenConfig::default();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.form.set_active(EditField::ThinkingBudget);

        assert!(!panel.buf_thinking_enabled);
        panel.toggle_thinking();
        assert!(panel.buf_thinking_enabled);
        panel.toggle_thinking();
        assert!(!panel.buf_thinking_enabled);
    }

    #[test]
    fn test_model_panel_thinking_budget_input_only_digits() {
        let cfg = ZenConfig::default();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.form.set_active(EditField::ThinkingBudget);
        panel.set_field_value(EditField::ThinkingBudget, String::new());

        panel.push_char('1');
        panel.push_char('a'); // 非数字，应忽略
        panel.push_char('2');
        panel.push_char('0');
        assert_eq!(panel.field_value(EditField::ThinkingBudget), "120");

        panel.pop_char();
        assert_eq!(panel.field_value(EditField::ThinkingBudget), "12");
    }

    #[test]
    fn test_model_panel_apply_edit_saves_thinking() {
        let mut cfg = make_cfg_with_providers();

        let mut panel = ModelPanel::from_config(&cfg);
        panel.mode = ModelPanelMode::Edit;
        panel.buf_thinking_enabled = true;
        panel.set_field_value(EditField::ThinkingBudget, "5000".to_string());

        let ok = panel.apply_edit(&mut cfg);
        assert!(ok);

        let t = cfg.config.thinking.as_ref().unwrap();
        assert_eq!(t.enabled, true);
        assert_eq!(t.budget_tokens, 5000);
    }

    #[test]
    fn test_model_panel_form_state_text_editing() {
        let cfg = make_cfg_with_providers();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.mode = ModelPanelMode::Edit;
        panel.form.set_active(EditField::Name);

        panel.push_char('t');
        panel.push_char('e');
        panel.push_char('s');
        panel.push_char('t');
        assert_eq!(panel.field_value(EditField::Name), "test");

        panel.form.next_field(); // → ProviderType
        panel.cycle_type();
        assert_eq!(panel.field_value(EditField::ProviderType), "anthropic");

        panel.form.next_field(); // → ApiKey (ProviderType.next = ApiKey)
        panel.push_char('k');
        panel.push_char('e');
        panel.push_char('y');
        assert_eq!(panel.field_value(EditField::ApiKey), "key");

        // 切回 Name，值保持
        panel.form.set_active(EditField::Name);
        assert_eq!(panel.field_value(EditField::Name), "test");
    }

    #[test]
    fn test_model_panel_form_state_field_navigation() {
        let cfg = ZenConfig::default();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.edit_field(), EditField::Name);

        panel.field_next();
        assert_eq!(panel.edit_field(), EditField::ProviderType);

        panel.field_next();
        assert_eq!(panel.edit_field(), EditField::ApiKey);

        panel.field_next();
        assert_eq!(panel.edit_field(), EditField::BaseUrl);

        panel.field_next();
        assert_eq!(panel.edit_field(), EditField::ThinkingBudget);

        panel.field_next();
        assert_eq!(panel.edit_field(), EditField::Name); // wrap
    }
}
