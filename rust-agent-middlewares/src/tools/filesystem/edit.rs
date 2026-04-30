use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

const EDIT_FILE_DESCRIPTION: &str = r#"Performs exact string replacements in files.

Usage:
- You must use your Read tool at least once in the conversation before editing. This tool will fail if you attempt an edit without reading the file
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix
- ALWAYS prefer editing existing files in the codebase. DO NOT create new files unless explicitly required
- The file_path parameter must be an absolute path, not a relative path
- The old_string parameter must match exactly, including all whitespace and indentation
- The edit will FAIL if old_string is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use replace_all to change every instance of old_string
- Use replace_all for replacing and renaming strings across the file

Error handling:
- old_string not found: returns an error indicating the string does not exist in the file
- old_string not unique: returns an error with the count of occurrences, suggesting more context or replace_all
- old_string is empty: returns an error rejecting the operation
- File not found: returns an error indicating the path does not exist"#;

/// Edit tool (replace) - 与 TypeScript replace_tool 对齐
pub struct EditFileTool {
    pub cwd: String,
}

impl EditFileTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

#[async_trait::async_trait]
impl BaseTool for EditFileTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        EDIT_FILE_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace. Must match EXACTLY including all whitespace, indentation, and newlines. The edit will fail if old_string is not unique in the file unless replace_all is true"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences of old_string. If false (default), replace only the first occurrence. Use this to rename variables or update repeated patterns across the file"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or("Missing file_path parameter")?;
        let old_string = input["old_string"]
            .as_str()
            .ok_or("Missing old_string parameter")?;
        let new_string = input["new_string"]
            .as_str()
            .ok_or("Missing new_string parameter")?;
        let replace_all = input["replace_all"].as_bool().unwrap_or(false);

        if old_string.is_empty() {
            return Ok("Error: old_string cannot be empty".to_string());
        }

        let resolved = resolve_path(&self.cwd, file_path);

        let content = match std::fs::read_to_string(&resolved) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(format!("Error: File not found at {file_path}"));
            }
            Err(e) => return Err(e.into()),
        };

        if replace_all {
            if !content.contains(old_string) {
                return Ok(format!(
                    "Error: old_string not found in {}",
                    resolved.display()
                ));
            }
            let new_content = content.replace(old_string, new_string);
            // 原子写入：先写临时文件再 rename
            let tmp_ext = format!("tmp.{}", uuid::Uuid::now_v7());
            let tmp_path = resolved.with_extension(tmp_ext);
            std::fs::write(&tmp_path, &new_content)?;
            match std::fs::rename(&tmp_path, &resolved) {
                Ok(_) => Ok(format!(
                    "File {} has been edited successfully. Replaced all occurrences of old_string.",
                    resolved.display()
                )),
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    Err(format!("Error renaming temp file: {e}").into())
                }
            }
        } else {
            let occurrences = content.matches(old_string).count();
            if occurrences == 0 {
                return Ok(format!(
                    "Error: old_string not found in {}",
                    resolved.display()
                ));
            }
            if occurrences > 1 {
                return Ok(format!(
                    "Error: old_string is not unique in {} (found {} occurrences). \
                     Please provide more context or set replace_all to true.",
                    resolved.display(),
                    occurrences
                ));
            }
            let new_content = content.replacen(old_string, new_string, 1);
            // 原子写入：先写临时文件再 rename
            let tmp_ext = format!("tmp.{}", uuid::Uuid::now_v7());
            let tmp_path = resolved.with_extension(tmp_ext);
            std::fs::write(&tmp_path, &new_content)?;
            match std::fs::rename(&tmp_path, &resolved) {
                Ok(_) => Ok(format!(
                    "File {} has been edited successfully. Replaced single occurrence of old_string.",
                    resolved.display()
                )),
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    Err(format!("Error renaming temp file: {e}").into())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_edit_file_single_replace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello foo world").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(
                serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}),
            )
            .await
            .unwrap();
        assert!(
            result.contains("edited successfully"),
            "unexpected: {result}"
        );
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "hello bar world");
    }

    #[tokio::test]
    async fn test_edit_file_old_string_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello world").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "missing", "new_string": "x"}))
            .await
            .unwrap();
        assert!(
            result.contains("not found"),
            "should report not found: {result}"
        );
    }

    #[tokio::test]
    async fn test_edit_file_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x x x").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": "x",
            "new_string": "y",
            "replace_all": true
        }))
        .await
        .unwrap();
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "y y y");
    }

    #[tokio::test]
    async fn test_edit_file_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "foo and foo").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(
                serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}),
            )
            .await
            .unwrap();
        assert!(
            result.contains("not unique"),
            "should report ambiguity: {result}"
        );
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(
                serde_json::json!({"file_path": "ghost.txt", "old_string": "x", "new_string": "y"}),
            )
            .await
            .unwrap();
        assert!(
            result.contains("File not found"),
            "should report file not found: {result}"
        );
    }

    #[tokio::test]
    async fn test_edit_file_empty_old_string_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello world").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "", "new_string": "x", "replace_all": true}))
            .await
            .unwrap();
        assert!(
            result.contains("cannot be empty"),
            "empty old_string should be rejected: {result}"
        );
        // 文件内容不应被修改
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "hello world", "file should not be modified");
    }

    #[test]
    fn test_description_extended() {
        let tool = EditFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("old_string"), "description 应提及 old_string");
        assert!(
            desc.contains("replace_all"),
            "description 应提及 replace_all"
        );
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    #[test]
    fn test_tool_name_is_Edit() {
        let tool = EditFileTool::new("/tmp");
        assert_eq!(tool.name(), "Edit");
    }
}
