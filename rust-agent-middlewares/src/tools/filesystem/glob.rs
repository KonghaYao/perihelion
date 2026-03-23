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

    for entry in walker.flatten() {
        if entry.file_type().is_file() {
            let abs_path = entry.path().to_string_lossy().to_string();
            if let Ok(rel) = entry.path().strip_prefix(base) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if glob_match(pattern, &rel_str) {
                    results.push(abs_path);
                }
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
        "Fast file pattern matching tool. Supports glob patterns like \"**/*.rs\". Parameters (JSON): pattern: string (required), path: string (optional)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern to match files, e.g. \"**/*.rs\"" },
                "path":    { "type": "string", "description": "Directory to search in (absolute or relative to cwd, default: cwd)" }
            },
            "required": ["pattern"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
            Ok(results.join("\n"))
        }
    }
}
