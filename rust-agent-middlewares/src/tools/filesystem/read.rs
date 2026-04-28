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
        if offset >= lines.len() {
            return Ok(format!(
                "Error: offset {} exceeds file length ({} lines)",
                offset,
                lines.len()
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_file_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "hello\nworld").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"file_path": "file.txt"})).await.unwrap();
        assert!(result.contains("1\thello"), "should contain line 1: {result}");
        assert!(result.contains("2\tworld"), "should contain line 2: {result}");
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"file_path": "nonexistent.txt"})).await.unwrap();
        assert!(result.contains("File not found"), "should report not found: {result}");
    }

    #[tokio::test]
    async fn test_read_file_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, "L1\nL2\nL3\nL4\nL5").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "lines.txt", "offset": 2, "limit": 2}))
            .await
            .unwrap();
        // offset=2 → starts at index 2 (L3), limit=2 → L3 and L4
        assert!(result.contains("3\tL3"), "should contain line 3: {result}");
        assert!(result.contains("4\tL4"), "should contain line 4: {result}");
        assert!(!result.contains("L1"), "should not contain L1");
        assert!(!result.contains("L5"), "should not contain L5");
    }

    #[tokio::test]
    async fn test_read_file_binary_extension() {
        let dir = tempfile::tempdir().unwrap();
        // Binary extension check happens before file read, no need to create the file
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"file_path": "image.png"})).await.unwrap();
        assert!(result.contains("BINARY FILE DETECTED"), "should detect binary: {result}");
    }

    #[tokio::test]
    async fn test_read_file_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abs.txt");
        std::fs::write(&path, "absolute").unwrap();
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.contains("absolute"), "should read via absolute path: {result}");
    }

    #[tokio::test]
    async fn test_read_file_offset_exceeds_length() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("short.txt"), "one\ntwo").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "short.txt", "offset": 999}))
            .await
            .unwrap();
        assert!(
            result.contains("exceeds file length"),
            "offset 超出文件长度应返回错误而非 panic: {result}"
        );
    }
}
