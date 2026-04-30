use rust_create_agent::tools::BaseTool;
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

        // 原子写入：先写临时文件再 rename，防止崩溃时丢失数据
        // 使用随机后缀避免并发写入冲突
        let tmp_ext = format!("tmp.{}", uuid::Uuid::now_v7());
        let tmp_path = resolved.with_extension(tmp_ext);
        if let Err(e) = std::fs::write(&tmp_path, content) {
            return Err(format!("Error writing file: {e}").into());
        }
        match std::fs::rename(&tmp_path, &resolved) {
            Ok(_) => Ok(format!(
                "File {} has been written successfully.",
                resolved.display()
            )),
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
        assert!(
            result.contains("written successfully"),
            "unexpected message: {result}"
        );
    }

    #[tokio::test]
    async fn test_write_file_no_tmp_residual() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "clean.txt", "content": "data"}))
            .await
            .unwrap();
        // 原子写入后不应残留任何 .tmp.* 临时文件
        let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("clean.tmp."))
            .collect();
        assert!(tmp_files.is_empty(), "临时文件应在 rename 后被清除");
        assert!(dir.path().join("clean.txt").exists());
    }

    #[tokio::test]
    async fn test_write_file_error_propagates() {
        let dir = tempfile::tempdir().unwrap();
        // 在只读目录上写入应返回 Err
        let readonly_dir = dir.path().join("readonly");
        std::fs::create_dir(&readonly_dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o444))
                .unwrap();
        }
        let tool = WriteFileTool::new(readonly_dir.to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "sub/nope.txt", "content": "x"}))
            .await;
        #[cfg(unix)]
        assert!(result.is_err(), "写入只读目录应返回 Err");
    }

    #[test]
    fn test_description_extended() {
        let tool = WriteFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("atomic write"), "description 应提及原子写入");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    #[test]
    fn test_tool_name_is_Write() {
        let tool = WriteFileTool::new("/tmp");
        assert_eq!(tool.name(), "Write");
    }
}
