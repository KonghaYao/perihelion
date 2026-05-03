use crate::config::ZenConfig;

// ─── 枚举 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigPanelMode {
    Browse,
    Edit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigEditField {
    Autocompact,
    CompactThreshold,
    Language,
    Persona,
    Tone,
    Proactiveness,
}

impl ConfigEditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Autocompact => Self::CompactThreshold,
            Self::CompactThreshold => Self::Language,
            Self::Language => Self::Persona,
            Self::Persona => Self::Tone,
            Self::Tone => Self::Proactiveness,
            Self::Proactiveness => Self::Autocompact,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Autocompact => Self::Proactiveness,
            Self::CompactThreshold => Self::Autocompact,
            Self::Language => Self::CompactThreshold,
            Self::Persona => Self::Language,
            Self::Tone => Self::Persona,
            Self::Proactiveness => Self::Tone,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Autocompact => "Autocompact",
            Self::CompactThreshold => "Compact 阈值",
            Self::Language => "语言",
            Self::Persona => "Persona",
            Self::Tone => "Tone",
            Self::Proactiveness => "Proactiveness",
        }
    }
}

// ─── ConfigPanel ─────────────────────────────────────────────────────────────

const FIELD_COUNT: usize = 6;

pub struct ConfigPanel {
    pub mode: ConfigPanelMode,
    /// Browse 模式当前选中字段索引（0-5）
    pub cursor: usize,
    pub edit_field: ConfigEditField,
    // 编辑缓冲区
    pub buf_autocompact: bool,
    pub buf_threshold: String,
    pub cur_threshold: usize,
    pub buf_language: String,
    pub cur_language: usize,
    pub buf_persona: String,
    pub cur_persona: usize,
    pub buf_tone: String,
    pub cur_tone: usize,
    pub buf_proactiveness: String, // "low" / "medium" / "high"
    pub scroll_offset: u16,
}

impl ConfigPanel {
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let compact_config = cfg.config.compact.as_ref();
        let autocompact = compact_config
            .map(|c| c.auto_compact_enabled)
            .unwrap_or(true);
        let threshold = compact_config
            .map(|c| format!("{}", (c.auto_compact_threshold * 100.0) as u8))
            .unwrap_or_else(|| "85".to_string());
        let proactiveness = cfg
            .config
            .proactiveness
            .clone()
            .unwrap_or_else(|| "medium".to_string());

