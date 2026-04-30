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

/// Grep tool - 与 Claude Code Grep 工具对齐
pub struct GrepTool {
    pub cwd: String,
}

impl GrepTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

const GREP_DESCRIPTION: &str = r#"A powerful search tool built on ripgrep. Supports full regex syntax (e.g. "log.*Error", "function\s+\w+"). Filter files with glob parameter (e.g. "*.js", "*.{ts,tsx}") or type parameter (e.g. "js", "py", "rust", "go"). Use output_mode to control result format.

Usage:
- Always provide pattern and output_mode parameters
- Use glob parameter for file type filtering (e.g. "*.js", "*.{ts,tsx}")
- Use type parameter for language-based filtering (e.g. "rust", "js", "py")
- Supports full regex syntax — literal braces need escaping (use \{\} to find interface{} in Go code)
- Output includes line numbers by default
- Search times out after 15 seconds; use more specific patterns for large codebases
- Default head_limit is 250 lines; use sparingly for large result sets

Output modes:
- "content": shows matching lines with line numbers (default)
- "files_with_matches": lists only file paths that contain matches
- "count": shows match counts per file

When to use:
- Prefer Grep over Bash commands like grep or rg for content search
- Use Glob for file name search, Grep for content search
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

/// Grep 工具的结构化输入参数，从 JSON 直接反序列化
struct GrepInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    type_filter: Option<String>,
    output_mode: String,           // "content" | "files_with_matches" | "count"
    case_insensitive: bool,        // 对应 -i，默认 false
    context: Option<usize>,        // 对应 -C
    line_number: bool,             // 对应 -n，默认 true
    multiline: bool,               // 对应 -U --multiline-dotall，默认 false
    head_limit: usize,             // 默认 250
    offset: Option<usize>,         // 跳过前 N 行
}

/// 将 type 参数（如 "rust"、"js"）映射为 glob 模式列表
fn type_to_glob(type_name: &str) -> Vec<&'static str> {
    match type_name {
        "rust" => vec!["*.rs"],
        "js" => vec!["*.js", "*.mjs"],
        "py" => vec!["*.py"],
        "go" => vec!["*.go"],
        "java" => vec!["*.java"],
        "ts" => vec!["*.ts", "*.tsx"],
        "c" => vec!["*.c", "*.h"],
        "cpp" => vec!["*.cpp", "*.hpp", "*.cc", "*.cxx"],
        "ruby" | "rb" => vec!["*.rb"],
        "swift" => vec!["*.swift"],
        "kotlin" | "kt" => vec!["*.kt", "*.kts"],
        "scala" => vec!["*.scala"],
        "html" => vec!["*.html", "*.htm"],
        "css" => vec!["*.css", "*.scss", "*.sass", "*.less"],
        "json" => vec!["*.json"],
        "yaml" | "yml" => vec!["*.yaml", "*.yml"],
        "markdown" | "md" => vec!["*.md", "*.mdx"],
        "sql" => vec!["*.sql"],
        "shell" | "sh" => vec!["*.sh", "*.bash", "*.zsh"],
        _ => vec![],
    }
}

