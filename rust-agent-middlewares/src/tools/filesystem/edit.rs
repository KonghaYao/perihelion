use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

/// edit_file tool (replace) - 与 TypeScript replace_tool 对齐
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
        "edit_file"
    }

    fn description(&self) -> &str {
        "Performs exact string replacements in files. Parameters (JSON): file_path: string (required), old_string: string (required), new_string: string (required), replace_all: bool (optional)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path":   { "type": "string",  "description": "Path to the file to modify" },
                "old_string":  { "type": "string",  "description": "The exact text to replace" },
                "new_string":  { "type": "string",  "description": "The replacement text" },
                "replace_all": { "type": "boolean", "description": "Replace all occurrences (default false)" }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
            std::fs::write(&resolved, new_content)?;
            Ok(format!(
                "File {} has been edited successfully. Replaced all occurrences of old_string.",
                resolved.display()
            ))
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
            std::fs::write(&resolved, new_content)?;
            Ok(format!(
                "File {} has been edited successfully. Replaced single occurrence of old_string.",
                resolved.display()
            ))
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
            .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}))
            .await
            .unwrap();
        assert!(result.contains("edited successfully"), "unexpected: {result}");
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
        assert!(result.contains("not found"), "should report not found: {result}");
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
            .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}))
            .await
            .unwrap();
        assert!(result.contains("not unique"), "should report ambiguity: {result}");
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "ghost.txt", "old_string": "x", "new_string": "y"}))
            .await
            .unwrap();
        assert!(result.contains("File not found"), "should report file not found: {result}");
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
        assert!(result.contains("cannot be empty"), "empty old_string should be rejected: {result}");
        // 文件内容不应被修改
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "hello world", "file should not be modified");
    }
}
