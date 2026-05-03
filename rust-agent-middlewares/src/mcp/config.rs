use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 单个 MCP 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// stdio 传输的可执行命令（如 "npx"）
    pub command: Option<String>,
    /// stdio 传输的命令参数
    #[serde(default)]
    pub args: Option<Vec<String>>,
    /// 传递给子进程的环境变量
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    /// Streamable HTTP 传输的 URL
    pub url: Option<String>,
    /// HTTP 请求的自定义头
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    /// OAuth 2.0 配置
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
}

/// MCP 服务器 OAuth 2.0 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    /// 是否启用 OAuth（默认 true）
    #[serde(default)]
    pub enabled: Option<bool>,
    /// OAuth 客户端 ID
    #[serde(default)]
    pub client_id: Option<String>,
    /// OAuth 客户端密钥（支持 ${VAR} 环境变量展开）
    #[serde(default)]
    pub client_secret: Option<String>,
    /// OAuth 权限范围列表
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

impl OAuthConfig {
    /// 判断 OAuth 是否启用，默认 true
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

/// MCP 配置文件顶层结构（.mcp.json / settings.json 中的 mcpServers 片段）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigFile {
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// MCP 配置加载错误
#[derive(Debug, Error)]
pub enum McpConfigError {
    #[error("MCP 配置文件解析失败: {path}: {source}")]
    ParseError {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("MCP 配置文件读取失败: {path}: {source}")]
    ReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("MCP 配置文件写入失败: {path}: {source}")]
    WriteError {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// 从指定 JSON 文件加载 MCP 配置，文件不存在时返回空配置
pub fn load_from_path(path: &Path) -> Result<McpConfigFile, McpConfigError> {
    if !path.exists() {
        return Ok(McpConfigFile::default());
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| McpConfigError::ReadError {
            path: path.display().to_string(),
            source: e,
        })?;
    serde_json::from_str::<McpConfigFile>(&content).map_err(|e| McpConfigError::ParseError {
        path: path.display().to_string(),
        source: e,
    })
}

/// 从全局 settings.json 的 extra 字段中提取 mcpServers
pub fn load_global_config(settings_json_path: &Path) -> Result<McpConfigFile, McpConfigError> {
    if !settings_json_path.exists() {
        return Ok(McpConfigFile::default());
    }
    let content = std::fs::read_to_string(settings_json_path).map_err(|e| {
        McpConfigError::ReadError {
            path: settings_json_path.display().to_string(),
            source: e,
        }
    })?;
    let v: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        McpConfigError::ParseError {
            path: settings_json_path.display().to_string(),
            source: e,
        }
    })?;
    // 从顶层 value 中提取 "config"."mcpServers" 或 "mcpServers"
    let mcp_servers = v
        .get("config")
        .and_then(|c| c.get("mcpServers"))
        .or_else(|| v.get("mcpServers"))
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let config = McpConfigFile {
        mcp_servers: serde_json::from_value(mcp_servers).unwrap_or_default(),
    };
    Ok(config)
}

