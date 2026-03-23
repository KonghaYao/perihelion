use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

/// read_file tool - 与 TypeScript read_tool 对齐
pub struct ReadFileTool {
    pub cwd: String,
}

impl ReadFileTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

const MAX_LINES: usize = 2000;

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" | "tiff"
            | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
            | "zip" | "rar" | "7z" | "tar" | "gz"
            | "mp3" | "wav" | "ogg" | "flac"
            | "mp4" | "avi" | "mkv" | "mov"
            | "exe" | "dll" | "so" | "dylib" | "bin" | "class"
    )
}

#[async_trait::async_trait]
impl BaseTool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Reads a file from the local filesystem. Relative paths are resolved based on the current working directory (cwd). Parameters (JSON): file_path: string (required), offset: number (optional), limit: number (optional)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the file (absolute or relative to cwd)" },
                "offset":    { "type": "number", "description": "Line number to start reading from (default 0)" },
                "limit":     { "type": "number", "description": "Number of lines to read (default 2000)" }
            },
            "required": ["file_path"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or("Missing file_path parameter")?;

        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(MAX_LINES as u64) as usize;

        let resolved = resolve_path(&self.cwd, file_path);

        if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
            if is_binary_extension(&ext.to_lowercase()) {
                return Ok(format!(
                    "[BINARY FILE DETECTED]\n\nFile type: .{ext}\nFile path: {}\n\nThis is a binary file and cannot be displayed as text.",
                    resolved.display()
                ));
            }
        }

        let content = match std::fs::read_to_string(&resolved) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(format!("Error: File not found at {file_path}"));
            }
            Err(e) => return Err(e.into()),
        };

        let lines: Vec<&str> = content.split('\n').collect();
        let start = offset;
        let end = (start + limit).min(lines.len());
        let selected = &lines[start..end];

        let numbered: Vec<String> = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        Ok(numbered.join("\n"))
    }
}
