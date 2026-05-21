//! plugin 子命令实现：list / install / uninstall

use anyhow::Result;

use crate::cli_args::PluginScope;

struct PluginListEntry {
    id: String,
    name: String,
    version: String,
    marketplace: String,
    enabled: bool,
    scope: String,
}

fn load_plugins() -> Vec<PluginListEntry> {
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");
    let plugins_path = claude_dir.join("plugins").join("installed_plugins.json");
    let installed = peri_middlewares::plugin::config::load_installed_plugins(Some(&plugins_path))
        .unwrap_or_default();

    installed
        .plugins
        .into_iter()
        .map(|p| PluginListEntry {
            id: p.id,
            name: p.name,
            version: p.version,
            marketplace: p.marketplace,
            enabled: true,
            scope: match p.scope {
                peri_middlewares::plugin::InstallScope::User => "user",
                peri_middlewares::plugin::InstallScope::Project => "project",
                peri_middlewares::plugin::InstallScope::Local => "local",
            }
            .to_string(),
        })
        .collect()
}

pub fn run_plugin_list(json: bool) -> Result<()> {
    let entries = load_plugins();

    if json {
        let json_entries: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "name": e.name,
                    "version": e.version,
                    "marketplace": e.marketplace,
                    "enabled": e.enabled,
                    "scope": e.scope,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_entries)?);
    } else if entries.is_empty() {
        println!("未安装任何插件。");
    } else {
        println!("{:<40} {:<10} {:<15} {}", "ID", "版本", "市场", "状态");
        println!("{}", "-".repeat(80));
        for e in &entries {
            let status = if e.enabled { "已启用" } else { "已禁用" };
            println!(
                "{:<40} {:<10} {:<15} {}",
                e.id, e.version, e.marketplace, status
            );
        }
    }
    Ok(())
}

pub async fn run_plugin_install(plugin_name: &str, scope_str: &str) -> Result<()> {
    let scope: PluginScope = scope_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");
    let cache_dir = peri_middlewares::plugin::config::marketplaces_cache_dir();

    let (name, marketplace) = plugin_name
        .split_once('@')
        .unwrap_or((plugin_name, "claude-plugins-official"));

    let result = peri_middlewares::plugin::install_plugin(
        name,
        marketplace,
        scope.into(),
        &cache_dir,
        &claude_dir,
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("安装失败: {e}"))?;

    println!(
        "已安装: {} v{} (scope: {})",
        result.id, result.version, scope_str
    );
    Ok(())
}

pub async fn run_plugin_uninstall(plugin_id: &str, _scope_str: Option<&str>) -> Result<()> {
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");

    peri_middlewares::plugin::uninstall_plugin(plugin_id, &claude_dir, None)
        .await
        .map_err(|e| anyhow::anyhow!("卸载失败: {e}"))?;

    println!("已卸载: {}", plugin_id);
    Ok(())
}