/// 展开 s 中所有 ${VAR} 占位符为环境变量值
pub fn expand_env_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // 消耗 '{'
            let var_name: String = chars.by_ref().take_while(|&ch| ch != '}').collect();
            if chars.peek() == Some(&'}') {
                chars.next(); // 消耗 '}'
            }
            match std::env::var(&var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => {
                    tracing::warn!(
                        var_name = %var_name,
                        "MCP 配置环境变量 ${{{}}} 未设置，替换为空字符串",
                        var_name
                    );
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// 对 McpServerConfig 中所有字符串字段执行环境变量展开
pub fn expand_server_config(config: &McpServerConfig) -> McpServerConfig {
    McpServerConfig {
        command: config.command.as_ref().map(|s| expand_env_vars(s)),
        args: config
            .args
            .as_ref()
            .map(|arr| arr.iter().map(|s| expand_env_vars(s)).collect()),
        env: config
            .env
            .as_ref()
            .map(|map| map.iter().map(|(k, v)| (k.clone(), expand_env_vars(v))).collect()),
        url: config.url.as_ref().map(|s| expand_env_vars(s)),
        headers: config
            .headers
            .as_ref()
            .map(|map| map.iter().map(|(k, v)| (k.clone(), expand_env_vars(v))).collect()),
        oauth: config.oauth.as_ref().map(|o| OAuthConfig {
            enabled: o.enabled,
            client_id: o.client_id.clone(),
            client_secret: o.client_secret.as_ref().map(|s| expand_env_vars(s)),
            scopes: o.scopes.clone(),
        }),
    }
}

/// 加载并合并 MCP 配置：全局 settings.json + 项目级 .mcp.json
/// 同名 server 以项目级覆盖全局，所有字段执行 ${VAR} 展开
pub fn load_merged_config(cwd: &Path) -> McpConfigFile {
    // 1. 加载全局配置
    let global_path = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zen-code")
        .join("settings.json");
    let global = load_global_config(&global_path).unwrap_or_else(|e| {
        tracing::warn!(
            path = %global_path.display(),
            error = %e,
            "加载全局 MCP 配置失败，跳过"
        );
        McpConfigFile::default()
    });

    // 2. 加载项目级配置
    let project_path = cwd.join(".mcp.json");
    let project = load_from_path(&project_path).unwrap_or_else(|e| {
        tracing::warn!(
            path = %project_path.display(),
            error = %e,
            "加载项目级 MCP 配置失败，跳过"
        );
        McpConfigFile::default()
    });

    // 3. 合并：项目级覆盖全局
    let mut merged = global;
    for (name, server_config) in project.mcp_servers {
        merged.mcp_servers.insert(name, server_config);
    }

    // 4. 环境变量展开
    let names: Vec<String> = merged.mcp_servers.keys().cloned().collect();
    for name in names {
        if let Some(server_config) = merged.mcp_servers.get(&name).cloned() {
            merged
                .mcp_servers
                .insert(name, expand_server_config(&server_config));
        }
    }

    merged
}

/// 原子写入 JSON 文件（先写临时文件，再 rename 替换）
fn atomic_write_json(path: &Path, value: &serde_json::Value) -> Result<(), McpConfigError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_path = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));

    let content = serde_json::to_string_pretty(value).map_err(|e| McpConfigError::WriteError {
        path: path.display().to_string(),
        source: e.into(),
    })?;

    use std::io::Write;
    let mut file = std::fs::File::create(&tmp_path).map_err(|e| McpConfigError::WriteError {
        path: path.display().to_string(),
        source: e,
    })?;
    file.write_all(content.as_bytes())
        .map_err(|e| McpConfigError::WriteError {
            path: path.display().to_string(),
            source: e,
        })?;
    drop(file);

    std::fs::rename(&tmp_path, path).map_err(|e| McpConfigError::WriteError {
        path: path.display().to_string(),
        source: e,
    })?;

    Ok(())
}

/// 从配置文件中删除指定的 MCP 服务器
/// 优先尝试项目级 .mcp.json，未找到则尝试全局 settings.json
pub fn remove_server_from_config(cwd: &Path, server_name: &str) -> Result<(), McpConfigError> {
    let global_path = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zen-code")
        .join("settings.json");
    remove_server_from_config_with_paths(cwd, &global_path, server_name)
}

/// 内部实现：允许注入全局路径（便于测试）
fn remove_server_from_config_with_paths(
    cwd: &Path,
    global_path: &Path,
    server_name: &str,
) -> Result<(), McpConfigError> {
    // 1. 尝试项目级删除
    let project_path = cwd.join(".mcp.json");
    if project_path.exists() {
        let content = std::fs::read_to_string(&project_path).map_err(|e| McpConfigError::ReadError {
            path: project_path.display().to_string(),
            source: e,
        })?;

        let mut config: McpConfigFile = serde_json::from_str(&content).map_err(|e| {
            McpConfigError::ParseError {
                path: project_path.display().to_string(),
                source: e,
            }
        })?;

        if config.mcp_servers.contains_key(server_name) {
            config.mcp_servers.remove(server_name);
            let value = serde_json::to_value(&config).map_err(|e| McpConfigError::WriteError {
                path: project_path.display().to_string(),
                source: e.into(),
            })?;
            atomic_write_json(&project_path, &value)?;
            return Ok(());
        }
    }

    // 2. 尝试全局删除
    if global_path.exists() {
        let content = std::fs::read_to_string(global_path).map_err(|e| McpConfigError::ReadError {
            path: global_path.display().to_string(),
            source: e,
        })?;

        let mut value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            McpConfigError::ParseError {
                path: global_path.display().to_string(),
                source: e,
            }
        })?;

        // 尝试 config.mcpServers 路径
        let mut removed = false;
        if let Some(config) = value.get_mut("config").and_then(|c| c.get_mut("mcpServers")) {
            if let Some(servers) = config.as_object_mut() {
                if servers.remove(server_name).is_some() {
                    removed = true;
                }
            }
        }

        // 尝试顶层 mcpServers 路径
        if !removed {
            if let Some(servers) = value.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
                if servers.remove(server_name).is_some() {
                    removed = true;
                }
            }
        }

        if removed {
            atomic_write_json(global_path, &value)?;
            return Ok(());
        }
    }

    // 未在任何配置中找到该 server，幂等返回
    Ok(())
}

