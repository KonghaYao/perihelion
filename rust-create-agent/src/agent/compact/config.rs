use serde::{Deserialize, Serialize};
use std::env;

const DEFAULT_COMPACTABLE_TOOLS: &[&str] = &[
    "bash",
    "read_file",
    "glob_files",
    "search_files_rg",
    "write_file",
    "edit_file",
];

fn default_true() -> bool {
    true
}
fn default_threshold_085() -> f64 {
    0.85
}
fn default_threshold_070() -> f64 {
    0.70
}
fn default_stale_steps() -> usize {
    5
}
fn default_compactable_tools() -> Vec<String> {
    DEFAULT_COMPACTABLE_TOOLS
        .iter()
        .map(|s| s.to_string())
        .collect()
}
fn default_summary_max_tokens() -> u32 {
    16000
}
fn default_re_inject_max_files() -> usize {
    5
}
fn default_re_inject_max_tokens_per_file() -> u32 {
    5000
}
fn default_re_inject_file_budget() -> u32 {
    25000
}
fn default_re_inject_skills_budget() -> u32 {
    25000
}
fn default_max_consecutive_failures() -> u32 {
    3
}
fn default_ptl_max_retries() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    #[serde(default = "default_true")]
    pub auto_compact_enabled: bool,
    #[serde(default = "default_threshold_085")]
    pub auto_compact_threshold: f64,
    #[serde(default = "default_threshold_070")]
    pub micro_compact_threshold: f64,
    #[serde(default = "default_stale_steps")]
    pub micro_compact_stale_steps: usize,
    #[serde(default = "default_compactable_tools")]
    pub micro_compactable_tools: Vec<String>,
    #[serde(default = "default_summary_max_tokens")]
    pub summary_max_tokens: u32,
    #[serde(default = "default_re_inject_max_files")]
    pub re_inject_max_files: usize,
    #[serde(default = "default_re_inject_max_tokens_per_file")]
    pub re_inject_max_tokens_per_file: u32,
    #[serde(default = "default_re_inject_file_budget")]
    pub re_inject_file_budget: u32,
    #[serde(default = "default_re_inject_skills_budget")]
    pub re_inject_skills_budget: u32,
    #[serde(default = "default_max_consecutive_failures")]
    pub max_consecutive_failures: u32,
    #[serde(default = "default_ptl_max_retries")]
    pub ptl_max_retries: u32,
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            auto_compact_enabled: true,
            auto_compact_threshold: 0.85,
            micro_compact_threshold: 0.70,
            micro_compact_stale_steps: 5,
            micro_compactable_tools: default_compactable_tools(),
            summary_max_tokens: 16000,
            re_inject_max_files: 5,
            re_inject_max_tokens_per_file: 5000,
            re_inject_file_budget: 25000,
            re_inject_skills_budget: 25000,
            max_consecutive_failures: 3,
            ptl_max_retries: 3,
        }
    }
}

