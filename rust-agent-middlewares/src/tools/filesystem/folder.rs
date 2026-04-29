use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::path::Path;

use super::resolve_path;
use chrono::{TimeZone, Utc};

/// folder_operations tool - 与 TypeScript folder_tool 对齐
pub struct FolderOperationsTool {
    pub cwd: String,
}

impl FolderOperationsTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 列表操作最多返回的条目数，防止撑爆 LLM context window
const MAX_LIST_ENTRIES: usize = 500;

fn list_folder(resolved: &Path) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let entries = std::fs::read_dir(resolved)?;

    let mut folders: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let size = metadata.len();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| {
                    Utc.timestamp_opt(d.as_secs() as i64, 0)
                        .single()
                        .map(|dt| dt.format("%Y/%m/%d").to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                })
            })
            .unwrap_or_else(|| "unknown".to_string());

        if metadata.is_dir() {
            folders.push(format!("  📁 {name}/ ({size} bytes, {modified})"));
        } else {
            files.push(format!("  📄 {name} ({size} bytes, {modified})"));
        }
    }

    let total_folders = folders.len();
    let total_files = files.len();
    let total = total_folders + total_files;
    let truncated = total > MAX_LIST_ENTRIES;

    if truncated {
        // 公平分配：folders 和 files 各占一半配额
        let half = MAX_LIST_ENTRIES / 2;
        folders.truncate(half.min(folders.len()));
        files.truncate((MAX_LIST_ENTRIES - folders.len()).min(files.len()));
    }

    let mut result = format!("📁 {}\n\n", resolved.display());

    if !folders.is_empty() {
        result.push_str("Directories:\n");
        for f in &folders {
            result.push_str(f);
            result.push('\n');
        }
        result.push('\n');
    }

    if !files.is_empty() {
        result.push_str("Files:\n");
        for f in &files {
            result.push_str(f);
            result.push('\n');
        }
    }

    if truncated {
        result.push_str(&format!(
            "\n[Output truncated: {} total entries, showing first {}]",
            total, MAX_LIST_ENTRIES
        ));
    }

    result.push_str(&format!(
        "\nTotal: {} directories, {} files",
        total_folders, total_files
    ));

    Ok(result)
}

#[async_trait::async_trait]
impl BaseTool for FolderOperationsTool {
    fn name(&self) -> &str {
        "folder_operations"
    }

    fn description(&self) -> &str {
        "Unified folder operations tool supporting create, list, and existence check. Parameters (JSON): operation: \"create\"|\"list\"|\"exists\" (required), folder_path: string (required), recursive: bool (optional)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation":   { "type": "string",  "enum": ["create", "list", "exists"], "description": "Operation to perform" },
                "folder_path": { "type": "string",  "description": "Path to the folder (absolute or relative to cwd)" },
                "recursive":   { "type": "boolean", "description": "Create parent directories if needed (default true)" }
            },
            "required": ["operation", "folder_path"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let operation = input["operation"]
            .as_str()
            .ok_or("Missing operation parameter")?;
        let folder_path = input["folder_path"]
            .as_str()
            .ok_or("Missing folder_path parameter")?;
        let recursive = input["recursive"].as_bool().unwrap_or(true);

        let resolved = resolve_path(&self.cwd, folder_path);

        match operation {
            "create" => {
                if recursive {
                    std::fs::create_dir_all(&resolved)?;
                } else {
                    std::fs::create_dir(&resolved)?;
                }
                Ok(format!(
                    "\u{2713} Folder created successfully at: {}",
                    resolved.display()
                ))
            }

            "exists" => {
                if resolved.exists() {
                    let kind = if resolved.is_dir() { "Directory" } else { "File" };
                    Ok(format!(
                        "\u{2713} Folder exists at: {}\n  Type: {kind}",
                        resolved.display()
                    ))
                } else {
                    Ok(format!(
                        "\u{2717} Folder does not exist at: {}",
                        resolved.display()
                    ))
                }
            }

            "list" => {
                if !resolved.exists() {
                    return Ok(format!(
                        "\u{2717} Folder not found: {}",
                        resolved.display()
                    ));
                }
                if !resolved.is_dir() {
                    return Ok(format!(
                        "\u{2717} Path exists but is not a folder: {}",
                        resolved.display()
                    ));
                }
                match list_folder(&resolved) {
                    Ok(s) => Ok(s),
                    Err(e) => Ok(format!("\u{2717} Error: {e}")),
                }
            }

            other => Ok(format!("\u{2717} Unknown operation: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_folder_create() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FolderOperationsTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"operation": "create", "folder_path": "newdir"}))
            .await
            .unwrap();
        assert!(result.contains("created successfully"), "unexpected: {result}");
        assert!(dir.path().join("newdir").is_dir());
    }

    #[tokio::test]
    async fn test_folder_create_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FolderOperationsTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"operation": "create", "folder_path": "a/b/c"}))
            .await
            .unwrap();
        assert!(dir.path().join("a/b/c").is_dir());
    }

    #[tokio::test]
    async fn test_folder_exists_true() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("existing")).unwrap();
        let tool = FolderOperationsTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"operation": "exists", "folder_path": "existing"}))
            .await
            .unwrap();
        assert!(result.contains("Folder exists"), "should report exists: {result}");
    }

    #[tokio::test]
    async fn test_folder_exists_false() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FolderOperationsTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"operation": "exists", "folder_path": "ghost"}))
            .await
            .unwrap();
        assert!(result.contains("does not exist"), "should report missing: {result}");
    }

    #[tokio::test]
    async fn test_folder_list() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("listed");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("file.txt"), "hello").unwrap();
        let tool = FolderOperationsTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"operation": "list", "folder_path": "listed"}))
            .await
            .unwrap();
        assert!(result.contains("file.txt"), "should list file.txt: {result}");
    }

    #[tokio::test]
    async fn test_folder_list_truncation_keeps_files() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("bigdir");
        std::fs::create_dir(&subdir).unwrap();
        // 创建超过 MAX_LIST_ENTRIES 的子目录
        for i in 0..600 {
            std::fs::create_dir(subdir.join(format!("d{}", i))).unwrap();
        }
        // 同时创建一些文件
        for i in 0..5 {
            std::fs::write(subdir.join(format!("f{}.txt", i)), "x").unwrap();
        }
        let tool = FolderOperationsTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"operation": "list", "folder_path": "bigdir"}))
            .await
            .unwrap();
        // 文件不应被全部丢弃
        assert!(
            result.contains("f0.txt") || result.contains("f1.txt"),
            "截断后应保留部分文件: {result}"
        );
        assert!(result.contains("truncated"), "应显示截断提示: {result}");
    }
}
