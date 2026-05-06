use crate::mcp::config::load_from_path;
use crate::mcp::McpServerConfig;
use crate::plugin::config::{load_claude_settings, load_installed_plugins, load_plugin_manifest};
use crate::plugin::types::{InstalledPlugins, McpServerEntry, PluginManifest};
use gray_matter::{engine::YAML, Matter};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum LoaderError {
    #[error("插件清单加载失败: {0}")]
    ManifestLoadFailed(String),
    #[error("插件配置读取失败: {0}")]
    ConfigError(#[from] crate::plugin::PluginConfigError),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub enum CommandSource {
    Builtin,
    Plugin { path: PathBuf },
}

#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub source: CommandSource,
}

pub trait CommandProvider: Send + Sync {
    fn commands(&self) -> Vec<CommandEntry>;
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct CommandFrontmatter {
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    args: Option<serde_yaml::Value>,
}

pub fn parse_command_md(path: &Path) -> Option<(CommandFrontmatter, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    let matter = Matter::<YAML>::new();
    let result: gray_matter::ParsedEntity = matter.parse(&content).ok()?;
    let fm: CommandFrontmatter = match result.data {
        Some(data) => data.deserialize().ok()?,
        None => CommandFrontmatter::default(),
    };
    Some((fm, result.content))
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub name: String,
    pub version: String,
    pub install_path: PathBuf,
    pub manifest: PluginManifest,
    pub commands: Vec<CommandEntry>,
    pub skills_dirs: Vec<PathBuf>,
    pub agents_dirs: Vec<PathBuf>,
    pub mcp_servers: HashMap<String, McpServerConfig>,
    /// 插件数据目录（install_path/.claude-plugin/data），供 ${CLAUDE_PLUGIN_DATA} 展开
    pub data_path: PathBuf,
}

pub fn load_manifest(plugin_dir: &Path) -> Result<PluginManifest, LoaderError> {
    load_plugin_manifest(plugin_dir)
        .map_err(|e| LoaderError::ManifestLoadFailed(format!("{}: {e}", plugin_dir.display())))
}

pub fn extract_commands(
    manifest: &PluginManifest,
    base_dir: &Path,
    plugin_name: &str,
) -> Vec<CommandEntry> {
    let commands = match &manifest.commands {
        Some(cmds) if !cmds.is_empty() => cmds,
        _ => return Vec::new(),
    };

    let mut result = Vec::new();
    for cmd in commands {
        let cmd_file_path = base_dir.join(&cmd.path);
        if !cmd_file_path.exists() {
            warn!(path = %cmd_file_path.display(), "插件命令文件不存在，跳过");
            continue;
        }

        let (fm, _body) = match parse_command_md(&cmd_file_path) {
            Some(parsed) => parsed,
            None => {
                warn!(path = %cmd_file_path.display(), "插件命令文件解析失败，跳过");
                continue;
            }
        };

        let cmd_name = cmd.name.as_deref().unwrap_or_else(|| {
            cmd_file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        });

        let full_name = format!("{plugin_name}:{cmd_name}");
        let description = fm
            .description
            .or(cmd.description.as_deref().map(String::from))
            .unwrap_or_default();

        result.push(CommandEntry {
            name: full_name,
            description,
            source: CommandSource::Plugin {
                path: cmd_file_path,
            },
        });
    }
    result
}

pub fn extract_skills_paths(manifest: &PluginManifest, base_dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();

    // 1. manifest 显式声明
    if let Some(skills) = &manifest.skills {
        if !skills.is_empty() {
            for skill_name in skills {
                let skill_path = base_dir.join("skills").join(skill_name);
                if skill_path.is_dir() {
                    result.push(skill_path);
                } else {
                    debug!(path = %skill_path.display(), "插件 skill 目录不存在，跳过");
                }
            }
            return result;
        }
    }

    // 2. fallback：扫描 base_dir/skills/ 下所有含 SKILL.md 的子目录
    let skills_dir = base_dir.join("skills");
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() && entry.path().join("SKILL.md").exists() {
                result.push(entry.path());
            }
        }
    }

    result
}

