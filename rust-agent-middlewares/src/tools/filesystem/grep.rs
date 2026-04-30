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

const SEARCH_FILES_RG_DESCRIPTION: &str = r#"A powerful search tool built on ripgrep (rg). Supports full regex syntax (e.g. "log.*Error", "function\s+\w+"). Filter files with glob parameter (e.g. "*.js", "*.{ts,tsx}") or type parameter (e.g. "js", "py", "rust", "go"). Use output_mode to control result format.

Usage:
- Use the args parameter as a ripgrep arguments array. Format: [OPTIONS..., PATTERN, PATH]
- If you need to identify a set of files, prefer glob_files over search_files_rg
- Supports full regex syntax — literal braces need escaping (use \{\} to find interface{} in Go code)
- Output includes line numbers by default when -n flag is used
- Search times out after 15 seconds; use more specific patterns for large codebases
- Maximum 500 lines of output; use head_limit parameter to adjust

Output modes:
- Default: shows matching lines with line numbers
- Use -l flag (in args) to list only file paths that contain matches
- Use -c flag (in args) to show match counts per file

When to use:
- Prefer search_files_rg over bash commands like grep or rg for content search
- Use glob_files for file name search, search_files_rg for content search
- For open-ended searches, start with the most specific query and broaden if needed"#;

/// 如果 args 中有至少 2 个非选项参数（PATTERN + PATH），将最后一个解析为绝对路径。
/// rg 的参数模型：[OPTIONS...] PATTERN [PATH]。只有 1 个非选项参数时是 PATTERN，不应解析。
fn resolve_last_path_arg(args: &mut [String], cwd: &str) {
    let non_option_count = args.iter().filter(|a| !a.starts_with('-')).count();
    if non_option_count < 2 {
        return; // 只有 PATTERN，没有 PATH 参数
    }
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
        SEARCH_FILES_RG_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ripgrep arguments as a string array. Format: [OPTIONS..., PATTERN, PATH]. Example: [\"-n\", \"fn main\", \"src/\"]. Supports regex patterns, glob filters (-g flag), file type filters (-t flag), context lines (-C flag), and all standard ripgrep options"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N matching lines (default 500). Use sparingly — large result sets waste context"
                }
            },
            "required": ["args"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args_val = input["args"]
            .as_array()
            .ok_or("Missing args parameter (array of strings)")?;

        if args_val.is_empty() {
            return Ok(
                "Error: No arguments provided. Please provide ripgrep arguments.".to_string(),
            );
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
        std::fs::write(
            dir.path().join("test.txt"),
            "needle in a haystack\nother line",
        )
        .unwrap();
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
        assert!(
            result.contains("No matches found"),
            "should report no match: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_files_rg_empty_args() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"args": []})).await.unwrap();
        assert!(
            result.contains("No arguments"),
            "should report missing args: {result}"
        );
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

    #[test]
    fn test_description_extended() {
        let tool = SearchFilesRgTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("regex"), "description 应提及正则支持");
        assert!(
            desc.contains("Output modes:"),
            "description 应包含 Output modes 段落"
        );
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
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