impl GrepInput {
    /// 将结构化参数转译为搜索引擎所需的 ParsedArgs
    fn to_parsed_args(&self) -> Result<ParsedArgs, String> {
        // output_mode 字符串 → OutputMode 枚举
        let output_mode = match self.output_mode.as_str() {
            "content" => OutputMode::Default,
            "files_with_matches" => OutputMode::FilesOnly,
            "count" => OutputMode::CountOnly,
            other => return Err(format!("Invalid output_mode: '{}'. Must be 'content', 'files_with_matches', or 'count'", other)),
        };

        // 组装 glob 过滤器：用户提供的 glob + type 映射
        let mut glob_filters = Vec::new();
        if let Some(ref glob) = self.glob {
            // 支持多 glob 模式，如 "*.{ts,tsx}" 或 "*.rs"
            glob_filters.push(glob.clone());
        }
        if let Some(ref type_name) = self.type_filter {
            let type_globs = type_to_glob(type_name);
            for g in type_globs {
                glob_filters.push(g.to_string());
            }
        }

        Ok(ParsedArgs {
            pattern: self.pattern.clone(),
            path: self.path.clone(),
            glob_filters,
            _type_filters: vec![],
            _type_excludes: vec![],
            output_mode,
            context_lines: self.context.unwrap_or(0),
            case_insensitive: self.case_insensitive,
            whole_word: false,
        })
    }
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
impl BaseTool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        GREP_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for in file contents. Supports full regex syntax (e.g. \"log.*Error\", \"function\\s+\\w+\")"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path to search in. Defaults to current working directory if not specified"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. \"*.js\", \"*.{ts,tsx}\"). Only files matching the glob will be searched"
                },
                "type": {
                    "type": "string",
                    "description": "Filter files by type. Common values: \"rust\", \"js\", \"py\", \"go\", \"java\", \"ts\". More efficient than glob for type-based filtering"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: \"content\" shows matching lines with line numbers, \"files_with_matches\" lists only file paths, \"count\" shows match counts per file"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Enable case-insensitive search (default: false)"
                },
                "-C": {
                    "type": "number",
                    "description": "Number of context lines to show before and after each match"
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers (default: true)"
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines (default: false)"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N matching lines (default 250). Pass 0 for unlimited. Use sparingly — large result sets waste context"
                },
                "offset": {
                    "type": "number",
                    "description": "Skip first N lines of output before applying head_limit"
                }
            },
            "required": ["pattern", "output_mode"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return Ok("Error: Missing required parameter 'pattern'".to_string()),
        };
        let output_mode = match input.get("output_mode").and_then(|v| v.as_str()) {
            Some(m) => m.to_string(),
            None => return Ok("Error: Missing required parameter 'output_mode'".to_string()),
        };

        let grep_input = GrepInput {
            pattern,
            path: input.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()),
            glob: input.get("glob").and_then(|v| v.as_str()).map(|s| s.to_string()),
            type_filter: input.get("type").and_then(|v| v.as_str()).map(|s| s.to_string()),
            output_mode,
            case_insensitive: input.get("-i").and_then(|v| v.as_bool()).unwrap_or(false),
            context: input.get("-C").and_then(|v| v.as_u64()).map(|n| n as usize),
            line_number: input.get("-n").and_then(|v| v.as_bool()).unwrap_or(true),
            multiline: input.get("multiline").and_then(|v| v.as_bool()).unwrap_or(false),
            head_limit: input.get("head_limit").and_then(|v| v.as_u64()).unwrap_or(250) as usize,
            offset: input.get("offset").and_then(|v| v.as_u64()).map(|n| n as usize),
        };

        let parsed = match grep_input.to_parsed_args() {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error: {e}")),
        };

        let head_limit = grep_input.head_limit;

        let cwd = self.cwd.clone();
        let result = timeout(
            Duration::from_secs(15),
            tokio::task::spawn_blocking(move || execute_search(&parsed, &cwd, head_limit)),
        ).await;

        // offset 后处理（在超时/结果后应用）
        let output = match result {
            Err(_) => return Ok("Error: Search timed out after 15 seconds. Please use a more specific pattern.".to_string()),
            Ok(Err(e)) => return Ok(format!("Error: {e}")),
            Ok(Ok(Ok(output))) => output,
            Ok(Ok(Err(e))) => return Ok(format!("Error: {e}")),
        };

        // 应用 offset：跳过前 N 行
        let final_output = if let Some(offset) = grep_input.offset {
            if offset > 0 {
                let lines: Vec<&str> = output.split('\n').collect();
                let skipped: Vec<&str> = lines.into_iter().skip(offset).collect();
                skipped.join("\n")
            } else {
                output
            }
        } else {
            output
        };

        Ok(final_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grep_hit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("test.txt"),
            "needle in a haystack\nother line",
        )
        .unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "needle", "output_mode": "content", "path": "./"}))
            .await
            .unwrap();
        assert!(result.contains("needle"), "should find needle: {result}");
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "haystack only").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "zzz_not_here", "output_mode": "content", "path": "./"}))
            .await
            .unwrap();
        assert!(
            result.contains("No matches found"),
            "should report no match: {result}"
        );
    }

    #[tokio::test]
    async fn test_grep_missing_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"output_mode": "content"})).await.unwrap();
        assert!(
            result.contains("Missing required parameter 'pattern'"),
            "should report missing pattern: {result}"
        );
    }

    #[tokio::test]
    async fn test_grep_regex() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle123\nneedle456").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "needle[0-9]+", "output_mode": "content", "path": "./"}))
            .await
            .unwrap();
        assert!(result.contains("needle"), "regex should match: {result}");
    }

    #[test]
    fn test_grep_description_extended() {
        let tool = GrepTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("regex"), "description 应提及正则支持");
        assert!(
            desc.contains("Output modes:"),
            "description 应包含 Output modes 段落"
        );
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    #[tokio::test]
    async fn test_grep_files_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "needle here\nother line").unwrap();
        std::fs::write(dir.path().join("b.txt"), "no match here").unwrap();
        std::fs::write(dir.path().join("c.txt"), "needle again").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "needle", "output_mode": "files_with_matches", "path": "./"}))
            .await
            .unwrap();
        assert!(result.contains("a.txt"), "should find a.txt: {result}");
        assert!(result.contains("c.txt"), "should find c.txt: {result}");
        assert!(!result.contains("needle here"), "should not include line content: {result}");
    }

    #[tokio::test]
    async fn test_grep_count() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "needle\nneedle\nneedle").unwrap();
        std::fs::write(dir.path().join("b.txt"), "needle once").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "needle", "output_mode": "count", "path": "./"}))
            .await
            .unwrap();
        assert!(result.contains("a.txt:3"), "a.txt should have 3 matches: {result}");
        assert!(result.contains("b.txt:1"), "b.txt should have 1 match: {result}");
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "NEEDLE\nneedle\nNeedle").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "NEEDLE", "output_mode": "content", "-i": true, "path": "./"}))
            .await
            .unwrap();
        assert!(result.contains("NEEDLE"), "should match uppercase: {result}");
        assert!(result.contains("needle"), "should match lowercase: {result}");
        assert!(result.contains("Needle"), "should match mixed case: {result}");
    }

    #[tokio::test]
    async fn test_grep_glob_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle in txt").unwrap();
        std::fs::write(dir.path().join("test.rs"), "needle in rs").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "needle", "output_mode": "content", "glob": "*.txt", "path": "./"}))
            .await
            .unwrap();
        assert!(result.contains("test.txt"), "should find in .txt: {result}");
        assert!(!result.contains("test.rs"), "should not find in .rs: {result}");
    }

    #[tokio::test]
    async fn test_grep_type_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle in txt").unwrap();
        std::fs::write(dir.path().join("test.rs"), "needle in rs").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({
                "pattern": "needle",
                "output_mode": "content",
                "type": "rust",
                "path": "./"
            }))
            .await
            .unwrap();
        assert!(result.contains("test.rs"), "should find in .rs: {result}");
        assert!(!result.contains("test.txt"), "should not find in .txt with type=rust: {result}");
    }

    #[test]
    fn test_grep_tool_name() {
        let tool = GrepTool::new("/tmp");
        assert_eq!(tool.name(), "Grep");
    }

    #[tokio::test]
    async fn test_grep_invalid_output_mode() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({
                "pattern": "needle",
                "output_mode": "invalid_mode"
            }))
            .await
            .unwrap();
        assert!(result.contains("Error"), "should report invalid output_mode: {result}");
    }

    #[tokio::test]
    async fn test_grep_offset() {
        let dir = tempfile::tempdir().unwrap();
        let lines: Vec<String> = (0..10).map(|i| format!("line {} needle", i)).collect();
        std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({
                "pattern": "needle",
                "output_mode": "content",
                "path": "./",
                "offset": 5
            }))
            .await
            .unwrap();
        assert!(!result.contains("line 0"), "should skip first 5 lines: {result}");
        assert!(result.contains("line 5"), "should include line 5+: {result}");
    }
}