pub fn extract_agents_paths(manifest: &PluginManifest, base_dir: &Path) -> Vec<PathBuf> {
    let agents = match &manifest.agents {
        Some(a) if !a.is_empty() => a,
        _ => return Vec::new(),
    };

    let mut result = Vec::new();
    for agent in agents {
        let agent_path = base_dir.join(&agent.path);
        if agent_path.exists() {
            result.push(agent_path);
        } else {
            debug!(path = %agent_path.display(), "插件 agent 路径不存在，跳过");
        }
    }
    result
}

/// Extract MCP servers from plugin manifest.
/// Supports inline config objects and .mcp.json file path references.
pub fn extract_mcp_servers(
    manifest: &PluginManifest,
    install_path: &Path,
) -> HashMap<String, McpServerConfig> {
    let entries = match &manifest.mcp_servers {
        Some(servers) => servers,
        None => return HashMap::new(),
    };

    let mut result = HashMap::new();
    for (name, entry) in entries {
        match entry {
            McpServerEntry::Config(cfg) => {
                result.insert(name.clone(), (**cfg).clone());
            }
            McpServerEntry::FilePath(path) => {
                let resolved = install_path.join(path);
                match load_from_path(&resolved) {
                    Ok(file_config) => {
                        for (srv_name, srv_cfg) in file_config.mcp_servers {
                            // 文件路径引用中的服务器名保留，外层会再加命名空间
                            let final_name = if srv_name == *name {
                                // 如果只有一个服务器且与 key 同名，直接使用
                                name.clone()
                            } else {
                                format!("{}.{}", name, srv_name)
                            };
                            result.insert(final_name, srv_cfg);
                        }
                    }
                    Err(e) => {
                        warn!(
                            path = %resolved.display(),
                            error = %e,
                            "插件 MCP 配置文件加载失败，跳过"
                        );
                    }
                }
            }
        }
    }
    result
}

pub fn load_plugins(installed: &InstalledPlugins) -> Result<Vec<LoadedPlugin>, LoaderError> {
    let mut result = Vec::new();

    for plugin in &installed.plugins {
        let manifest = match load_manifest(&plugin.install_path) {
            Ok(m) => m,
            Err(_) => {
                // 静默跳过无法加载的插件（文件被删除或移动）
                continue;
            }
        };

        let commands = extract_commands(&manifest, &plugin.install_path, &plugin.name);
        let skills_dirs = extract_skills_paths(&manifest, &plugin.install_path);
        let agents_dirs = extract_agents_paths(&manifest, &plugin.install_path);
        let mcp_servers = extract_mcp_servers(&manifest, &plugin.install_path);
        let data_path = plugin.install_path.join(".claude-plugin").join("data");

        result.push(LoadedPlugin {
            name: plugin.name.clone(),
            version: plugin.version.clone(),
            install_path: plugin.install_path.clone(),
            manifest,
            commands,
            skills_dirs,
            agents_dirs,
            mcp_servers,
            data_path,
        });
    }

    debug!(count = result.len(), "已加载插件");
    Ok(result)
}

pub fn load_enabled_plugins(claude_dir: &Path) -> Result<Vec<LoadedPlugin>, LoaderError> {
    let plugins_path = claude_dir.join("plugins").join("installed_plugins.json");
    let settings_path = claude_dir.join("settings.json");

    let installed = load_installed_plugins(Some(&plugins_path))?;
    let settings = load_claude_settings(Some(&settings_path))?;

    let enabled_ids: std::collections::HashSet<&str> = settings
        .enabled_plugins
        .iter()
        .map(|s| s.as_str())
        .collect();

    let filtered: Vec<_> = installed
        .plugins
        .into_iter()
        .filter(|p| enabled_ids.contains(p.id.as_str()))
        .collect();

    let filtered_installed = InstalledPlugins {
        version: installed.version,
        plugins: filtered,
    };

    load_plugins(&filtered_installed)
}

pub struct PluginCommandProvider {
    entries: Vec<CommandEntry>,
}