impl CompactConfig {
    /// 从环境变量构建配置，未设置的环境变量使用默认值
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if env::var("DISABLE_COMPACT").is_ok() {
            config.auto_compact_enabled = false;
            config.micro_compact_threshold = 1.0;
        }
        if env::var("DISABLE_AUTO_COMPACT").is_ok() {
            config.auto_compact_enabled = false;
        }
        if let Ok(val) = env::var("COMPACT_THRESHOLD") {
            if let Ok(threshold) = val.parse::<f64>() {
                if (0.0..=1.0).contains(&threshold) {
                    config.auto_compact_threshold = threshold;
                }
            }
        }
        config
    }

    /// 在已有配置基础上应用环境变量覆盖
    pub fn apply_env_overrides(&mut self) {
        if env::var("DISABLE_COMPACT").is_ok() {
            self.auto_compact_enabled = false;
            self.micro_compact_threshold = 1.0;
        }
        if env::var("DISABLE_AUTO_COMPACT").is_ok() {
            self.auto_compact_enabled = false;
        }
        if let Ok(val) = env::var("COMPACT_THRESHOLD") {
            if let Ok(threshold) = val.parse::<f64>() {
                if (0.0..=1.0).contains(&threshold) {
                    self.auto_compact_threshold = threshold;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_values() {
        let config = CompactConfig::default();
        assert!(config.auto_compact_enabled);
        assert!((config.auto_compact_threshold - 0.85).abs() < 0.001);
        assert!((config.micro_compact_threshold - 0.70).abs() < 0.001);
        assert_eq!(config.micro_compact_stale_steps, 5);
        assert_eq!(config.micro_compactable_tools.len(), 6);
        assert!(config.micro_compactable_tools.contains(&"bash".to_string()));
        assert!(config
            .micro_compactable_tools
            .contains(&"read_file".to_string()));
        assert_eq!(config.summary_max_tokens, 16000);
        assert_eq!(config.re_inject_max_files, 5);
        assert_eq!(config.re_inject_max_tokens_per_file, 5000);
        assert_eq!(config.re_inject_file_budget, 25000);
        assert_eq!(config.re_inject_skills_budget, 25000);
        assert_eq!(config.max_consecutive_failures, 3);
        assert_eq!(config.ptl_max_retries, 3);
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = CompactConfig {
            auto_compact_threshold: 0.90,
            micro_compact_stale_steps: 10,
            summary_max_tokens: 8000,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CompactConfig = serde_json::from_str(&json).unwrap();
        assert!((deserialized.auto_compact_threshold - 0.90).abs() < 0.001);
        assert_eq!(deserialized.micro_compact_stale_steps, 10);
        assert_eq!(deserialized.summary_max_tokens, 8000);
        assert!((deserialized.micro_compact_threshold - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_serde_partial_deserialize() {
        let json = r#"{"auto_compact_threshold": 0.90}"#;
        let config: CompactConfig = serde_json::from_str(json).unwrap();
        assert!((config.auto_compact_threshold - 0.90).abs() < 0.001);
        assert!(config.auto_compact_enabled);
        assert!((config.micro_compact_threshold - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_serde_empty_object() {
        let json = "{}";
        let config: CompactConfig = serde_json::from_str(json).unwrap();
        assert!(config.auto_compact_enabled);
        assert!((config.auto_compact_threshold - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_from_env_disable_compact() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::remove_var("DISABLE_AUTO_COMPACT");
        env::remove_var("COMPACT_THRESHOLD");
        env::set_var("DISABLE_COMPACT", "1");
        let config = CompactConfig::from_env();
        env::remove_var("DISABLE_COMPACT");
        assert!(!config.auto_compact_enabled);
        assert!((config.micro_compact_threshold - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_from_env_disable_auto_compact() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::remove_var("DISABLE_COMPACT");
        env::remove_var("COMPACT_THRESHOLD");
        env::set_var("DISABLE_AUTO_COMPACT", "1");
        let config = CompactConfig::from_env();
        env::remove_var("DISABLE_AUTO_COMPACT");
        assert!(!config.auto_compact_enabled);
        assert!((config.micro_compact_threshold - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_from_env_compact_threshold() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::remove_var("DISABLE_COMPACT");
        env::remove_var("DISABLE_AUTO_COMPACT");
        env::set_var("COMPACT_THRESHOLD", "0.75");
        let config = CompactConfig::from_env();
        env::remove_var("COMPACT_THRESHOLD");
        assert!((config.auto_compact_threshold - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_from_env_compact_threshold_invalid() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::remove_var("DISABLE_COMPACT");
        env::remove_var("DISABLE_AUTO_COMPACT");
        env::set_var("COMPACT_THRESHOLD", "abc");
        let config = CompactConfig::from_env();
        env::remove_var("COMPACT_THRESHOLD");
        assert!((config.auto_compact_threshold - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_from_env_compact_threshold_out_of_range() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::remove_var("DISABLE_COMPACT");
        env::remove_var("DISABLE_AUTO_COMPACT");
        env::set_var("COMPACT_THRESHOLD", "1.5");
        let config = CompactConfig::from_env();
        env::remove_var("COMPACT_THRESHOLD");
        assert!((config.auto_compact_threshold - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_apply_env_overrides_on_custom_config() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::remove_var("DISABLE_COMPACT");
        env::remove_var("DISABLE_AUTO_COMPACT");
        env::set_var("COMPACT_THRESHOLD", "0.80");
        let mut config = CompactConfig {
            auto_compact_threshold: 0.90,
            ..Default::default()
        };
        config.apply_env_overrides();
        env::remove_var("COMPACT_THRESHOLD");
        assert!((config.auto_compact_threshold - 0.80).abs() < 0.001);
    }

    #[test]
    fn test_compactable_tools_default_content() {
        let config = CompactConfig::default();
        assert_eq!(
            config.micro_compactable_tools,
            vec![
                "bash".to_string(),
                "read_file".to_string(),
                "glob_files".to_string(),
                "search_files_rg".to_string(),
                "write_file".to_string(),
                "edit_file".to_string(),
            ]
        );
    }
}
