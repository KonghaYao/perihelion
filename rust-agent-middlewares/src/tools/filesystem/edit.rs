use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::path::Path;

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

        let resolved = if Path::new(file_path).is_absolute() {
            Path::new(file_path).to_path_buf()
        } else {
            Path::new(&self.cwd).join(file_path)
        };

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
