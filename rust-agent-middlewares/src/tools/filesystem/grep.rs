use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// search_files_rg tool - 与 TypeScript grep_tool 对齐
pub struct SearchFilesRgTool {
    pub cwd: String,
}

impl SearchFilesRgTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

fn resolve_last_path_arg(args: &mut [String], cwd: &str) {
    if let Some(last) = args.last_mut() {
        if !last.starts_with('-') {
            let p = Path::new(last.as_str());
            if !p.is_absolute() {
                *last = Path::new(cwd).join(p).to_string_lossy().to_string();
            }
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for SearchFilesRgTool {
    fn name(&self) -> &str {
        "search_files_rg"
    }

    fn description(&self) -> &str {
        "Ripgrep (rg) - A fast text search tool. Parameters (JSON): args: string[] (required) - ripgrep arguments array e.g. [\"-n\", \"pattern\", \"./\"], head_limit: number (optional)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ripgrep arguments array. Format: [OPTIONS..., PATTERN, PATH]. Example: [\"-n\", \"fn main\", \"src/\"]"
                },
                "head_limit": { "type": "number", "description": "Limit output to first N lines (default 500)" }
            },
            "required": ["args"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args_val = input["args"]
            .as_array()
            .ok_or("Missing args parameter (array of strings)")?;

        if args_val.is_empty() {
            return Ok("Error: No arguments provided. Please provide ripgrep arguments.".to_string());
        }

        let mut args: Vec<String> = args_val
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        let head_limit = input["head_limit"].as_u64().unwrap_or(500) as usize;

        resolve_last_path_arg(&mut args, &self.cwd);

        let rg_bin = which_rg();

        let output = timeout(
            Duration::from_secs(15),
            Command::new(rg_bin)
                .args(&args)
                .current_dir(&self.cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match output {
            Err(_) => Ok(
                "Error: Search timed out after 15 seconds. Please use a more specific pattern."
                    .to_string(),
            ),
            Ok(Err(e)) => Ok(format!("Error executing ripgrep: {e}")),
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();

                if !out.status.success() && stdout.is_empty() {
                    if stderr.is_empty() {
                        return Ok("No matches found.".to_string());
                    }
                    return Ok(format!("Error executing ripgrep: {stderr}"));
                }

                let result = if stdout.is_empty() {
                    "No matches found.".to_string()
                } else {
                    let lines: Vec<&str> = stdout.split('\n').collect();
                    if lines.len() > head_limit {
                        lines[..head_limit].join("\n")
                    } else {
                        stdout
                    }
                };

                Ok(result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_files_rg_hit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle in a haystack\nother line").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-n", "needle", "./"]}))
            .await
            .unwrap();
        if result.starts_with("Error executing ripgrep") {
            return; // rg not available in this environment
        }
        assert!(result.contains("needle"), "should find needle: {result}");
    }

    #[tokio::test]
    async fn test_search_files_rg_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "haystack only").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-n", "zzz_not_here", "./"]}))
            .await
            .unwrap();
        if result.starts_with("Error executing ripgrep") {
            return;
        }
        assert!(result.contains("No matches found"), "should report no match: {result}");
    }

    #[tokio::test]
    async fn test_search_files_rg_empty_args() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": []}))
            .await
            .unwrap();
        assert!(result.contains("No arguments"), "should report missing args: {result}");
    }

    #[tokio::test]
    async fn test_search_files_rg_regex() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle123\nneedle456").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-n", "needle[0-9]+", "./"]}))
            .await
            .unwrap();
        if result.starts_with("Error executing ripgrep") {
            return;
        }
        assert!(result.contains("needle"), "regex should match: {result}");
    }
}

fn which_rg() -> &'static str {
    static RG_PATH: OnceLock<&'static str> = OnceLock::new();
    RG_PATH.get_or_init(|| {
        for candidate in &[
            "rg",
            "/usr/local/bin/rg",
            "/opt/homebrew/bin/rg",
            "/usr/bin/rg",
        ] {
            if std::process::Command::new(candidate)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
            {
                return *candidate;
            }
        }
        "rg"
    })
}
