use crate::config::{ProviderConfig, ThinkingConfig, ZenConfig};

// ─── 枚举 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ModelPanelMode {
    /// 浏览 provider 列表
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
    /// 当前激活的 provider_id（来自 ZenConfig）
    pub active_id: String,
    /// 光标位置
    pub cursor: usize,
    /// 当前模式
    pub mode: ModelPanelMode,
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
}

impl ModelPanel {
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let providers = cfg.config.providers.clone();
        let active_id = cfg.config.provider_id.clone();
        // 将光标定位到当前激活的 provider
        let cursor = providers.iter().position(|p| p.id == active_id).unwrap_or(0);
        let (thinking_enabled, thinking_budget) = match &cfg.config.thinking {
            Some(t) => (t.enabled, t.budget_tokens.to_string()),
            None => (false, "8000".to_string()),
        };
        Self {
            providers,
            active_id,
            cursor,
            mode: ModelPanelMode::Browse,
            edit_field: EditField::Name,
            buf_name: String::new(),
            buf_type: String::new(),
            buf_model: String::new(),
            buf_api_key: String::new(),
            buf_base_url: String::new(),
            buf_thinking_enabled: thinking_enabled,
            buf_thinking_budget: thinking_budget,
        }
    }

    // ── 浏览模式操作 ──────────────────────────────────────────────────────────

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
            // thinking 缓冲保持当前全局值，不重置
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
        // thinking 缓冲保持当前全局值，不重置
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
            // 用 name 转 snake_case 作为 id，不能为空
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

        // 同步 model_id（若填了的话）
        if !self.buf_model.trim().is_empty() && cfg.config.provider_id == id {
            cfg.config.model_id = self.buf_model.trim().to_string();
        }

        // 保存 thinking 配置（全局，不属于单个 provider）
        let budget_tokens = self.buf_thinking_budget.trim().parse::<u32>().unwrap_or(8000);
        cfg.config.thinking = Some(ThinkingConfig {
            enabled: self.buf_thinking_enabled,
            budget_tokens,
        });

        self.providers = cfg.config.providers.clone();
        self.mode = ModelPanelMode::Browse;
        true
    }

    /// 确认选中当前 provider（更新 provider_id），写入 cfg
    pub fn confirm_select(&mut self, cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            cfg.config.provider_id = p.id.clone();
            // 如果当前 model_id 为空则清空，让使用方从 env fallback
            self.active_id = p.id.clone();
        }
    }

    /// 删除光标处的 provider，写入 cfg
    pub fn confirm_delete(&mut self, cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            let id = p.id.clone();
            cfg.config.providers.retain(|x| x.id != id);
            // 若删掉的是当前激活的，清空 provider_id
            if cfg.config.provider_id == id {
                cfg.config.provider_id = cfg.config.providers.first().map(|x| x.id.clone()).unwrap_or_default();
            }
            self.providers = cfg.config.providers.clone();
            if self.cursor >= self.providers.len() && !self.providers.is_empty() {
                self.cursor = self.providers.len() - 1;
            }
        }
        self.mode = ModelPanelMode::Browse;
    }

}
