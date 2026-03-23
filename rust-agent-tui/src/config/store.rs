use std::path::PathBuf;
use anyhow::Result;
use super::types::{ModelAliasConfig, ZenConfig};

/// 配置文件路径：~/.zen-code/settings.json
pub fn config_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zen-code")
        .join("settings.json")
}

/// 检测旧格式并迁移：若 opus.provider_id 为空，但旧字段 provider_id 不为空，则迁移
fn migrate_if_needed(cfg: &mut ZenConfig) -> bool {
    let has_old = !cfg.config.provider_id.is_empty();
    let has_new = !cfg.config.model_aliases.opus.provider_id.is_empty();
    if has_old && !has_new {
        let old_provider = cfg.config.provider_id.clone();
        let old_model = cfg.config.model_id.clone();
        cfg.config.model_aliases.opus = ModelAliasConfig {
            provider_id: old_provider.clone(),
            model_id: old_model,
        };
        cfg.config.model_aliases.sonnet = ModelAliasConfig {
            provider_id: old_provider.clone(),
            model_id: String::new(),
        };
        cfg.config.model_aliases.haiku = ModelAliasConfig {
            provider_id: old_provider,
            model_id: String::new(),
        };
        cfg.config.active_alias = "opus".to_string();
        return true;
    }
    false
}

/// 加载配置，文件不存在时返回默认空配置；检测到旧格式时自动迁移并写回
pub fn load() -> Result<ZenConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(ZenConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let mut cfg: ZenConfig = serde_json::from_str(&content)?;
    if migrate_if_needed(&mut cfg) {
        let _ = save(&cfg);
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ZenConfig;

    #[test]
    fn test_migration_from_old_format() {
        // 构造旧格式配置（有 provider_id/model_id，无 model_aliases）
        let mut cfg = ZenConfig::default();
        cfg.config.provider_id = "test-provider".to_string();
        cfg.config.model_id = "gpt-4o".to_string();

        let migrated = migrate_if_needed(&mut cfg);
        assert!(migrated, "应检测到旧格式并返回 true");

        assert_eq!(cfg.config.model_aliases.opus.provider_id, "test-provider");
        assert_eq!(cfg.config.model_aliases.opus.model_id, "gpt-4o");
        assert_eq!(cfg.config.model_aliases.sonnet.provider_id, "test-provider");
        assert_eq!(cfg.config.model_aliases.sonnet.model_id, "");
        assert_eq!(cfg.config.model_aliases.haiku.provider_id, "test-provider");
        assert_eq!(cfg.config.model_aliases.haiku.model_id, "");
        assert_eq!(cfg.config.active_alias, "opus");
    }

    #[test]
    fn test_no_migration_when_new_format() {
        // 已有新格式配置，不应触发迁移
        let mut cfg = ZenConfig::default();
        cfg.config.model_aliases.opus.provider_id = "anthropic".to_string();
        cfg.config.model_aliases.opus.model_id = "claude-opus-4-6".to_string();

        let migrated = migrate_if_needed(&mut cfg);
        assert!(!migrated, "新格式不应触发迁移");
    }

    #[test]
    fn test_migration_active_alias_is_opus() {
        let mut cfg = ZenConfig::default();
        cfg.config.provider_id = "openrouter".to_string();
        cfg.config.model_id = "gpt-5.4".to_string();

        migrate_if_needed(&mut cfg);
        assert_eq!(cfg.config.active_alias, "opus");
    }
}

/// 原子写回配置文件（先写临时文件，再 rename，避免写入中断导致文件损坏）
pub fn save(cfg: &ZenConfig) -> Result<()> {
    let path = config_path();

    // 确保目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(cfg)?;

    // atomic write
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, &path)?;

    Ok(())
}