impl PluginCommandProvider {
    pub fn new(plugins: &[LoadedPlugin]) -> Self {
        let entries: Vec<CommandEntry> = plugins.iter().flat_map(|p| p.commands.clone()).collect();
        Self { entries }
    }
}

impl CommandProvider for PluginCommandProvider {
    fn commands(&self) -> Vec<CommandEntry> {
        self.entries.clone()
    }
}

pub fn merge_plugin_mcp_servers(plugins: &[LoadedPlugin]) -> HashMap<String, McpServerConfig> {
    let mut result = HashMap::new();
    for plugin in plugins {
        for (name, config) in &plugin.mcp_servers {
            // 与 Claude Code 一致：使用 plugin:{插件名}:{服务器名} 前缀
            let namespaced = format!("plugin:{}:{}", plugin.name, name);
            result.insert(namespaced, config.clone());
        }
    }
    result
}

/// 所有已启用插件的聚合加载结果
#[derive(Debug, Clone)]
pub struct PluginLoadResult {
    pub plugins: Vec<LoadedPlugin>,
    pub all_skill_dirs: Vec<PathBuf>,
    pub all_mcp_servers: HashMap<String, McpServerConfig>,
    pub all_agent_dirs: Vec<PathBuf>,
    pub all_commands: Vec<CommandEntry>,
}

