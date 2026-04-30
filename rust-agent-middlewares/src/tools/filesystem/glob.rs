use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::path::Path;

use super::resolve_path;

/// glob_files tool - 与 TypeScript glob_tool 对齐
pub struct GlobFilesTool {
    pub cwd: String,
}

impl GlobFilesTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 最多返回的文件数，防止撑爆 LLM context window
const MAX_RESULTS: usize = 1_000;

const GLOB_FILES_DESCRIPTION: &str = r#"Fast file pattern matching tool that works with any codebase size. Supports glob patterns like "**/*.js" or "src/**/*.ts". Returns matching file paths sorted by modification time.

Usage:
- Use this tool when you need to find files by name patterns
- Returns file paths sorted by modification time (most recently modified first)
- Maximum 1000 results returned; results are truncated beyond this limit with a notice
- Common directories like node_modules, .git, target, dist, build are automatically excluded from results
- The path parameter is optional; defaults to the current working directory
- For searching file contents, use search_files_rg instead

When to use:
- Use glob_files when searching for files by name pattern (e.g., find all TypeScript files, find a specific config file)
- Use search_files_rg when searching for content within files (e.g., find where a function is defined)
- For open-ended searches requiring multiple rounds, consider using a sub-agent via launch_agent"#;

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | ".git"
            | "dist"
            | "build"
            | ".next"
            | ".turbo"
            | "coverage"
            | ".nyc_output"
            | "temp"
            | ".cache"
            | "vendor"
            | "venv"
            | "__pycache__"
            | "target"
            | "out"
            | ".output"
    )
}

fn glob_match(pattern: &str, path: &str) -> bool {
    glob::Pattern::new(pattern)
        .map(|p| p.matches(path))
        .unwrap_or(false)
}

fn collect_files(base: &Path, pattern: &str, results: &mut Vec<String>) {
    let walker = walkdir::WalkDir::new(base)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !should_skip_dir(&name)
            } else {
                true
            }
        });

    for entry in walker {
        match entry {
            Ok(e) => {
                if e.file_type().is_file() {
                    let abs_path = e.path().to_string_lossy().to_string();
                    if let Ok(rel) = e.path().strip_prefix(base) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if glob_match(pattern, &rel_str) {
                            results.push(abs_path);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "glob walk error (skipped)");
            }
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for GlobFilesTool {
    fn name(&self) -> &str {
        "glob_files"
    }

    fn description(&self) -> &str {
        GLOB_FILES_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g. \"**/*.js\", \"src/**/*.rs\", \"*.config.json\"). Use ** for recursive matching"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Absolute path or relative to cwd. If not specified, the current working directory is used"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or("Missing pattern parameter")?;

        let search_root = if let Some(p) = input["path"].as_str() {
            resolve_path(&self.cwd, p)
        } else {
            Path::new(&self.cwd).to_path_buf()
        };

        if !search_root.exists() {
            return Ok(format!(
                "Error: Directory not found: {}",
                search_root.display()
            ));
        }

        let mut results = Vec::new();
        collect_files(&search_root, pattern, &mut results);

        results.sort_by(|a, b| {
            let ta = std::fs::metadata(a).and_then(|m| m.modified()).ok();
            let tb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
            tb.cmp(&ta)
        });

        if results.is_empty() {
            Ok("No files found.".to_string())
        } else {
            if results.len() > MAX_RESULTS {
                let truncated = &results[..MAX_RESULTS];
                Ok(format!(
                    "{}\n\n[Output truncated: {} files total, showing first {}]",
                    truncated.join("\n"),
                    results.len(),
                    MAX_RESULTS
                ))
            } else {
                Ok(results.join("\n"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_glob_match_simple() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.rs"}))
            .await
            .unwrap();
        assert!(result.contains("a.rs"), "should find a.rs: {result}");
        assert!(result.contains("b.rs"), "should find b.rs: {result}");
        assert!(!result.contains("c.txt"), "should not find c.txt: {result}");
    }

    #[tokio::test]
    async fn test_glob_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.go"}))
            .await
            .unwrap();
        assert_eq!(result, "No files found.");
    }

    #[tokio::test]
    async fn test_glob_recursive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/d.rs"), "").unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "**/*.rs"}))
            .await
            .unwrap();
        assert!(result.contains("d.rs"), "should find nested d.rs: {result}");
    }

    #[tokio::test]
    async fn test_glob_dir_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.rs", "path": "nonexistent_dir"}))
            .await
            .unwrap();
        assert!(
            result.contains("Directory not found"),
            "should report missing dir: {result}"
        );
    }

    #[test]
    fn test_description_extended() {
        let tool = GlobFilesTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(
            desc.contains("modification time"),
            "description 应提及排序规则"
        );
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
}
