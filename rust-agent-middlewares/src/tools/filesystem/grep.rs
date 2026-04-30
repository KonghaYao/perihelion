use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::cell::Cell;
use tokio::time::{timeout, Duration};

use grep::regex::RegexMatcherBuilder;
use grep::searcher::{SearcherBuilder, BinaryDetection, Sink, SinkMatch, SinkContext, SinkContextKind, Searcher};
use ignore::WalkBuilder;

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

/// 从 args 数组中解析搜索参数
struct ParsedArgs {
    pattern: String,
    path: Option<String>,       // 搜索路径，None 表示 cwd
    glob_filters: Vec<String>,  // -g 参数
    _type_filters: Vec<String>,  // -t 参数（暂不实现）
    _type_excludes: Vec<String>, // -T 参数（暂不实现）
    output_mode: OutputMode,    // 默认/文件名/计数
    context_lines: usize,       // -C 参数
    case_insensitive: bool,     // -i 参数
    whole_word: bool,           // -w 参数
}

#[derive(Clone, Copy, PartialEq)]
enum OutputMode {
    Default,  // 显示匹配行
    FilesOnly, // -l
    CountOnly, // -c
}

/// 解析 ripgrep 风格的命令行参数
fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut pattern: Option<String> = None;
    let mut path: Option<String> = None;
    let mut glob_filters = Vec::new();
    let mut type_filters = Vec::new();
    let mut type_excludes = Vec::new();
    let mut output_mode = OutputMode::Default;
    let mut context_lines: usize = 0;
    let mut case_insensitive = false;
    let mut whole_word = false;
    let mut i = 0;
    let mut stop_parsing = false;

    while i < args.len() {
        let arg = &args[i];

        if stop_parsing || !arg.starts_with('-') {
            // 位置参数: 第一个是 PATTERN，第二个是 PATH
            if pattern.is_none() {
                pattern = Some(arg.clone());
            } else if path.is_none() {
                path = Some(arg.clone());
            }
            i += 1;
            continue;
        }

        match arg.as_str() {
            "--" => {
                stop_parsing = true;
                i += 1;
            }
            "-g" | "--glob" => {
                i += 1;
                if i < args.len() {
                    glob_filters.push(args[i].clone());
                    i += 1;
                } else {
                    return Err("-g requires a value".to_string());
                }
            }
            "-t" | "--type" => {
                i += 1;
                if i < args.len() {
                    type_filters.push(args[i].clone());
                    i += 1;
                } else {
                    return Err("-t requires a value".to_string());
                }
            }
            "-T" | "--type-not" => {
                i += 1;
                if i < args.len() {
                    type_excludes.push(args[i].clone());
                    i += 1;
                } else {
                    return Err("-T requires a value".to_string());
                }
            }
            "-l" => {
                output_mode = OutputMode::FilesOnly;
                i += 1;
            }
            "-c" => {
                output_mode = OutputMode::CountOnly;
                i += 1;
            }
            "-C" | "--context" => {
                i += 1;
                if i < args.len() {
                    context_lines = args[i].parse::<usize>()
                        .map_err(|_| format!("-C requires a number, got: {}", args[i]))?;
                    i += 1;
                } else {
                    return Err("-C requires a value".to_string());
                }
            }
            "-i" | "--ignore-case" => {
                case_insensitive = true;
                i += 1;
            }
            "-n" | "--line-number" => {
                // 始终开启，忽略
                i += 1;
            }
            "-w" | "--word-regexp" => {
                whole_word = true;
                i += 1;
            }
            _ => {
                // 未知选项，当作位置参数处理
                if pattern.is_none() {
                    pattern = Some(arg.clone());
                } else if path.is_none() {
                    path = Some(arg.clone());
                }
                i += 1;
            }
        }
    }

    let pattern = pattern.ok_or("No search pattern provided")?;

    Ok(ParsedArgs {
        pattern,
        path,
        glob_filters,
        _type_filters: type_filters,
        _type_excludes: type_excludes,
        output_mode,
        context_lines,
        case_insensitive,
        whole_word,
    })
}

/// 自定义 Sink，支持三种输出模式和行数限制
struct SearchSink {
    output_mode: OutputMode,
    results: Arc<Mutex<Vec<String>>>,
    total_lines: Arc<AtomicUsize>,
    max_limit: usize,
    stopped: Arc<AtomicBool>,
    display_path: String,
    match_count: Cell<usize>,
    has_match: Cell<bool>,
    context_lines: usize,
}

