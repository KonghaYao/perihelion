use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::path::Path;

use super::resolve_path;

/// folder_operations tool - 与 TypeScript folder_tool 对齐
pub struct FolderOperationsTool {
    pub cwd: String,
}

impl FolderOperationsTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

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
                    let secs = d.as_secs();
                    let days = secs / 86400;
                    let years = 1970 + days / 365;
                    let rem_days = days % 365;
                    let month = rem_days / 30 + 1;
                    let day = rem_days % 30 + 1;
                    format!("{}/{}/{}", month, day, years)
                })
            })
            .unwrap_or_else(|| "unknown".to_string());

        if metadata.is_dir() {
            folders.push(format!("  \u{1F4C1} {name}/ ({size} bytes, {modified})"));
        } else {
            files.push(format!("  \u{1F4C4} {name} ({size} bytes, {modified})"));
        }
    }

    let mut result = format!("\u{1F4C1} {}\n\n", resolved.display());

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

    result.push_str(&format!(
        "\nTotal: {} directories, {} files",
        folders.len(),
        files.len()
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