fn test_config() -> McpServerConfig {
    McpServerConfig { command: None, args: None, env: None, url: None, headers: None, oauth: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_from_nonexistent_path() {
        let result = load_from_path(Path::new("/nonexistent/path/file.json"));
        assert!(result.is_ok());
        assert!(result.unwrap().mcp_servers.is_empty());
    }

    #[test]
    fn test_load_from_valid_json() {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(
            &mut f,
            br#"{"mcpServers":{"fs":{"command":"npx","args":["-y","@mcp/filesystem"]}}}"#,
        )
        .unwrap();
        let config = load_from_path(f.path()).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers["fs"].command.as_deref(), Some("npx"));
        assert_eq!(config.mcp_servers["fs"].args.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_load_from_invalid_json() {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, b"{invalid json}").unwrap();
        let result = load_from_path(f.path());
        assert!(matches!(result, Err(McpConfigError::ParseError { .. })));
    }

    #[test]
    fn test_load_global_config() {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(
            &mut f,
            br#"{"config":{"mcpServers":{"gh":{"url":"https://api.github.com"}}}}"#,
        )
        .unwrap();
        let config = load_global_config(f.path()).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers["gh"].url.as_deref(), Some("https://api.github.com"));
    }

    #[test]
    fn test_load_global_config_top_level() {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(
            &mut f,
            br#"{"mcpServers":{"gh":{"command":"npx"}}}"#,
        )
        .unwrap();
        let config = load_global_config(f.path()).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers["gh"].command.as_deref(), Some("npx"));
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_MCP_VAR", "hello");
        let result = expand_env_vars("prefix_${TEST_MCP_VAR}_suffix");
        assert_eq!(result, "prefix_hello_suffix");
        std::env::remove_var("TEST_MCP_VAR");
    }

    #[test]
    fn test_expand_env_vars_missing() {
        let result = expand_env_vars("${NONEXISTENT_MCP_VAR_12345}");
        assert_eq!(result, "");
    }

    #[test]
    fn test_expand_env_vars_no_braces() {
        let result = expand_env_vars("$NO_BRACE");
        assert_eq!(result, "$NO_BRACE");
    }

    #[test]
    fn test_oauth_config_default_enabled() {
        let config = OAuthConfig::default();
        assert!(config.is_enabled());
    }

    #[test]
    fn test_oauth_config_explicitly_disabled() {
        let config = OAuthConfig { enabled: Some(false), ..Default::default() };
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_oauth_config_deserialize() {
        let json = r#"{"clientId":"my-app","clientSecret":"${MY_SECRET}","scopes":["read","write"]}"#;
        let config: OAuthConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.client_id.as_deref(), Some("my-app"));
        assert_eq!(config.client_secret.as_deref(), Some("${MY_SECRET}"));
        assert_eq!(config.scopes.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_oauth_config_missing_fields() {
        let json = r#"{"clientId":"my-app"}"#;
        let config: OAuthConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.client_id.as_deref(), Some("my-app"));
        assert!(config.client_secret.is_none());
        assert!(config.scopes.is_none());
        assert!(config.enabled.is_none());
        assert!(config.is_enabled());
    }

    #[test]
    fn test_mcp_server_config_oauth_field() {
        let json = r#"{"url":"https://example.com","oauth":{"clientId":"app"}}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.oauth.is_some());
        assert_eq!(config.oauth.unwrap().client_id.as_deref(), Some("app"));
    }

    #[test]
    fn test_mcp_server_config_oauth_default() {
        let json = r#"{"command":"npx"}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.oauth.is_none());
    }

    #[test]
    fn test_expand_server_config_oauth_client_secret() {
        std::env::set_var("TEST_OAUTH_SECRET", "secret123");
        let config = McpServerConfig {
            oauth: Some(OAuthConfig { client_secret: Some("${TEST_OAUTH_SECRET}".into()), ..Default::default() }),
            ..test_config()
        };
        let expanded = expand_server_config(&config);
        assert_eq!(expanded.oauth.unwrap().client_secret.as_deref(), Some("secret123"));
        std::env::remove_var("TEST_OAUTH_SECRET");
    }

    #[test]
    fn test_merge_project_overrides_global() {
        let mut global = McpConfigFile::default();
        global.mcp_servers.insert("fs".to_string(), McpServerConfig { command: Some("npx".into()), ..test_config() });
        let mut project = McpConfigFile::default();
        project.mcp_servers.insert("fs".to_string(), McpServerConfig { command: Some("uvx".into()), ..test_config() });
        let mut merged = global;
        for (name, server_config) in project.mcp_servers {
            merged.mcp_servers.insert(name, server_config);
        }
        assert_eq!(merged.mcp_servers["fs"].command.as_deref(), Some("uvx"));
    }

    #[test]
    fn test_merge_project_adds_new_server() {
        let mut global = McpConfigFile::default();
        global.mcp_servers.insert("fs".to_string(), McpServerConfig { command: Some("npx".into()), ..test_config() });
        let mut project = McpConfigFile::default();
        project.mcp_servers.insert("gh".to_string(), McpServerConfig { url: Some("https://api.github.com".into()), ..test_config() });
        let mut merged = global;
        for (name, server_config) in project.mcp_servers {
            merged.mcp_servers.insert(name, server_config);
        }
        assert_eq!(merged.mcp_servers.len(), 2);
        assert!(merged.mcp_servers.contains_key("fs"));
        assert!(merged.mcp_servers.contains_key("gh"));
    }

    #[test]
    fn test_remove_server_from_project_config() {
        let dir = tempfile::tempdir().unwrap();
        let mcp_path = dir.path().join(".mcp.json");
        std::fs::write(
            &mcp_path,
            r#"{"mcpServers":{"server-a":{"command":"npx"},"server-b":{"command":"uvx"}}}"#,
        )
        .unwrap();

        remove_server_from_config(dir.path(), "server-a").unwrap();

        let content = std::fs::read_to_string(&mcp_path).unwrap();
        let config: McpConfigFile = serde_json::from_str(&content).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert!(config.mcp_servers.contains_key("server-b"));
    }

    #[test]
    fn test_remove_server_from_global_config_nested() {
        let dir = tempfile::tempdir().unwrap();
        let settings_dir = dir.path().join(".zen-code");
        std::fs::create_dir_all(&settings_dir).unwrap();
        let settings_path = settings_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            r#"{"config":{"mcpServers":{"gh":{"url":"https://api.github.com"}}},"otherSetting":42}"#,
        )
        .unwrap();

        let empty_cwd = dir.path().join("empty_project");
        std::fs::create_dir_all(&empty_cwd).unwrap();
        remove_server_from_config_with_paths(&empty_cwd, &settings_path, "gh").unwrap();

        let content = std::fs::read_to_string(&settings_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(value["config"]["mcpServers"].as_object().unwrap().is_empty());
        assert_eq!(value["otherSetting"], 42);
    }

    #[test]
    fn test_remove_server_from_global_config_top_level() {
        let dir = tempfile::tempdir().unwrap();
        let settings_dir = dir.path().join(".zen-code");
        std::fs::create_dir_all(&settings_dir).unwrap();
        let settings_path = settings_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            r#"{"mcpServers":{"fs":{"command":"npx"}},"otherSetting":42}"#,
        )
        .unwrap();

        let empty_cwd = dir.path().join("empty_project");
        std::fs::create_dir_all(&empty_cwd).unwrap();
        remove_server_from_config_with_paths(&empty_cwd, &settings_path, "fs").unwrap();

        let content = std::fs::read_to_string(&settings_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(value["mcpServers"].as_object().unwrap().is_empty());
        assert_eq!(value["otherSetting"], 42);
    }

    #[test]
    fn test_remove_server_nonexistent_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".mcp.json"), r#"{"mcpServers":{}}"#).unwrap();
        let settings_dir = dir.path().join(".zen-code");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(settings_dir.join("settings.json"), r#"{}"#).unwrap();

        assert!(remove_server_from_config(dir.path(), "nonexistent").is_ok());

        let content = std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        assert_eq!(content, r#"{"mcpServers":{}}"#);
    }
}