/// 加载所有已启用插件，返回聚合结果（skills 路径、MCP 服务器、agent 路径、命令列表）
pub fn load_enabled_plugins_aggregated(claude_dir: &Path) -> PluginLoadResult {
    let plugins = match load_enabled_plugins(claude_dir) {
        Ok(p) => p,
        Err(_) => {
            // 静默失败，避免在 TUI 上打印错误日志
            return PluginLoadResult {
                plugins: vec![],
                all_skill_dirs: vec![],
                all_mcp_servers: HashMap::new(),
                all_agent_dirs: vec![],
                all_commands: vec![],
            };
        }
    };

    let all_skill_dirs: Vec<PathBuf> = plugins.iter().flat_map(|p| p.skills_dirs.clone()).collect();

    let all_mcp_servers = merge_plugin_mcp_servers(&plugins);

    let all_agent_dirs: Vec<PathBuf> = plugins.iter().flat_map(|p| p.agents_dirs.clone()).collect();

    let all_commands: Vec<CommandEntry> = plugins.iter().flat_map(|p| p.commands.clone()).collect();

    PluginLoadResult {
        plugins,
        all_skill_dirs,
        all_mcp_servers,
        all_agent_dirs,
        all_commands,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::plugin::types::PluginCommand;
    use crate::plugin::types::{InstallScope, InstalledPlugin, PluginAgent};
    use tempfile::tempdir;

    pub(crate) fn make_manifest_with_commands(commands: Vec<PluginCommand>) -> PluginManifest {
        PluginManifest {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            description: String::new(),
            author: None,
            commands: if commands.is_empty() {
                None
            } else {
                Some(commands)
            },
            agents: None,
            skills: None,
            hooks: None,
            mcp_servers: None,
            lsp_servers: None,
            output_styles: None,
            channels: None,
            options: None,
            settings: None,
        }
    }

    #[test]
    fn test_parse_command_md_with_shell() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cmd.md");
        std::fs::write(&path, "---\nshell: echo hello\n---\nBody content").unwrap();
        let (fm, body) = parse_command_md(&path).unwrap();
        assert_eq!(fm.shell.as_deref(), Some("echo hello"));
        assert_eq!(body.trim(), "Body content");
    }

    #[test]
    fn test_parse_command_md_with_all_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cmd.md");
        std::fs::write(
            &path,
            "---\nshell: echo hi\neffort: low\nmodel: opus\ndescription: Test cmd\nargs:\n  - foo\n---\nBody",
        )
        .unwrap();
        let (fm, _) = parse_command_md(&path).unwrap();
        assert_eq!(fm.shell.as_deref(), Some("echo hi"));
        assert_eq!(fm.effort.as_deref(), Some("low"));
        assert_eq!(fm.model.as_deref(), Some("opus"));
        assert_eq!(fm.description.as_deref(), Some("Test cmd"));
        assert!(fm.args.is_some());
    }

    #[test]
    fn test_parse_command_md_no_frontmatter() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cmd.md");
        std::fs::write(&path, "Just plain markdown").unwrap();
        let (fm, body) = parse_command_md(&path).unwrap();
        assert!(fm.shell.is_none());
        assert_eq!(body, "Just plain markdown");
    }

    #[test]
    fn test_parse_command_md_file_not_found() {
        let result = parse_command_md(Path::new("/nonexistent/cmd.md"));
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_commands_single() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("commands")).unwrap();
        std::fs::write(dir.path().join("commands/test.md"), "---\n---\nContent").unwrap();

        let manifest = make_manifest_with_commands(vec![PluginCommand {
            path: "commands/test.md".into(),
            name: None,
            description: None,
        }]);

        let entries = extract_commands(&manifest, dir.path(), "my-plugin");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my-plugin:test");
    }

    #[test]
    fn test_extract_commands_multiple() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("commands")).unwrap();
        std::fs::write(dir.path().join("commands/a.md"), "---\n---\nA").unwrap();
        std::fs::write(dir.path().join("commands/b.md"), "---\n---\nB").unwrap();

        let manifest = make_manifest_with_commands(vec![
            PluginCommand {
                path: "commands/a.md".into(),
                name: None,
                description: None,
            },
            PluginCommand {
                path: "commands/b.md".into(),
                name: None,
                description: None,
            },
        ]);

        let entries = extract_commands(&manifest, dir.path(), "p");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_extract_commands_missing_file() {
        let manifest = make_manifest_with_commands(vec![PluginCommand {
            path: "commands/missing.md".into(),
            name: None,
            description: None,
        }]);
        let entries = extract_commands(&manifest, Path::new("/tmp"), "p");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_extract_commands_explicit_name() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("commands")).unwrap();
        std::fs::write(dir.path().join("commands/x.md"), "---\n---\nX").unwrap();

        let manifest = make_manifest_with_commands(vec![PluginCommand {
            path: "commands/x.md".into(),
            name: Some("my-cmd".into()),
            description: None,
        }]);

        let entries = extract_commands(&manifest, dir.path(), "p");
        assert_eq!(entries[0].name, "p:my-cmd");
    }

    #[test]
    fn test_extract_commands_none() {
        let manifest = make_manifest_with_commands(vec![]);
        let entries = extract_commands(&manifest, Path::new("/tmp"), "p");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_extract_commands_frontmatter_description() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("commands")).unwrap();
        std::fs::write(
            dir.path().join("commands/x.md"),
            "---\ndescription: FM desc\n---\nBody",
        )
        .unwrap();

        let manifest = make_manifest_with_commands(vec![PluginCommand {
            path: "commands/x.md".into(),
            name: None,
            description: Some("manifest desc".into()),
        }]);

        let entries = extract_commands(&manifest, dir.path(), "p");
        assert_eq!(entries[0].description, "FM desc");
    }

    #[test]
    fn test_extract_skills_paths() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("skills").join("code-review")).unwrap();

        let mut manifest = make_manifest_with_commands(vec![]);
        manifest.skills = Some(vec!["code-review".into()]);

        let paths = extract_skills_paths(&manifest, dir.path());
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("code-review"));
    }

    #[test]
    fn test_extract_skills_paths_missing_dir() {
        let mut manifest = make_manifest_with_commands(vec![]);
        manifest.skills = Some(vec!["nonexistent".into()]);

        let paths = extract_skills_paths(&manifest, Path::new("/tmp"));
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_skills_paths_none() {
        let dir = tempdir().unwrap();
        let manifest = make_manifest_with_commands(vec![]);
        // no skills dir at all → fallback finds nothing
        let paths = extract_skills_paths(&manifest, dir.path());
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_skills_paths_fallback_disk_scan() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skills").join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\n---\nbody").unwrap();

        // manifest has no skills field → fallback to disk scan
        let manifest = make_manifest_with_commands(vec![]);
        let paths = extract_skills_paths(&manifest, dir.path());
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("my-skill"));
    }

    #[test]
    fn test_extract_skills_paths_fallback_ignores_no_skill_md() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skills").join("incomplete");
        std::fs::create_dir_all(&skill_dir).unwrap();
        // no SKILL.md → should be skipped

        let manifest = make_manifest_with_commands(vec![]);
        let paths = extract_skills_paths(&manifest, dir.path());
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_agents_paths() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("agents")).unwrap();
        std::fs::write(dir.path().join("agents/reviewer.md"), "content").unwrap();

        let mut manifest = make_manifest_with_commands(vec![]);
        manifest.agents = Some(vec![PluginAgent {
            path: "agents/reviewer.md".into(),
            name: "reviewer".into(),
        }]);

        let paths = extract_agents_paths(&manifest, dir.path());
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_extract_agents_paths_missing() {
        let mut manifest = make_manifest_with_commands(vec![]);
        manifest.agents = Some(vec![PluginAgent {
            path: "agents/missing.md".into(),
            name: "missing".into(),
        }]);

        let paths = extract_agents_paths(&manifest, Path::new("/tmp"));
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_agents_paths_none() {
        let manifest = make_manifest_with_commands(vec![]);
        let paths = extract_agents_paths(&manifest, Path::new("/tmp"));
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_mcp_servers() {
        let mut manifest = make_manifest_with_commands(vec![]);
        let mut servers = HashMap::new();
        servers.insert(
            "s1".into(),
            McpServerEntry::Config(Box::new(McpServerConfig {
                command: Some("node".into()),
                args: None,
                env: None,
                url: None,
                headers: None,
                oauth: None,
                disabled: None,
                source: None,
            })),
        );
        manifest.mcp_servers = Some(servers);

        let result = extract_mcp_servers(&manifest, Path::new("/tmp"));
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("s1"));
    }

    #[test]
    fn test_extract_mcp_servers_none() {
        let manifest = make_manifest_with_commands(vec![]);
        let result = extract_mcp_servers(&manifest, Path::new("/tmp"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_mcp_servers_file_path_ref() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("my-plugin");
        let servers_dir = plugin_dir.join("servers");
        std::fs::create_dir_all(&servers_dir).unwrap();

        // 创建 .mcp.json 文件
        let mcp_json = r#"{"mcpServers":{"db":{"command":"sqlite3","args":["test.db"]}}}"#;
        std::fs::write(servers_dir.join(".mcp.json"), mcp_json).unwrap();

        let mut manifest = make_manifest_with_commands(vec![]);
        let mut servers = HashMap::new();
        servers.insert(
            "db".into(),
            McpServerEntry::FilePath("servers/.mcp.json".into()),
        );
        manifest.mcp_servers = Some(servers);

        let result = extract_mcp_servers(&manifest, &plugin_dir);
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("db"));
        assert_eq!(result["db"].command.as_deref(), Some("sqlite3"));
    }

    #[test]
    fn test_extract_mcp_servers_file_path_not_found() {
        let dir = tempdir().unwrap();
        let mut manifest = make_manifest_with_commands(vec![]);
        let mut servers = HashMap::new();
        servers.insert(
            "missing".into(),
            McpServerEntry::FilePath("nonexistent/.mcp.json".into()),
        );
        manifest.mcp_servers = Some(servers);

        let result = extract_mcp_servers(&manifest, dir.path());
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_plugin_mcp_servers() {
        let mut p1 = LoadedPlugin {
            name: "plugin-a".into(),
            version: "1.0.0".into(),
            install_path: PathBuf::new(),
            manifest: make_manifest_with_commands(vec![]),
            commands: vec![],
            skills_dirs: vec![],
            agents_dirs: vec![],
            mcp_servers: HashMap::new(),
            data_path: PathBuf::new(),
        };
        p1.mcp_servers.insert(
            "db".into(),
            McpServerConfig {
                command: Some("pg".into()),
                args: None,
                env: None,
                url: None,
                headers: None,
                oauth: None,
                disabled: None,
                source: None,
            },
        );

        let mut p2 = LoadedPlugin {
            name: "plugin-b".into(),
            version: "1.0.0".into(),
            install_path: PathBuf::new(),
            manifest: make_manifest_with_commands(vec![]),
            commands: vec![],
            skills_dirs: vec![],
            agents_dirs: vec![],
            mcp_servers: HashMap::new(),
            data_path: PathBuf::new(),
        };
        p2.mcp_servers.insert(
            "db".into(),
            McpServerConfig {
                command: Some("mongo".into()),
                args: None,
                env: None,
                url: None,
                headers: None,
                oauth: None,
                disabled: None,
                source: None,
            },
        );

        let merged = merge_plugin_mcp_servers(&[p1, p2]);
        assert_eq!(merged.len(), 2);
        assert!(merged.contains_key("plugin:plugin-a:db"));
        assert!(merged.contains_key("plugin:plugin-b:db"));
    }

    #[test]
    fn test_load_plugins_success() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("my-plugin");
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"my-plugin","version":"1.0.0"}"#,
        )
        .unwrap();

        let installed = InstalledPlugins {
            version: 2,
            plugins: vec![InstalledPlugin {
                id: "my-plugin@test".into(),
                name: "my-plugin".into(),
                version: "1.0.0".into(),
                marketplace: "test".into(),
                install_path: plugin_dir,
                scope: InstallScope::User,
                project_path: None,
            }],
        };

        let loaded = load_plugins(&installed).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "my-plugin");
    }

    #[test]
    fn test_load_plugins_empty() {
        let installed = InstalledPlugins::default();
        let loaded = load_plugins(&installed).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_plugins_invalid_manifest() {
        let dir = tempdir().unwrap();
        let installed = InstalledPlugins {
            version: 2,
            plugins: vec![InstalledPlugin {
                id: "bad@test".into(),
                name: "bad".into(),
                version: "1.0.0".into(),
                marketplace: "test".into(),
                install_path: dir.path().join("empty"),
                scope: InstallScope::User,
                project_path: None,
            }],
        };

        let loaded = load_plugins(&installed).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_enabled_plugins() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("my-plugin");
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"my-plugin","version":"1.0.0"}"#,
        )
        .unwrap();

        std::fs::create_dir_all(dir.path().join("plugins")).unwrap();
        let installed_json = serde_json::to_string(&InstalledPlugins {
            version: 2,
            plugins: vec![InstalledPlugin {
                id: "my-plugin@test".into(),
                name: "my-plugin".into(),
                version: "1.0.0".into(),
                marketplace: "test".into(),
                install_path: plugin_dir.clone(),
                scope: InstallScope::User,
                project_path: None,
            }],
        })
        .unwrap();
        std::fs::write(
            dir.path().join("plugins").join("installed_plugins.json"),
            installed_json,
        )
        .unwrap();

        let settings = r#"{"enabledPlugins":["my-plugin@test"]}"#;
        std::fs::write(dir.path().join("settings.json"), settings).unwrap();

        let loaded = load_enabled_plugins(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn test_load_enabled_plugins_disabled() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("plugins")).unwrap();
        let installed_json = serde_json::to_string(&InstalledPlugins {
            version: 2,
            plugins: vec![InstalledPlugin {
                id: "my-plugin@test".into(),
                name: "my-plugin".into(),
                version: "1.0.0".into(),
                marketplace: "test".into(),
                install_path: dir.path().join("fake"),
                scope: InstallScope::User,
                project_path: None,
            }],
        })
        .unwrap();
        std::fs::write(
            dir.path().join("plugins").join("installed_plugins.json"),
            installed_json,
        )
        .unwrap();

        let settings = r#"{"enabledPlugins":[]}"#;
        std::fs::write(dir.path().join("settings.json"), settings).unwrap();

        let loaded = load_enabled_plugins(dir.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_plugin_command_provider_empty() {
        let provider = PluginCommandProvider::new(&[]);
        assert!(provider.commands().is_empty());
    }

    #[test]
    fn test_plugin_command_provider_multiple() {
        let loaded = vec![
            LoadedPlugin {
                name: "p1".into(),
                version: "1.0.0".into(),
                install_path: PathBuf::new(),
                manifest: make_manifest_with_commands(vec![]),
                commands: vec![
                    CommandEntry {
                        name: "p1:cmd1".into(),
                        description: "d1".into(),
                        source: CommandSource::Builtin,
                    },
                    CommandEntry {
                        name: "p1:cmd2".into(),
                        description: "d2".into(),
                        source: CommandSource::Builtin,
                    },
                ],
                skills_dirs: vec![],
                agents_dirs: vec![],
                mcp_servers: HashMap::new(),
                data_path: PathBuf::new(),
            },
            LoadedPlugin {
                name: "p2".into(),
                version: "1.0.0".into(),
                install_path: PathBuf::new(),
                manifest: make_manifest_with_commands(vec![]),
                commands: vec![
                    CommandEntry {
                        name: "p2:cmd3".into(),
                        description: "d3".into(),
                        source: CommandSource::Builtin,
                    },
                    CommandEntry {
                        name: "p2:cmd4".into(),
                        description: "d4".into(),
                        source: CommandSource::Builtin,
                    },
                ],
                skills_dirs: vec![],
                agents_dirs: vec![],
                mcp_servers: HashMap::new(),
                data_path: PathBuf::new(),
            },
        ];

        let provider = PluginCommandProvider::new(&loaded);
        assert_eq!(provider.commands().len(), 4);
    }

    #[test]
    fn test_load_no_plugins_aggregated() {
        let result = load_enabled_plugins_aggregated(Path::new("/nonexistent/path"));
        assert!(result.plugins.is_empty());
        assert!(result.all_skill_dirs.is_empty());
        assert!(result.all_mcp_servers.is_empty());
        assert!(result.all_agent_dirs.is_empty());
        assert!(result.all_commands.is_empty());
    }

    #[test]
    fn test_load_enabled_plugins_aggregated() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("my-plugin");
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"my-plugin","version":"1.0.0"}"#,
        )
        .unwrap();

        std::fs::create_dir_all(dir.path().join("plugins")).unwrap();
        let installed_json = serde_json::to_string(&InstalledPlugins {
            version: 2,
            plugins: vec![InstalledPlugin {
                id: "my-plugin@test".into(),
                name: "my-plugin".into(),
                version: "1.0.0".into(),
                marketplace: "test".into(),
                install_path: plugin_dir.clone(),
                scope: InstallScope::User,
                project_path: None,
            }],
        })
        .unwrap();
        std::fs::write(
            dir.path().join("plugins").join("installed_plugins.json"),
            installed_json,
        )
        .unwrap();

        let settings = r#"{"enabledPlugins":["my-plugin@test"]}"#;
        std::fs::write(dir.path().join("settings.json"), settings).unwrap();

        let result = load_enabled_plugins_aggregated(dir.path());
        assert_eq!(result.plugins.len(), 1);
        assert_eq!(result.plugins[0].name, "my-plugin");
    }

    #[test]
    fn test_load_plugin_skill_dirs_aggregated() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("skill-plugin");
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(plugin_dir.join("skills").join("my-skill")).unwrap();
        std::fs::write(
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"skill-plugin","version":"1.0.0","skills":["my-skill"]}"#,
        )
        .unwrap();

        std::fs::create_dir_all(dir.path().join("plugins")).unwrap();
        let installed_json = serde_json::to_string(&InstalledPlugins {
            version: 2,
            plugins: vec![InstalledPlugin {
                id: "skill-plugin@test".into(),
                name: "skill-plugin".into(),
                version: "1.0.0".into(),
                marketplace: "test".into(),
                install_path: plugin_dir.clone(),
                scope: InstallScope::User,
                project_path: None,
            }],
        })
        .unwrap();
        std::fs::write(
            dir.path().join("plugins").join("installed_plugins.json"),
            installed_json,
        )
        .unwrap();

        let settings = r#"{"enabledPlugins":["skill-plugin@test"]}"#;
        std::fs::write(dir.path().join("settings.json"), settings).unwrap();

        let result = load_enabled_plugins_aggregated(dir.path());
        assert_eq!(result.all_skill_dirs.len(), 1);
        assert!(result.all_skill_dirs[0].ends_with("my-skill"));
    }
}
