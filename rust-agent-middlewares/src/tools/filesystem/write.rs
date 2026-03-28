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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_file_creates_new() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "new.txt", "content": "hello"}))
            .await
            .unwrap();
        let content = std::fs::read_to_string(dir.path().join("new.txt")).unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_write_file_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "old").unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "f.txt", "content": "new"}))
            .await
            .unwrap();
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn test_write_file_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "sub/dir/file.txt", "content": "deep"}))
            .await
            .unwrap();
        assert!(dir.path().join("sub/dir/file.txt").exists());
    }

    #[tokio::test]
    async fn test_write_file_missing_content_param() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"file_path": "f.txt"})).await;
        assert!(result.is_err(), "missing content should return Err");
    }

    #[tokio::test]
    async fn test_write_file_success_message() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "msg.txt", "content": "x"}))
            .await
            .unwrap();
        assert!(result.contains("written successfully"), "unexpected message: {result}");
    }
}
