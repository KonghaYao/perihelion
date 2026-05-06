use crate::mcp::McpServerConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// plugin.json 中 mcpServers 字段的值：内联配置对象或文件路径引用
#[derive(Debug, Clone)]
pub enum McpServerEntry {
    /// 内联 MCP 服务器配置
    Config(Box<McpServerConfig>),
    /// .mcp.json 文件路径（相对于插件根目录）
    FilePath(String),
}

impl Serialize for McpServerEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            McpServerEntry::Config(cfg) => cfg.serialize(serializer),
            McpServerEntry::FilePath(path) => serializer.serialize_str(path),
        }
    }
}

impl McpServerEntry {
    /// 如果是内联配置，返回内部 McpServerConfig 的引用
    pub fn as_config(&self) -> Option<&McpServerConfig> {
        match self {
            McpServerEntry::Config(cfg) => Some(cfg),
            McpServerEntry::FilePath(_) => None,
        }
    }
}

impl<'de> Deserialize<'de> for McpServerEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(s) = value.as_str() {
            return Ok(McpServerEntry::FilePath(s.to_string()));
        }
        let config: McpServerConfig =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(McpServerEntry::Config(Box::new(config)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    pub path: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAgent {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLspServer {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginChannel {
    pub name: String,
    #[serde(rename = "mcpServer")]
    pub mcp_server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOption {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub option_type: String,
    pub default: Option<serde_json::Value>,
}

/// 兼容 Claude Code 的插件清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub author: Option<PluginAuthor>,
    pub commands: Option<Vec<PluginCommand>>,
    pub agents: Option<Vec<PluginAgent>>,
    pub skills: Option<Vec<String>>,
    /// 预留字段，本次不实现
    pub hooks: Option<serde_json::Value>,
    #[serde(rename = "mcpServers")]
    pub mcp_servers: Option<HashMap<String, McpServerEntry>>,
    #[serde(rename = "lspServers")]
    pub lsp_servers: Option<Vec<PluginLspServer>>,
    #[serde(rename = "outputStyles")]
    pub output_styles: Option<Vec<String>>,
    pub channels: Option<Vec<PluginChannel>>,
    pub options: Option<Vec<PluginOption>>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePlugin {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// 插件来源：可以是字符串路径（"./plugins/foo"）或对象（{"source":"url","url":"..."}）
    pub source: serde_json::Value,
    #[serde(default)]
    pub version: String,
    pub sha: Option<String>,
    pub author: Option<PluginAuthor>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceManifest {
    pub name: String,
    pub plugins: Vec<MarketplacePlugin>,
    #[serde(rename = "allowCrossMarketplaceDependenciesOn")]
    pub allow_cross_marketplace: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source")]
pub enum MarketplaceSource {
    #[serde(rename = "github")]
    GitHub { repo: String },
    #[serde(rename = "git")]
    Git { url: String },
    #[serde(rename = "url")]
    Url { url: String },
    #[serde(rename = "file")]
    File { path: String },
    #[serde(rename = "directory")]
    Directory { path: String },
    #[serde(rename = "npm")]
    Npm { package: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum InstallScope {
    #[default]
    User,
    Project,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub marketplace: String,
    pub install_path: PathBuf,
    #[serde(default)]
    pub scope: InstallScope,
    /// 项目路径 (仅用于 project/local scope)
    #[serde(default, rename = "projectPath")]
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugins {
    pub version: u32,
    #[serde(default, deserialize_with = "deserialize_installed_plugins")]
    pub plugins: Vec<InstalledPlugin>,
}

/// Claude Code 的 installed_plugins.json 中每个版本记录的格式
#[derive(Debug, Clone, Deserialize)]
struct ClaudeCodeVersionRecord {
    #[serde(default)]
    scope: String,
    #[serde(rename = "installPath")]
    install_path: String,
    version: String,
    #[serde(default, rename = "projectPath")]
    project_path: Option<String>,
}

/// 兼容 Claude Code 两种 installed_plugins 格式：
/// - Claude Code 对象格式: `{"plugin-id@marketplace": [{version record}]}`
/// - 内部数组格式: `[InstalledPlugin, ...]`
fn deserialize_installed_plugins<'de, D>(deserializer: D) -> Result<Vec<InstalledPlugin>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Object(map) => {
            let mut plugins = Vec::new();
            for (id, versions) in map {
                let version_arr = match versions {
                    serde_json::Value::Array(arr) => arr,
                    _ => continue,
                };
                let latest = match version_arr.first() {
                    Some(v) => v,
                    None => continue,
                };
                let record: ClaudeCodeVersionRecord = match serde_json::from_value(latest.clone()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let (name, marketplace) = match id.split_once('@') {
                    Some((n, m)) => (n.to_string(), m.to_string()),
                    None => (id.clone(), String::new()),
                };
                let scope = match record.scope.as_str() {
                    "project" => InstallScope::Project,
                    "local" => InstallScope::Local,
                    _ => InstallScope::User,
                };
                plugins.push(InstalledPlugin {
                    id,
                    name,
                    version: record.version,
                    marketplace,
                    install_path: PathBuf::from(&record.install_path),
                    scope,
                    project_path: record.project_path,
                });
            }
            Ok(plugins)
        }
        serde_json::Value::Array(arr) => {
            serde_json::from_value(serde_json::Value::Array(arr)).map_err(serde::de::Error::custom)
        }
        _ => Ok(Vec::new()),
    }
}

impl Default for InstalledPlugins {
    fn default() -> Self {
        Self {
            version: 2,
            plugins: Vec::new(),
        }
    }
}

/// 已注册的 marketplace 配置条目
///
/// 与 Claude Code 的 KnownMarketplaceSchema 兼容：
/// - source: required - marketplace 来源
/// - installLocation: required - 本地缓存路径
/// - lastUpdated: required - ISO 8601 时间戳
/// - autoUpdate: optional - 是否自动更新
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownMarketplace {
    pub source: MarketplaceSource,
    #[serde(rename = "installLocation")]
    pub install_location: String,
    #[serde(rename = "autoUpdate", default)]
    pub auto_update: bool,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
}

/// 声明格式的 marketplace（用于 settings.json 的 extraKnownMarketplaces）
///
/// 这是意图层（intent layer）的声明，只需要 source 字段。
/// 当 marketplace 实际安装后，会转换为 KnownMarketplace 并添加 installLocation 和 lastUpdated。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclaredMarketplace {
    pub source: MarketplaceSource,
    #[serde(rename = "installLocation", default)]
    pub install_location: Option<String>,
    #[serde(rename = "autoUpdate", default)]
    pub auto_update: bool,
    #[serde(rename = "lastUpdated", default)]
    pub last_updated: Option<String>,
}

impl From<DeclaredMarketplace> for KnownMarketplace {
    fn from(declared: DeclaredMarketplace) -> Self {
        KnownMarketplace {
            source: declared.source,
            install_location: declared.install_location.unwrap_or_default(),
            auto_update: declared.auto_update,
            last_updated: declared.last_updated.unwrap_or_default(),
        }
    }
}

impl From<KnownMarketplace> for DeclaredMarketplace {
    fn from(known: KnownMarketplace) -> Self {
        DeclaredMarketplace {
            source: known.source,
            install_location: if known.install_location.is_empty() {
                None
            } else {
                Some(known.install_location)
            },
            auto_update: known.auto_update,
            last_updated: if known.last_updated.is_empty() {
                None
            } else {
                Some(known.last_updated)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_minimal() {
        let json = r#"{"name":"test-plugin","version":"1.0.0"}"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.description.is_empty());
        assert!(manifest.author.is_none());
        assert!(manifest.commands.is_none());
        assert!(manifest.agents.is_none());
        assert!(manifest.skills.is_none());
        assert!(manifest.hooks.is_none());
        assert!(manifest.mcp_servers.is_none());
        assert!(manifest.lsp_servers.is_none());
        assert!(manifest.output_styles.is_none());
        assert!(manifest.channels.is_none());
        assert!(manifest.options.is_none());
        assert!(manifest.settings.is_none());
    }

    #[test]
    fn test_plugin_manifest_full() {
        let json = r#"{
            "name": "full-plugin",
            "version": "2.0.0",
            "description": "A full plugin",
            "author": {"name": "Test Author", "url": "https://example.com"},
            "commands": [{"path": "/commands/test.md", "name": "test", "description": "Test command"}],
            "agents": [{"path": "/agents/test.md", "name": "test-agent"}],
            "skills": ["/skills/test-skill"],
            "hooks": {},
            "mcpServers": {
                "test-server": {
                    "command": "node",
                    "args": ["server.js"]
                }
            },
            "lspServers": [{"name": "test-lsp", "command": "test-lsp-binary", "args": []}],
            "outputStyles": ["compact"],
            "channels": [{"name": "test-channel", "mcpServer": "test-server"}],
            "options": [{"name": "opt1", "description": "Option 1", "type": "string", "default": "val1"}],
            "settings": {"key": "value"}
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "full-plugin");
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(manifest.description, "A full plugin");
        assert_eq!(manifest.author.as_ref().unwrap().name, "Test Author");
        assert_eq!(manifest.commands.as_ref().unwrap().len(), 1);
        assert_eq!(manifest.agents.as_ref().unwrap().len(), 1);
        assert_eq!(manifest.skills.as_ref().unwrap().len(), 1);
        assert!(manifest.mcp_servers.is_some());
        let mcp = manifest.mcp_servers.as_ref().unwrap();
        match mcp.get("test-server").unwrap() {
            McpServerEntry::Config(cfg) => {
                assert_eq!(cfg.command.as_deref(), Some("node"));
            }
            McpServerEntry::FilePath(_) => panic!("expected Config variant"),
        }
        assert_eq!(manifest.lsp_servers.as_ref().unwrap().len(), 1);
        assert_eq!(manifest.channels.as_ref().unwrap().len(), 1);
        assert_eq!(manifest.options.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_plugin_manifest_mcp_servers_rename() {
        let json = r#"{"name":"p","version":"1.0.0","mcpServers":{"srv":{"command":"cmd","args":["-a"]}}}"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let servers = manifest.mcp_servers.unwrap();
        assert!(servers.contains_key("srv"));
        match &servers["srv"] {
            McpServerEntry::Config(cfg) => {
                assert_eq!(cfg.command.as_deref(), Some("cmd"));
            }
            McpServerEntry::FilePath(_) => panic!("expected Config variant"),
        }
    }

    #[test]
    fn test_mcp_server_entry_file_path() {
        let json = r#"{"name":"p","version":"1.0.0","mcpServers":{"srv":"./path/to/.mcp.json"}}"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let servers = manifest.mcp_servers.unwrap();
        match servers.get("srv").unwrap() {
            McpServerEntry::FilePath(path) => assert_eq!(path, "./path/to/.mcp.json"),
            McpServerEntry::Config(_) => panic!("expected FilePath variant"),
        }
    }

    #[test]
    fn test_mcp_server_entry_inline_config() {
        let json = r#"{"name":"p","version":"1.0.0","mcpServers":{"srv":{"command":"node","args":["server.js"]}}}"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let servers = manifest.mcp_servers.unwrap();
        match servers.get("srv").unwrap() {
            McpServerEntry::Config(cfg) => {
                assert_eq!(cfg.command.as_deref(), Some("node"));
            }
            McpServerEntry::FilePath(_) => panic!("expected Config variant"),
        }
    }

    #[test]
    fn test_marketplace_source_github() {
        let json = r#"{"source":"github","repo":"anthropics/claude-plugins-official"}"#;
        let source: MarketplaceSource = serde_json::from_str(json).unwrap();
        match source {
            MarketplaceSource::GitHub { repo } => {
                assert_eq!(repo, "anthropics/claude-plugins-official")
            }
            _ => panic!("expected GitHub variant"),
        }
    }

    #[test]
    fn test_marketplace_source_url() {
        let json = r#"{"source":"url","url":"https://example.com/marketplace.json"}"#;
        let source: MarketplaceSource = serde_json::from_str(json).unwrap();
        match source {
            MarketplaceSource::Url { url } => {
                assert_eq!(url, "https://example.com/marketplace.json")
            }
            _ => panic!("expected Url variant"),
        }
    }

    #[test]
    fn test_installed_plugins_default() {
        let default = InstalledPlugins::default();
        assert_eq!(default.version, 2);
        assert!(default.plugins.is_empty());
    }

    #[test]
    fn test_installed_plugins_claude_code_object_format() {
        let json = r#"{
            "version": 2,
            "plugins": {
                "typescript-lsp@claude-plugins-official": [
                    {
                        "scope": "user",
                        "installPath": "/Users/test/.claude/plugins/cache/claude-plugins-official/typescript-lsp/1.0.0",
                        "version": "1.0.0",
                        "installedAt": "2026-04-03T11:48:01.555Z",
                        "gitCommitSha": "abc123"
                    }
                ],
                "frontend-design@claude-plugins-official": [
                    {
                        "scope": "user",
                        "installPath": "/Users/test/.claude/plugins/cache/claude-plugins-official/frontend-design/7ed523140f50",
                        "version": "7ed523140f50"
                    }
                ]
            }
        }"#;
        let installed: InstalledPlugins = serde_json::from_str(json).unwrap();
        assert_eq!(installed.version, 2);
        assert_eq!(installed.plugins.len(), 2);

        let mut plugins = installed.plugins.clone();
        plugins.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(plugins[0].id, "frontend-design@claude-plugins-official");
        assert_eq!(plugins[0].name, "frontend-design");
        assert_eq!(plugins[0].version, "7ed523140f50");

        assert_eq!(plugins[1].id, "typescript-lsp@claude-plugins-official");
        assert_eq!(plugins[1].name, "typescript-lsp");
        assert_eq!(plugins[1].marketplace, "claude-plugins-official");
        assert_eq!(plugins[1].version, "1.0.0");
        assert_eq!(plugins[1].scope, InstallScope::User);
        assert!(plugins[1].install_path.ends_with("typescript-lsp/1.0.0"));
    }

    #[test]
    fn test_installed_plugins_internal_array_format() {
        let json = r#"{
            "version": 2,
            "plugins": [
                {
                    "id": "test@marketplace",
                    "name": "test",
                    "version": "1.0.0",
                    "marketplace": "marketplace",
                    "install_path": "/tmp/test",
                    "scope": "User"
                }
            ]
        }"#;
        let installed: InstalledPlugins = serde_json::from_str(json).unwrap();
        assert_eq!(installed.plugins.len(), 1);
        assert_eq!(installed.plugins[0].id, "test@marketplace");
    }

    #[test]
    fn test_installed_plugins_id_without_at() {
        let json = r#"{
            "version": 2,
            "plugins": {
                "standalone-plugin": [
                    {
                        "scope": "project",
                        "installPath": "/tmp/standalone",
                        "version": "2.0.0"
                    }
                ]
            }
        }"#;
        let installed: InstalledPlugins = serde_json::from_str(json).unwrap();
        assert_eq!(installed.plugins.len(), 1);
        assert_eq!(installed.plugins[0].id, "standalone-plugin");
        assert_eq!(installed.plugins[0].name, "standalone-plugin");
        assert_eq!(installed.plugins[0].marketplace, "");
        assert_eq!(installed.plugins[0].scope, InstallScope::Project);
    }

    #[test]
    fn test_install_scope_default() {
        assert_eq!(InstallScope::default(), InstallScope::User);
    }

    #[test]
    fn test_known_marketplace_deserialize() {
        let json = r#"{
            "source": {"source":"github","repo":"test/repo"},
            "installLocation": "/tmp/test",
            "autoUpdate": true,
            "lastUpdated": "2025-01-01T00:00:00Z"
        }"#;
        let km: KnownMarketplace = serde_json::from_str(json).unwrap();
        match &km.source {
            MarketplaceSource::GitHub { repo } => assert_eq!(repo, "test/repo"),
            _ => panic!("expected GitHub variant"),
        }
        assert_eq!(km.install_location, "/tmp/test");
        assert!(km.auto_update);
        assert_eq!(km.last_updated, "2025-01-01T00:00:00Z");
    }

    #[test]
    fn test_known_marketplace_without_auto_update() {
        let json = r#"{
            "source": {"source":"github","repo":"test/repo"},
            "installLocation": "/tmp/test",
            "lastUpdated": "2025-01-01T00:00:00Z"
        }"#;
        let km: KnownMarketplace = serde_json::from_str(json).unwrap();
        assert!(!km.auto_update); // default value
        assert_eq!(km.install_location, "/tmp/test");
        assert_eq!(km.last_updated, "2025-01-01T00:00:00Z");
    }

    #[test]
    fn test_plugin_manifest_serialization_roundtrip() {
        let original = PluginManifest {
            name: "roundtrip".into(),
            version: "1.2.3".into(),
            description: "test".into(),
            author: Some(PluginAuthor {
                name: "Author".into(),
                url: Some("https://example.com".into()),
            }),
            commands: Some(vec![PluginCommand {
                path: "/cmd.md".into(),
                name: Some("cmd".into()),
                description: Some("desc".into()),
            }]),
            agents: Some(vec![PluginAgent {
                path: "/agent.md".into(),
                name: "agent".into(),
            }]),
            skills: Some(vec!["/skill".into()]),
            hooks: None,
            mcp_servers: None,
            lsp_servers: None,
            output_styles: None,
            channels: None,
            options: None,
            settings: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, original.name);
        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.description, original.description);
        assert_eq!(
            deserialized.author.as_ref().unwrap().name,
            original.author.as_ref().unwrap().name
        );
        assert_eq!(deserialized.commands.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.agents.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.skills.as_ref().unwrap().len(), 1);
    }
}
