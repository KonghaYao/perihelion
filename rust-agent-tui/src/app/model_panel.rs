use crate::config::{ThinkingConfig, ZenConfig};

// ─── AliasTab 枚举 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AliasTab {
    Opus,
    Sonnet,
    Haiku,
}

impl AliasTab {
    pub fn label(&self) -> &str {
        match self {
            Self::Opus => "Opus",
            Self::Sonnet => "Sonnet",
            Self::Haiku => "Haiku",
        }
    }

    pub fn to_key(&self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Sonnet => "sonnet",
            Self::Haiku => "haiku",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Opus => "Most capable for complex work",
            Self::Sonnet => "Balanced performance and speed",
            Self::Haiku => "Fastest for quick answers",
        }
    }
}

// ─── 行索引常量 ─────────────────────────────────────────────────────────────────

pub const ROW_OPUS: usize = 0;
pub const ROW_SONNET: usize = 1;
pub const ROW_HAIKU: usize = 2;
pub const ROW_EFFORT: usize = 3;
pub const ROW_COUNT: usize = 4;

// ─── ModelPanel ─────────────────────────────────────────────────────────────────

pub struct ModelPanel {
    /// 当前激活 Provider 的显示名称
    pub provider_name: String,
    /// 竖向列表光标 (0..ROW_COUNT)
    pub cursor: usize,
    /// 当前选中的级别
    pub active_tab: AliasTab,
    /// Thinking effort 缓冲 "low" / "medium" / "high"
    pub buf_thinking_effort: String,
}

impl ModelPanel {
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let active_tab = match cfg.config.active_alias.as_str() {
            "sonnet" => AliasTab::Sonnet,
            "haiku" => AliasTab::Haiku,
            _ => AliasTab::Opus,
        };

        let provider_name = cfg
            .config
            .providers
            .iter()
            .find(|p| p.id == cfg.config.active_provider_id)
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();

        let cursor = match active_tab {
            AliasTab::Opus => ROW_OPUS,
            AliasTab::Sonnet => ROW_SONNET,
            AliasTab::Haiku => ROW_HAIKU,
        };

        let effort = cfg
            .config
            .thinking
            .as_ref()
            .map(|t| t.effort.clone())
            .unwrap_or_else(|| "high".to_string());

        Self {
            provider_name,
            cursor,
            active_tab,
            buf_thinking_effort: effort,
        }
    }

    /// 上下移动光标（循环）
    pub fn move_cursor(&mut self, delta: i32) {
        if delta > 0 {
            self.cursor = (self.cursor + 1) % ROW_COUNT;
        } else if delta < 0 {
            self.cursor = (self.cursor + ROW_COUNT - 1) % ROW_COUNT;
        }
    }

    /// 循环切换 effort（仅 Effort 行）：medium → high → low → medium
    pub fn cycle_effort(&mut self, reverse: bool) {
        if self.cursor != ROW_EFFORT {
            return;
        }
        if reverse {
            self.buf_thinking_effort = match self.buf_thinking_effort.as_str() {
                "low" => "high".to_string(),
                "high" => "medium".to_string(),
                _ => "low".to_string(),
            };
        } else {
            self.buf_thinking_effort = match self.buf_thinking_effort.as_str() {
                "low" => "medium".to_string(),
                "medium" => "high".to_string(),
                _ => "low".to_string(),
            };
        }
    }

    /// 将面板状态写入 ZenConfig（alias + thinking）
    pub fn apply_to_config(&self, cfg: &mut ZenConfig) {
        cfg.config.active_alias = self.active_tab.to_key().to_string();
        let t = cfg.config.thinking.get_or_insert_with(|| ThinkingConfig {
            enabled: true,
            budget_tokens: 8000,
            effort: self.buf_thinking_effort.clone(),
        });
        t.enabled = true;
        t.effort = self.buf_thinking_effort.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::AppConfig;
    use crate::config::ProviderConfig;

    fn make_config() -> ZenConfig {
        ZenConfig {
            config: AppConfig {
                active_alias: "opus".to_string(),
                active_provider_id: "test".to_string(),
                providers: vec![ProviderConfig {
                    id: "test".to_string(),
                    name: Some("TestProvider".to_string()),
                    ..Default::default()
                }],
                thinking: Some(ThinkingConfig {
                    enabled: false,
                    budget_tokens: 8000,
                    effort: "medium".to_string(),
                }),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_from_config_defaults() {
        let cfg = make_config();
        let panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.active_tab, AliasTab::Opus);
        assert_eq!(panel.cursor, ROW_OPUS);
        assert_eq!(panel.provider_name, "TestProvider");
        assert_eq!(panel.buf_thinking_effort, "medium");
    }

    #[test]
    fn test_from_config_sonnet() {
        let mut cfg = make_config();
        cfg.config.active_alias = "sonnet".to_string();
        let panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.active_tab, AliasTab::Sonnet);
        assert_eq!(panel.cursor, ROW_SONNET);
    }

    #[test]
    fn test_move_cursor_wrap() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_SONNET);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_HAIKU);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_EFFORT);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.move_cursor(-1);
        assert_eq!(panel.cursor, ROW_EFFORT);
    }

    #[test]
    fn test_cycle_effort() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.cursor = ROW_EFFORT;

        assert_eq!(panel.buf_thinking_effort, "medium");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "high");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "low");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "medium");

        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "low");
        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "high");
    }

    #[test]
    fn test_cycle_effort_ignored_on_model_rows() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "medium");
    }

    #[test]
    fn test_apply_to_config() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.active_tab = AliasTab::Sonnet;
        panel.buf_thinking_effort = "high".to_string();

        let mut cfg2 = make_config();
        panel.apply_to_config(&mut cfg2);
        assert_eq!(cfg2.config.active_alias, "sonnet");
        assert!(cfg2.config.thinking.as_ref().unwrap().enabled);
        assert_eq!(cfg2.config.thinking.as_ref().unwrap().effort, "high");
    }

    #[test]
    fn test_apply_to_config_creates_thinking_when_none() {
        let mut cfg = ZenConfig {
            config: AppConfig {
                active_alias: "opus".to_string(),
                active_provider_id: "test".to_string(),
                providers: vec![ProviderConfig {
                    id: "test".to_string(),
                    ..Default::default()
                }],
                thinking: None,
                ..Default::default()
            },
        };
        let panel = ModelPanel::from_config(&cfg);
        panel.apply_to_config(&mut cfg);
        let t = cfg.config.thinking.as_ref().unwrap();
        assert!(t.enabled);
        assert_eq!(t.effort, "high");
    }

    #[test]
    fn test_alias_tab_description() {
        assert_eq!(AliasTab::Opus.description(), "Most capable for complex work");
        assert_eq!(AliasTab::Sonnet.description(), "Balanced performance and speed");
        assert_eq!(AliasTab::Haiku.description(), "Fastest for quick answers");
    }
}