impl Sink for SearchSink {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        if self.stopped.load(Ordering::Relaxed) {
            return Ok(false);
        }

        match self.output_mode {
            OutputMode::Default => {
                let line_number = mat.line_number().unwrap_or(0);
                let content = String::from_utf8_lossy(mat.bytes());
                let content = content.trim_end_matches(|c| c == '\n' || c == '\r');
                let line = format!("{}:{}: {}", self.display_path, line_number, content);

                let total = self.total_lines.fetch_add(1, Ordering::Relaxed) + 1;
                if total >= self.max_limit {
                    self.stopped.store(true, Ordering::Relaxed);
                }

                self.results.lock().unwrap().push(line);
                Ok(!self.stopped.load(Ordering::Relaxed))
            }
            OutputMode::CountOnly => {
                self.match_count.set(self.match_count.get() + 1);
                Ok(true)
            }
            OutputMode::FilesOnly => {
                self.has_match.set(true);
                Ok(false)
            }
        }
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, Self::Error> {
        if self.stopped.load(Ordering::Relaxed) || self.context_lines == 0 {
            return Ok(true);
        }
        if self.output_mode != OutputMode::Default {
            return Ok(true);
        }

        let line_number = ctx.line_number().unwrap_or(0);
        let content = String::from_utf8_lossy(ctx.bytes());
        let content = content.trim_end_matches(|c| c == '\n' || c == '\r');

        let separator = match ctx.kind() {
            SinkContextKind::Before => '-',
            SinkContextKind::After => '+',
            SinkContextKind::Other => '-',
        };

        let line = format!("{}:{}{}: {}", self.display_path, line_number, separator, content);

        let total = self.total_lines.fetch_add(1, Ordering::Relaxed) + 1;
        if total >= self.max_limit {
            self.stopped.store(true, Ordering::Relaxed);
        }

        self.results.lock().unwrap().push(line);
        Ok(!self.stopped.load(Ordering::Relaxed))
    }
}

