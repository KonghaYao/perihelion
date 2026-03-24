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

#[derive(Debug, Clone, PartialEq)]
pub enum EditField {
    Name,
    ProviderType,
    ModelId,
    ApiKey,
    BaseUrl,
    ThinkingBudget,
}

impl EditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Name => Self::ProviderType,
            Self::ProviderType => Self::ModelId,
            Self::ModelId => Self::ApiKey,
            Self::ApiKey => Self::BaseUrl,
            Self::BaseUrl => Self::ThinkingBudget,
            Self::ThinkingBudget => Self::Name,
        }
    }
    pub fn prev(&self) -> Self {
        match self {
            Self::Name => Self::ThinkingBudget,
            Self::ProviderType => Self::Name,
            Self::ModelId => Self::ProviderType,
            Self::ApiKey => Self::ModelId,
            Self::BaseUrl => Self::ApiKey,
            Self::ThinkingBudget => Self::BaseUrl,
        }
    }
    pub fn label(&self) -> &str {
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
    /// 正在编辑的字段
    pub edit_field: EditField,
    /// 编辑缓冲区（新建/编辑时使用）
    pub buf_name: String,
    pub buf_type: String,
    pub buf_model: String,
    pub buf_api_key: String,
    pub buf_base_url: String,
    /// Thinking 配置缓冲（全局，不属于单个 provider）
    pub buf_thinking_enabled: bool,
    pub buf_thinking_budget: String,
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

        Self {
            providers,
            mode: ModelPanelMode::AliasConfig,
            active_tab,
            buf_alias_provider,
            buf_alias_model,
            alias_edit_field: AliasEditField::Provider,
            cursor,
            active_id,
            edit_field: EditField::Name,
            buf_name: String::new(),
            buf_type: String::new(),
            buf_model: String::new(),
            buf_api_key: String::new(),
            buf_base_url: String::new(),
            buf_thinking_enabled: thinking_enabled,
            buf_thinking_budget: thinking_budget,
            scroll_offset: 0,
        }
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
        if let Some(p) = self.providers.get(self.cursor) {
            self.buf_name = p.display_name().to_string();
            self.buf_type = p.provider_type.clone();
            self.buf_model = String::new();
            self.buf_api_key = p.api_key.clone();
            self.buf_base_url = p.base_url.clone();
            self.edit_field = EditField::Name;
            self.mode = ModelPanelMode::Edit;
        }
    }

    /// 进入新建模式
    pub fn enter_new(&mut self) {
        self.buf_name = String::new();
        self.buf_type = "openai".to_string();
        self.buf_model = String::new();
        self.buf_api_key = String::new();
        self.buf_base_url = String::new();
        self.edit_field = EditField::Name;
        self.mode = ModelPanelMode::New;
    }

    /// 切换 thinking enabled（空格键，当 edit_field == ThinkingBudget 时）
    pub fn toggle_thinking(&mut self) {
        if self.edit_field == EditField::ThinkingBudget {
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
        self.edit_field = self.edit_field.next();
    }

    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
    }

    /// 循环切换 provider_type（空格键）
    pub fn cycle_type(&mut self) {
        if self.edit_field == EditField::ProviderType {
            let cur = PROVIDER_TYPES.iter().position(|&t| t == self.buf_type).unwrap_or(0);
            self.buf_type = PROVIDER_TYPES[(cur + 1) % PROVIDER_TYPES.len()].to_string();
        }
    }

    pub fn push_char(&mut self, c: char) {
        match self.edit_field {
            EditField::Name => self.buf_name.push(c),
            EditField::ProviderType => {} // 只能 cycle，不能直接输入
            EditField::ModelId => self.buf_model.push(c),
            EditField::ApiKey => self.buf_api_key.push(c),
            EditField::BaseUrl => self.buf_base_url.push(c),
            EditField::ThinkingBudget => {
                if c.is_ascii_digit() {
                    self.buf_thinking_budget.push(c);
                }
            }
        }
    }

    pub fn pop_char(&mut self) {
        match self.edit_field {
            EditField::Name => { self.buf_name.pop(); }
            EditField::ProviderType => {}
            EditField::ModelId => { self.buf_model.pop(); }
            EditField::ApiKey => { self.buf_api_key.pop(); }
            EditField::BaseUrl => { self.buf_base_url.pop(); }
            EditField::ThinkingBudget => { self.buf_thinking_budget.pop(); }
        }
    }

    // ── 保存操作 ──────────────────────────────────────────────────────────────

    /// 将编辑/新建的内容应用到 ZenConfig，并更新内部 providers 快照
    /// 返回 true 表示成功
    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) -> bool {
        let id = if self.mode == ModelPanelMode::New {
            if self.buf_name.trim().is_empty() {
                return false;
            }
            self.buf_name.trim().to_lowercase().replace(' ', "_")
        } else {
            self.providers.get(self.cursor).map(|p| p.id.clone()).unwrap_or_default()
        };

        if id.is_empty() { return false; }

        let mut p = ProviderConfig {
            id: id.clone(),
            provider_type: self.buf_type.clone(),
            api_key: self.buf_api_key.clone(),
            base_url: self.buf_base_url.clone(),
            name: if self.buf_name.trim().is_empty() { None } else { Some(self.buf_name.trim().to_string()) },
            extra: Default::default(),
        };

        // 保留原有的 extra 字段
        if self.mode == ModelPanelMode::Edit {
            if let Some(orig) = self.providers.get(self.cursor) {
                p.extra = orig.extra.clone();
            }
        }

        if self.mode == ModelPanelMode::New {
            cfg.config.providers.push(p);
            self.cursor = cfg.config.providers.len() - 1;
        } else if let Some(existing) = cfg.config.providers.iter_mut().find(|x| x.id == id) {
            *existing = p;
        }

        // 保存 thinking 配置（全局，不属于单个 provider）
        let budget_tokens = self.buf_thinking_budget.trim().parse::<u32>().unwrap_or(8000);
        cfg.config.thinking = Some(ThinkingConfig {
            enabled: self.buf_thinking_enabled,
            budget_tokens,
        });

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
        assert_eq!(panel.buf_thinking_budget, "4000");
    }

    #[test]
    fn test_model_panel_from_config_defaults_when_no_thinking() {
        let cfg = ZenConfig::default();
        let panel = ModelPanel::from_config(&cfg);
        assert!(!panel.buf_thinking_enabled);
        assert_eq!(panel.buf_thinking_budget, "8000");
    }

    #[test]
    fn test_model_panel_toggle_thinking() {
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
        let mut cfg = make_cfg_with_providers();

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