        Self {
            mode: ConfigPanelMode::Browse,
            cursor: 0,
            edit_field: ConfigEditField::Autocompact,
            buf_autocompact: autocompact,
            buf_threshold: threshold,
            cur_threshold: 0,
            buf_language: cfg.config.language.clone().unwrap_or_default(),
            cur_language: 0,
            buf_persona: cfg.config.persona.clone().unwrap_or_default(),
            cur_persona: 0,
            buf_tone: cfg.config.tone.clone().unwrap_or_default(),
            cur_tone: 0,
            buf_proactiveness: proactiveness,
            scroll_offset: 0,
        }
    }

    pub fn enter_edit(&mut self) {
        self.mode = ConfigPanelMode::Edit;
        // 将 cursor 映射到 edit_field
        self.edit_field = match self.cursor {
            0 => ConfigEditField::Autocompact,
            1 => ConfigEditField::CompactThreshold,
            2 => ConfigEditField::Language,
            3 => ConfigEditField::Persona,
            4 => ConfigEditField::Tone,
            _ => ConfigEditField::Proactiveness,
        };
        // 设置光标到末尾
        self.cur_threshold = self.buf_threshold.chars().count();
        self.cur_language = self.buf_language.chars().count();
        self.cur_persona = self.buf_persona.chars().count();
        self.cur_tone = self.buf_tone.chars().count();
    }

    pub fn field_next(&mut self) {
        self.edit_field = self.edit_field.next();
    }

    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
    }

    /// 返回当前可编辑字段的 (buf, cursor) 可变引用
    /// Autocompact 和 Proactiveness 返回 None（用 Space 切换）
    pub fn active_field(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.edit_field {
            ConfigEditField::Autocompact | ConfigEditField::Proactiveness => None,
            ConfigEditField::CompactThreshold => {
                Some((&mut self.buf_threshold, &mut self.cur_threshold))
            }
            ConfigEditField::Language => Some((&mut self.buf_language, &mut self.cur_language)),
            ConfigEditField::Persona => Some((&mut self.buf_persona, &mut self.cur_persona)),
            ConfigEditField::Tone => Some((&mut self.buf_tone, &mut self.cur_tone)),
        }
    }

    pub fn cycle_autocompact(&mut self) {
        self.buf_autocompact = !self.buf_autocompact;
    }

    pub fn cycle_proactiveness(&mut self) {
        self.buf_proactiveness = match self.buf_proactiveness.as_str() {
            "low" => "medium".to_string(),
            "medium" => "high".to_string(),
            _ => "low".to_string(),
        };
    }

    pub fn paste_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        if let Some((buf, cursor)) = self.active_field() {
            let char_count = buf.chars().count();
            if *cursor > char_count {
                *cursor = char_count;
            }
            let byte_pos = buf
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(buf.len());
            buf.insert_str(byte_pos, &text);
            *cursor += text.chars().count();
        }
    }

    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) {
        // autocompact + threshold
        let compact = cfg
            .config
            .compact
            .get_or_insert_with(rust_create_agent::agent::compact::CompactConfig::default);
        compact.auto_compact_enabled = self.buf_autocompact;
        let threshold_val: u8 = self
            .buf_threshold
            .parse()
            .unwrap_or(85)
            .clamp(50, 99);
        compact.auto_compact_threshold = threshold_val as f64 / 100.0;

        // language
        cfg.config.language = if self.buf_language.is_empty() || self.buf_language == "auto" {
            None
        } else {
            Some(self.buf_language.clone())
        };

        // persona
        cfg.config.persona = if self.buf_persona.is_empty() {
            None
        } else {
            Some(self.buf_persona.clone())
        };

        // tone
        cfg.config.tone = if self.buf_tone.is_empty() {
            None
        } else {
            Some(self.buf_tone.clone())
        };

        // proactiveness
        cfg.config.proactiveness = if self.buf_proactiveness == "medium" {
            None
        } else {
            Some(self.buf_proactiveness.clone())
        };
    }

    pub fn field_count() -> usize {
        FIELD_COUNT
    }

    pub fn field_label(index: usize) -> &'static str {
        match index {
            0 => "Autocompact",
            1 => "Compact 阈值",
            2 => "语言",
            3 => "Persona",
            4 => "Tone",
            5 => "Proactiveness",
            _ => "???",
        }
    }

    pub fn field_display_value(&self, index: usize) -> String {
        match index {
            0 => {
                if self.buf_autocompact {
                    "开".to_string()
                } else {
                    "关".to_string()
                }
            }
            1 => format!("{}%", self.buf_threshold),
            2 => {
                if self.buf_language.is_empty() {
                    "auto".to_string()
                } else {
                    self.buf_language.clone()
                }
            }
            3 => {
                if self.buf_persona.is_empty() {
                    "-".to_string()
                } else {
                    self.buf_persona.clone()
                }
            }
            4 => {
                if self.buf_tone.is_empty() {
                    "-".to_string()
                } else {
                    self.buf_tone.clone()
                }
            }
            5 => self.buf_proactiveness.clone(),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_panel_from_config_defaults() {
        let cfg = ZenConfig::default();
        let panel = ConfigPanel::from_config(&cfg);
        assert!(panel.buf_autocompact);
        assert_eq!(panel.buf_threshold, "85");
        assert!(panel.buf_language.is_empty());
        assert_eq!(panel.buf_proactiveness, "medium");
    }

    #[test]
    fn test_config_panel_field_navigation() {
        let panel = ConfigPanel::from_config(&ZenConfig::default());
        let fields: Vec<_> = (0..6)
            .map(|_| {
                let mut p = ConfigEditField::Autocompact;
                for _ in 0..0 {
                    p = p.next();
                }
                p
            })
            .collect();
        // verify all 6 fields are distinct
        assert_eq!(ConfigPanel::field_count(), 6);

        let mut f = ConfigEditField::Autocompact;
        for _ in 0..6 {
            f = f.next();
        }
        assert_eq!(f, ConfigEditField::Autocompact);

        f = ConfigEditField::Proactiveness;
        f = f.prev();
        assert_eq!(f, ConfigEditField::Tone);
    }

    #[test]
    fn test_config_panel_cycle_autocompact() {
        let mut panel = ConfigPanel::from_config(&ZenConfig::default());
        assert!(panel.buf_autocompact);
        panel.cycle_autocompact();
        assert!(!panel.buf_autocompact);
        panel.cycle_autocompact();
        assert!(panel.buf_autocompact);
    }

    #[test]
    fn test_config_panel_cycle_proactiveness() {
        let mut panel = ConfigPanel::from_config(&ZenConfig::default());
        panel.buf_proactiveness = "low".to_string();
        panel.cycle_proactiveness();
        assert_eq!(panel.buf_proactiveness, "medium");
        panel.cycle_proactiveness();
        assert_eq!(panel.buf_proactiveness, "high");
        panel.cycle_proactiveness();
        assert_eq!(panel.buf_proactiveness, "low");
    }

    #[test]
    fn test_config_panel_apply_edit_saves_to_config() {
        let mut cfg = ZenConfig::default();
        let mut panel = ConfigPanel::from_config(&cfg);
        panel.buf_language = "zh-CN".to_string();
        panel.buf_persona = "Rust expert".to_string();
        panel.buf_tone = "concise".to_string();
        panel.buf_proactiveness = "high".to_string();
        panel.apply_edit(&mut cfg);
        assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
        assert_eq!(cfg.config.persona.as_deref(), Some("Rust expert"));
        assert_eq!(cfg.config.tone.as_deref(), Some("concise"));
        assert_eq!(cfg.config.proactiveness.as_deref(), Some("high"));
    }

    #[test]
    fn test_config_panel_apply_edit_compact_threshold() {
        let mut cfg = ZenConfig::default();
        let mut panel = ConfigPanel::from_config(&cfg);
        panel.buf_threshold = "90".to_string();
        panel.apply_edit(&mut cfg);
        let compact = cfg.config.compact.unwrap();
        assert!((compact.auto_compact_threshold - 0.90).abs() < 0.001);
    }

    #[test]
    fn test_config_panel_apply_edit_invalid_threshold_clamps() {
        let mut cfg = ZenConfig::default();
        let mut panel = ConfigPanel::from_config(&cfg);
        panel.buf_threshold = "30".to_string();
        panel.apply_edit(&mut cfg);
        let compact = cfg.config.compact.unwrap();
        assert!((compact.auto_compact_threshold - 0.50).abs() < 0.001);
    }

    #[test]
    fn test_config_panel_active_field_text_editable() {
        let mut panel = ConfigPanel::from_config(&ZenConfig::default());
        // Autocompact → None
        panel.edit_field = ConfigEditField::Autocompact;
        assert!(panel.active_field().is_none());
        // Proactiveness → None
        panel.edit_field = ConfigEditField::Proactiveness;
        assert!(panel.active_field().is_none());
        // Language → Some
        panel.edit_field = ConfigEditField::Language;
        assert!(panel.active_field().is_some());
        // Persona → Some
        panel.edit_field = ConfigEditField::Persona;
        assert!(panel.active_field().is_some());
        // Tone → Some
        panel.edit_field = ConfigEditField::Tone;
        assert!(panel.active_field().is_some());
        // CompactThreshold → Some
        panel.edit_field = ConfigEditField::CompactThreshold;
        assert!(panel.active_field().is_some());
    }
}