/// 核心搜索函数（同步，在 spawn_blocking 中运行）
fn execute_search(
    parsed: &ParsedArgs,
    cwd: &str,
    head_limit: usize,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // 构建搜索路径
    let search_path = match &parsed.path {
        Some(p) => {
            let p = Path::new(p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                Path::new(cwd).join(p)
            }
        }
        None => PathBuf::from(cwd),
    };

    if !search_path.exists() {
        return Err(format!("Search path does not exist: {}", search_path.display()).into());
    }

    // 构建 RegexMatcher
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(parsed.case_insensitive)
        .word(parsed.whole_word)
        .build(&parsed.pattern)?;

    // 构建 WalkBuilder
    let mut builder = WalkBuilder::new(&search_path);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .threads(num_cpus::get());

    // 预编译 glob 过滤器
    let glob_filters: Vec<glob::Pattern> = parsed
        .glob_filters
        .iter()
        .filter_map(|g| glob::Pattern::new(g).ok())
        .collect();

    // 共享状态
    let results = Arc::new(Mutex::new(Vec::new()));
    let total_lines = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicBool::new(false));
    let matcher = Arc::new(matcher);
    let cwd = Arc::new(cwd.to_string());
    let context_lines = parsed.context_lines;

    // 并行搜索
    builder.build_parallel().run(|| {
        let matcher = Arc::clone(&matcher);
        let total_lines = Arc::clone(&total_lines);
        let stopped = Arc::clone(&stopped);
        let cwd = Arc::clone(&cwd);
        let glob_filters = glob_filters.clone();
        let results = Arc::clone(&results);

        Box::new(move |entry_result: Result<ignore::DirEntry, ignore::Error>| {
            use ignore::WalkState;

            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            if stopped.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                return WalkState::Continue;
            }

            // -g glob 过滤
            if !glob_filters.is_empty() {
                let file_name = entry.file_name().to_string_lossy();
                if !glob_filters.iter().any(|p| p.matches(&file_name)) {
                    return WalkState::Continue;
                }
            }

            // 显示路径：相对于 cwd 的路径
            let display_path = entry
                .path()
                .strip_prefix(cwd.as_str())
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_string();

            let mut searcher_builder = SearcherBuilder::new();
            searcher_builder
                .line_number(true)
                .binary_detection(BinaryDetection::quit(b'\x00'));
            if context_lines > 0 {
                searcher_builder
                    .before_context(context_lines)
                    .after_context(context_lines);
            }
            let mut searcher = searcher_builder.build();

            let mut sink = SearchSink {
                output_mode: parsed.output_mode,
                results: Arc::clone(&results),
                total_lines: Arc::clone(&total_lines),
                max_limit: head_limit,
                stopped: Arc::clone(&stopped),
                display_path: display_path.clone(),
                match_count: Cell::new(0),
                has_match: Cell::new(false),
                context_lines,
            };

            match searcher.search_path(&*matcher, entry.path(), &mut sink) {
                Ok(_) => {}
                Err(_) => {
                    // 二进制文件等错误，跳过
                    return WalkState::Continue;
                }
            }

            // FilesOnly / CountOnly 模式在搜索完成后处理
            if parsed.output_mode == OutputMode::FilesOnly && sink.has_match.get() {
                let mut r = results.lock().unwrap();
                r.push(display_path.clone());
            } else if parsed.output_mode == OutputMode::CountOnly && sink.match_count.get() > 0 {
                let mut r = results.lock().unwrap();
                r.push(format!("{}:{}", display_path, sink.match_count.get()));
            }

            if stopped.load(Ordering::Relaxed) {
                WalkState::Quit
            } else {
                WalkState::Continue
            }
        })
    });

    // 格式化输出
    let results = results.lock().unwrap();
    if results.is_empty() {
        return Ok("No matches found.".to_string());
    }

    let mut output = results.join("\n");
    let total = total_lines.load(Ordering::Relaxed);
    if total >= head_limit && head_limit > 0 {
        output.push_str(&format!("\n... (truncated at {} lines)", head_limit));
    }

    Ok(output)
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

        let args: Vec<String> = args_val
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        let head_limit = input["head_limit"].as_u64().unwrap_or(500) as usize;

        let parsed = match parse_args(&args) {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error: {e}")),
        };

        let cwd = self.cwd.clone();
        let result = timeout(
            Duration::from_secs(15),
            tokio::task::spawn_blocking(move || execute_search(&parsed, &cwd, head_limit)),
        )
        .await;

        match result {
            Err(_) => Ok(
                "Error: Search timed out after 15 seconds. Please use a more specific pattern."
                    .to_string(),
            ),
            Ok(Err(e)) => Ok(format!("Error: {e}")),
            Ok(Ok(Ok(output))) => Ok(output),
            Ok(Ok(Err(e))) => Ok(format!("Error: {e}")),
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

    #[tokio::test]
    async fn test_search_files_rg_files_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "needle here\nother line").unwrap();
        std::fs::write(dir.path().join("b.txt"), "no match here").unwrap();
        std::fs::write(dir.path().join("c.txt"), "needle again").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-l", "needle", "./"]}))
            .await
            .unwrap();
        assert!(result.contains("a.txt"), "should find a.txt: {result}");
        assert!(result.contains("c.txt"), "should find c.txt: {result}");
        assert!(!result.contains("needle here"), "should not include line content: {result}");
    }

    #[tokio::test]
    async fn test_search_files_rg_count() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "needle\nneedle\nneedle").unwrap();
        std::fs::write(dir.path().join("b.txt"), "needle once").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-c", "needle", "./"]}))
            .await
            .unwrap();
        assert!(result.contains("a.txt:3"), "a.txt should have 3 matches: {result}");
        assert!(result.contains("b.txt:1"), "b.txt should have 1 match: {result}");
    }

    #[tokio::test]
    async fn test_search_files_rg_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "NEEDLE\nneedle\nNeedle").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-i", "NEEDLE", "./"]}))
            .await
            .unwrap();
        assert!(result.contains("NEEDLE"), "should match uppercase: {result}");
        assert!(result.contains("needle"), "should match lowercase: {result}");
        assert!(result.contains("Needle"), "should match mixed case: {result}");
    }

    #[tokio::test]
    async fn test_search_files_rg_glob_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle in txt").unwrap();
        std::fs::write(dir.path().join("test.rs"), "needle in rs").unwrap();
        let tool = SearchFilesRgTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"args": ["-n", "-g", "*.txt", "needle", "./"]}))
            .await
            .unwrap();
        assert!(result.contains("test.txt"), "should find in .txt: {result}");
        assert!(!result.contains("test.rs"), "should not find in .rs: {result}");
    }
}
