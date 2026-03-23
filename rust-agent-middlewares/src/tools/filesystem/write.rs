use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

/// write_file tool - 与 TypeScript write_tool 对齐
pub struct WriteFileTool {
    pub cwd: String,
}

impl WriteFileTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

#[async_trait::async_trait]
impl BaseTool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Writes a file to the local filesystem. Relative paths are resolved based on the current working directory (cwd). Parameters (JSON): file_path: string (required), content: string (required)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the file (absolute or relative to cwd)" },
                "content":   { "type": "string", "description": "Content to write to the file" }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or("Missing file_path parameter")?;
        let content = input["content"]
            .as_str()
            .ok_or("Missing content parameter")?;

        let resolved = resolve_path(&self.cwd, file_path);

        if let Some(parent) = resolved.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        match std::fs::write(&resolved, content) {
            Ok(_) => Ok(format!(
                "File {} has been written successfully.",
                resolved.display()
            )),
            Err(e) => Ok(format!("Error writing file: {e}")),
        }
    }
}
