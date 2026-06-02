use peri_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

const WRITE_FILE_DESCRIPTION: &str = r#"Writes a file to the local filesystem.

Usage:
- This tool will overwrite the existing file if there is one at the provided path
- If this is an existing file, you MUST use the Read tool first to read the file's contents. This tool will fail if you did not read the file first
- ALWAYS prefer editing existing files in the codebase. DO NOT create new files unless explicitly required
- The file_path parameter must be an absolute path, not a relative path
- Parent directories are created automatically if they do not exist

Notes:
- Uses atomic write (write to temp file then rename) to prevent data loss on crash
- NEVER create documentation files (*.md) or README files unless explicitly requested by the User
- Only use emojis if the User explicitly requests it. Avoid writing emojis to files unless asked"#;

/// Write tool - 与 TypeScript write_tool 对齐
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
        "Write"
    }

    fn description(&self) -> &str {
        WRITE_FILE_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The full content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or("The 'file_path' parameter is required for the Write tool.")?;
        let content = input["content"]
            .as_str()
            .ok_or("The 'content' parameter is required for the Write tool.")?;

        let resolved = resolve_path(&self.cwd, file_path);
        let line_count = content.lines().count();

        if let Some(parent) = resolved.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 原子写入：先写临时文件再 rename，防止崩溃时丢失数据
        // 使用随机后缀避免并发写入冲突
        let tmp_ext = format!("tmp.{}", uuid::Uuid::now_v7());
        let tmp_path = resolved.with_extension(tmp_ext);
        if let Err(e) = std::fs::write(&tmp_path, content) {
            return Err(format!("Error writing file: {e}").into());
        }
        match std::fs::rename(&tmp_path, &resolved) {
            Ok(_) => {
                let rel = resolved
                    .strip_prefix(&self.cwd)
                    .unwrap_or(&resolved)
                    .display()
                    .to_string();
                let lines_label = if line_count == 1 { "line" } else { "lines" };
                Ok(format!("Wrote {} {} {}", line_count, lines_label, rel))
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_path);
                Err(format!("Error renaming temp file: {e}").into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("write_test.rs");
}
