use std::path::PathBuf;
use anyhow::Result;
use super::types::ZenConfig;

/// 配置文件路径：~/.zen-code/settings.json
pub fn config_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zen-code")
        .join("settings.json")
}

/// 加载配置，文件不存在时返回默认空配置
pub fn load() -> Result<ZenConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(ZenConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let cfg: ZenConfig = serde_json::from_str(&content)?;
    Ok(cfg)
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
