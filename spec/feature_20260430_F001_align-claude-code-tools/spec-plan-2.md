# Claude Code 工具接口对齐 执行计划（下）

**目标:** 完成中等到重大重构的工具对齐，以及 TUI 层全局更新

**技术栈:** Rust 2021 edition, async-trait, serde_json, grep crate, ignore crate, tokio, ratatui

**设计文档:** spec-design.md

## 改动总览

本次计划覆盖 Task 5（Bash 工具 timeout 单位迁移+新增参数）、Task 6（Grep 工具参数结构重写）、Task 7（Agent 工具参数结构重写）、Task 8（TUI 层和提示词全局更新）、Task 9（总验收）。

- Task 5、6、7 各自独立修改一个工具定义文件（Task 6 额外更新 `tools/mod.rs` re-export 和 `middleware/filesystem.rs` import），互不依赖，可并行执行
- Task 8 依赖 Task 5~7 的工具名和参数变更全部落地，统一更新 TUI 层和提示词
- Task 9 依赖所有前置 Task，执行全量构建和端到端验证
- 关键设计决策：Grep 工具内部将结构化参数转译为现有 `ParsedArgs`，复用 `execute_search()` 搜索引擎；Agent 工具的 `isolation`/`run_in_background` 预留字段仅解析不执行

---

### Task 0: 环境准备

**背景:**
本文件依赖 plan-1 的 Task 0 已完成全量环境验证。此处仅做快速构建检查，确保 `rust-agent-middlewares` crate 处于可编译状态。

**执行步骤:**
- [x] 快速构建检查（参考 plan-1 Task 0 的全量验证）
  - `cargo build -p rust-agent-middlewares`
  - 预期: 构建成功，无编译错误

**检查步骤:**
- [x] 中间件 crate 构建成功
  - `cargo build -p rust-agent-middlewares`
  - 预期: 构建成功，无错误

---

### Task 5: Bash timeout 单位迁移 + 新增参数

**背景:**
将 `bash` 工具重命名为 `Bash`，超时参数从秒（`timeout_secs`，1-300）迁移为毫秒（`timeout`，1-600000），对齐 Claude Code 的 Bash 工具接口。同时新增 `description`（命令用途简述）和 `run_in_background`（后台运行预留）两个可选参数。当前 `BashTool` 使用 `Duration::from_secs(timeout_secs)` 计算超时，需改为 `Duration::from_millis(timeout_ms)`。Task 8 的 TUI 层工具名更新和 HITL 工具名匹配依赖本 Task 的工具名变更。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/middleware/terminal.rs`

**执行步骤:**

- [x] 修改 BashTool 工具名 — 将 `fn name()` 返回值从 `"bash"` 改为 `"Bash"`
  - 位置: `terminal.rs` → `impl BaseTool for BashTool` 的 `fn name()` (~L107)
  - 将 `"bash"` 改为 `"Bash"`
  - 原因: 对齐 Claude Code 工具命名

- [x] 重写参数 schema — 将 `timeout_secs` 替换为 `timeout`，新增 `description` 和 `run_in_background`
  - 位置: `terminal.rs` → `impl BaseTool for BashTool` 的 `fn parameters()` (~L115-L130)
  - 删除 `timeout_secs` 属性定义
  - 新增三个属性定义：
    ```
    "timeout": {
        "type": "number",
        "description": "Optional timeout in milliseconds (default 120000, max 600000). If the command takes longer than this, it will be killed and a timeout error returned"
    },
    "description": {
        "type": "string",
        "description": "A clear, concise description of what this command does in active voice. Never use words like 'complex' or 'risk' in the description — just describe what it does"
    },
    "run_in_background": {
        "type": "boolean",
        "description": "Set to true to run this command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later"
    }
    ```
  - 原因: 对齐 Claude Code 的 Bash 参数结构

- [x] 更新 `invoke()` 方法 — 修改超时解析逻辑，解析新增参数
  - 位置: `terminal.rs` → `impl BaseTool for BashTool` 的 `async fn invoke()` (~L132-L198)
  - 将 `let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(120).clamp(1, 300);` 改为:
    ```rust
    let timeout_ms = input["timeout"].as_u64().unwrap_or(120_000).clamp(1, 600_000);
    let _description = input["description"].as_str();
    let _run_in_background = input["run_in_background"].as_bool().unwrap_or(false);
    ```
  - 将 `Duration::from_secs(timeout_secs)` 改为 `Duration::from_millis(timeout_ms)`
  - 将超时错误消息中的 `{timeout_secs} seconds` 改为 `{timeout_ms/1000.0} seconds`（保持用户友好的秒数显示，用 `timeout_ms as f64 / 1000.0`）
  - 原因: timeout 单位从秒迁移到毫秒，`_description` 和 `_run_in_background` 解析但不使用

- [x] 更新 BASH_DESCRIPTION 常量 — 更新描述文本中的工具引用名和超时说明
  - 位置: `terminal.rs` → `BASH_DESCRIPTION` 常量 (~L12-L36)
  - 将 `glob_files` 改为 `Glob`（1 处，~L18）
  - 将 `search_files_rg` 改为 `Grep`（1 处，~L19）
  - 将 `read_file` 改为 `Read`（1 处，~L20）
  - 将 `edit_file` 改为 `Edit`（1 处，~L21）
  - 将 `write_file` 改为 `Write`（1 处，~L22）
  - 将 `You can specify an optional timeout in seconds (up to 300 seconds / 5 minutes). Default is 120 seconds (2 minutes)` 改为 `You can specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). Default is 120000ms (2 minutes)`
  - 原因: 描述文本引用的工具名需全部对齐新名称，超时说明需对齐毫秒单位

- [x] 更新 `TerminalMiddleware::tool_names()` — 将工具名从 `"bash"` 改为 `"Bash"`
  - 位置: `terminal.rs` → `impl TerminalMiddleware` 的 `fn tool_names()` (~L213-L215)
  - 将 `vec!["bash"]` 改为 `vec!["Bash"]`
  - 原因: 工具名列表需与 `fn name()` 一致

- [x] 更新所有测试用例 — 将 `timeout_secs` 替换为 `timeout` 并使用毫秒值
  - 位置: `terminal.rs` → `mod tests` 块中的所有涉及 `timeout_secs` 的测试 (~L262-L401)
  - `test_bash_timeout_returns_quickly` (~L263): 将 `timeout_secs` 变量改名为 `timeout_ms`，值从 `1` 改为 `1000`；JSON 中 `"timeout_secs": timeout_secs` 改为 `"timeout": timeout_ms`；注释中 "timeout_secs = 1 秒" 改为 "timeout = 1000 毫秒"；`elapsed.as_secs() < 3` 保持不变
  - `test_bash_timeout_clamped_to_minimum` (~L367): JSON 中 `"timeout_secs": 0` 改为 `"timeout": 0`；注释中 "timeout_secs = 0 → clamp 到 1 秒" 改为 "timeout = 0 → clamp 到 1 毫秒"；`elapsed.as_millis() < 500` 保持不变
  - `test_bash_timeout_maximum_accepted` (~L390): JSON 中 `"timeout_secs": 300` 改为 `"timeout": 600000`；注释中 "超时 300 秒" 改为 "超时 600000 毫秒"
  - 原因: 测试参数需与新接口一致

- [x] 为 Bash 工具新名称和新参数编写单元测试
  - 测试文件: `terminal.rs` → `mod tests`
  - 新增测试函数 `test_tool_name_is_Bash()`:
    ```rust
    #[test]
    fn test_tool_name_is_Bash() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        assert_eq!(tool.name(), "Bash");
    }
    ```
  - 新增测试函数 `test_bash_default_timeout_is_120_seconds()`:
    ```rust
    #[tokio::test]
    async fn test_bash_default_timeout_is_120_seconds() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        // 不传 timeout → 默认 120000ms = 120s
        let result = tool.invoke(serde_json::json!({"command": "echo ok"})).await.unwrap();
        assert!(result.contains("ok"));
    }
    ```
  - 新增测试函数 `test_bash_description_and_run_in_background_parsed()`:
    ```rust
    #[tokio::test]
    async fn test_bash_description_and_run_in_background_parsed() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        // description 和 run_in_background 不影响执行
        let result = tool.invoke(serde_json::json!({
            "command": "echo ok",
            "description": "test description",
            "run_in_background": true
        })).await.unwrap();
        assert!(result.contains("ok"));
    }
    ```
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- middleware::terminal::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 BashTool 工具名为 `"Bash"`
  - `cargo test -p rust-agent-middlewares --lib -- test_tool_name_is_Bash`
  - 预期: 测试通过
- [x] 验证描述文本中无旧工具名和旧参数名残留
  - `grep -n 'read_file\|edit_file\|write_file\|glob_files\|search_files_rg\|timeout_secs' rust-agent-middlewares/src/middleware/terminal.rs`
  - 预期: 无匹配输出（旧名称已全部清除）
- [x] 验证 `tool_names()` 返回 `"Bash"`
  - `grep -n 'tool_names' rust-agent-middlewares/src/middleware/terminal.rs`
  - 预期: 函数体中包含 `"Bash"` 而非 `"bash"`
- [x] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 所有测试通过，无编译错误

---

### Task 6: Grep 重大重构

**背景:**
将 `search_files_rg` 工具重命名为 `Grep`，并将 ripgrep 原始参数数组（`args: string[]`）替换为结构化字段（`pattern`/`path`/`glob`/`type`/`output_mode`/`-i`/`-C`/`-n`/`multiline`/`head_limit`/`offset`）。当前 `SearchFilesRgTool.invoke()` 接收 `args` 数组，通过 `parse_args()` 解析为 `ParsedArgs`，再调用 `execute_search()` 执行搜索。重构后 `GrepTool.invoke()` 将结构化 JSON 字段转译为 `ParsedArgs`（或等价结构），复用现有 `execute_search()` 搜索引擎。Task 8 的 TUI 层工具名和提示词更新依赖本 Task。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/filesystem/grep.rs`
- 修改: `rust-agent-middlewares/src/middleware/filesystem.rs`（更新 `tool_names()`、import、`build_tools()` 中 `SearchFilesRgTool` → `GrepTool`）
- 修改: `rust-agent-middlewares/src/tools/mod.rs`（更新 re-export `SearchFilesRgTool` → `GrepTool`）

**执行步骤:**

- [x] 重命名结构体 — 将 `SearchFilesRgTool` 改为 `GrepTool`
  - 位置: `grep.rs` ~L14
  - 将 `pub struct SearchFilesRgTool` 改为 `pub struct GrepTool`
  - 将 `impl SearchFilesRgTool` 改为 `impl GrepTool`
  - 更新注释从 `search_files_rg tool` 改为 `Grep tool`
  - 原因: 结构体名对齐新工具名

- [x] 新增 `GrepInput` 结构体 — 替代 `parse_args()` 函数，用于反序列化结构化参数
  - 位置: `grep.rs` → 在 `ParsedArgs` 定义之后（~L56），`OutputMode` 枚举之后（~L62）
  - 新增结构体：
    ```rust
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
    ```
  - 原因: 定义新的结构化参数容器

- [x] 新增 `type_to_glob()` 辅助函数 — 将 `type` 字段值映射为 glob 后缀
  - 位置: `grep.rs` → 在 `GrepInput` 结构体之后
  - 新增函数：
    ```rust
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
    ```
  - 原因: `type` 参数需映射为 glob 模式用于文件过滤

- [x] 新增 `GrepInput::to_parsed_args()` 方法 — 将结构化参数转译为 `ParsedArgs`
  - 位置: `grep.rs` → `impl GrepInput` 块
  - 新增方法：
    ```rust
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
    ```
  - 原因: 结构化参数需转译为内部 `ParsedArgs` 以复用 `execute_search()`

- [x] 重写 `BaseTool` 实现 — 更新 `name()`、`description()`、`parameters()`、`invoke()`
  - 位置: `grep.rs` → `impl BaseTool for SearchFilesRgTool` (~L416-L486)，改为 `impl BaseTool for GrepTool`
  - `fn name()` (~L417): 将 `"search_files_rg"` 改为 `"Grep"`
  - `fn description()` (~L421): 替换 `SEARCH_FILES_RG_DESCRIPTION` 常量引用为新的 `GREP_DESCRIPTION` 常量
  - `fn parameters()` (~L425): 替换整个 schema 为：
    ```rust
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
    ```
  - `async fn invoke()` (~L443): 完全重写为解析结构化参数：
    ```rust
    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
    ```
  - 原因: 参数结构从 args 数组完全重写为结构化字段

- [x] 替换 `SEARCH_FILES_RG_DESCRIPTION` 常量为 `GREP_DESCRIPTION`
  - 位置: `grep.rs` → `SEARCH_FILES_RG_DESCRIPTION` 常量 (~L24-L42)
  - 删除 `SEARCH_FILES_RG_DESCRIPTION` 常量
  - 新增 `GREP_DESCRIPTION` 常量：
    ```rust
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
    ```
  - 原因: 描述文本需引用新的参数名和工具名

- [x] 更新 `FilesystemMiddleware::tool_names()` — 将 `"search_files_rg"` 改为 `"Grep"`
  - 位置: `rust-agent-middlewares/src/middleware/filesystem.rs` → `fn tool_names()` (~L30-L38)
  - 将列表中的 `"search_files_rg"` 改为 `"Grep"`
  - 原因: 工具名列表需与 `fn name()` 一致

- [x] 更新 `filesystem.rs` 的 import 和 `build_tools()` — 将 `SearchFilesRgTool` 改为 `GrepTool`
  - 位置: `rust-agent-middlewares/src/middleware/filesystem.rs`
  - 将 `use crate::tools::{..., SearchFilesRgTool, ...}` 中的 `SearchFilesRgTool` 改为 `GrepTool` (~L7)
  - 将 `build_tools()` 中 `Box::new(SearchFilesRgTool::new(cwd))` 改为 `Box::new(GrepTool::new(cwd))` (~L25)
  - 原因: 结构体重命名后，所有引用需同步更新

- [x] 更新 `tools/mod.rs` re-exports — 将 `SearchFilesRgTool` 改为 `GrepTool`
  - 位置: `rust-agent-middlewares/src/tools/mod.rs` (~L6-L8)
  - 将 `pub use filesystem::{..., SearchFilesRgTool, ...}` 中的 `SearchFilesRgTool` 改为 `GrepTool`
  - 原因: mod.rs 是公开 API 入口，结构体重命名后 re-export 名称需同步

- [x] 保留 `parse_args()` 和 `ParsedArgs` 不变 — 内部搜索引擎依赖它们
  - 位置: `grep.rs` ~L44-L179
  - `parse_args()` 函数和 `ParsedArgs` 结构体保留，作为 `GrepInput::to_parsed_args()` 的转译目标
  - `OutputMode` 枚举、`SearchSink`、`execute_search()` 全部保留不变
  - 原因: 搜索引擎代码无需修改，仅修改入口层

- [x] 重写所有测试用例 — 从 `{"args": [...]}` 改为 `{"pattern": "...", "output_mode": "..."}` 格式
  - 位置: `grep.rs` → `mod tests` 块 (~L488-L615)
  - 所有 `SearchFilesRgTool` 引用改为 `GrepTool`
  - 每个测试的 `invoke()` 输入从 args 数组格式改为结构化字段格式：
    - `test_search_files_rg_hit` → 重命名为 `test_grep_hit`: `{"args": ["-n", "needle", "./"]}` 改为 `{"pattern": "needle", "output_mode": "content", "path": "./"}`
    - `test_search_files_rg_no_match` → 重命名为 `test_grep_no_match`: `{"args": ["-n", "zzz_not_here", "./"]}` 改为 `{"pattern": "zzz_not_here", "output_mode": "content", "path": "./"}`
    - `test_search_files_rg_empty_args` → 重命名为 `test_grep_missing_pattern`: `{"args": []}` 改为 `{"output_mode": "content"}`（不传 pattern，验证返回错误提示）
    - `test_search_files_rg_regex` → 重命名为 `test_grep_regex`: `{"args": ["-n", "needle[0-9]+", "./"]}` 改为 `{"pattern": "needle[0-9]+", "output_mode": "content", "path": "./"}`
    - `test_description_extended` → 重命名为 `test_grep_description_extended`: `SearchFilesRgTool::new` 改为 `GrepTool::new`，断言中检查 `"regex"` 和 `"Output modes:"` 保持不变
    - `test_search_files_rg_files_only` → 重命名为 `test_grep_files_only`: `{"args": ["-l", "needle", "./"]}` 改为 `{"pattern": "needle", "output_mode": "files_with_matches", "path": "./"}`
    - `test_search_files_rg_count` → 重命名为 `test_grep_count`: `{"args": ["-c", "needle", "./"]}` 改为 `{"pattern": "needle", "output_mode": "count", "path": "./"}`
    - `test_search_files_rg_case_insensitive` → 重命名为 `test_grep_case_insensitive`: `{"args": ["-i", "NEEDLE", "./"]}` 改为 `{"pattern": "NEEDLE", "output_mode": "content", "-i": true, "path": "./"}`
    - `test_search_files_rg_glob_filter` → 重命名为 `test_grep_glob_filter`: `{"args": ["-n", "-g", "*.txt", "needle", "./"]}` 改为 `{"pattern": "needle", "output_mode": "content", "glob": "*.txt", "path": "./"}`
  - 原因: 测试需覆盖新的结构化参数格式

- [x] 为 Grep 工具新增额外测试
  - 测试文件: `grep.rs` → `mod tests`
  - 新增测试函数 `test_grep_type_filter()`:
    ```rust
    #[tokio::test]
    async fn test_grep_type_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "needle in txt").unwrap();
        std::fs::write(dir.path().join("test.rs"), "needle in rs").unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "type": "rust",
            "path": "./"
        })).await.unwrap();
        assert!(result.contains("test.rs"), "should find in .rs: {result}");
        assert!(!result.contains("test.txt"), "should not find in .txt with type=rust: {result}");
    }
    ```
  - 新增测试函数 `test_grep_tool_name()`:
    ```rust
    #[test]
    fn test_grep_tool_name() {
        let tool = GrepTool::new("/tmp");
        assert_eq!(tool.name(), "Grep");
    }
    ```
  - 新增测试函数 `test_grep_invalid_output_mode()`:
    ```rust
    #[tokio::test]
    async fn test_grep_invalid_output_mode() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "invalid_mode"
        })).await.unwrap();
        assert!(result.contains("Error"), "should report invalid output_mode: {result}");
    }
    ```
  - 新增测试函数 `test_grep_offset()`:
    ```rust
    #[tokio::test]
    async fn test_grep_offset() {
        let dir = tempfile::tempdir().unwrap();
        let lines: Vec<String> = (0..10).map(|i| format!("line {} needle", i)).collect();
        std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
        let tool = GrepTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "offset": 5
        })).await.unwrap();
        assert!(!result.contains("line 0"), "should skip first 5 lines: {result}");
        assert!(result.contains("line 5"), "should include line 5+: {result}");
    }
    ```
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 GrepTool 工具名为 `"Grep"`
  - `cargo test -p rust-agent-middlewares --lib -- test_grep_tool_name`
  - 预期: 测试通过
- [x] 验证旧工具名和旧参数格式在 grep.rs 中无残留
  - `grep -n 'search_files_rg\|"args"\|SearchFilesRg' rust-agent-middlewares/src/tools/filesystem/grep.rs`
  - 预期: 仅在注释或已被删除的旧代码中可能残留，活跃代码中无匹配
- [x] 验证 `FilesystemMiddleware::tool_names()` 中包含 `"Grep"`
  - `grep -n 'tool_names' rust-agent-middlewares/src/middleware/filesystem.rs`
  - 预期: 函数体中包含 `"Grep"` 而非 `"search_files_rg"`
- [x] 验证 `filesystem.rs` 中无 `SearchFilesRgTool` 残留
  - `grep -n 'SearchFilesRgTool' rust-agent-middlewares/src/middleware/filesystem.rs`
  - 预期: 无匹配输出（已全部替换为 `GrepTool`）
- [x] 验证 `tools/mod.rs` 中无 `SearchFilesRgTool` 残留
  - `grep -n 'SearchFilesRgTool' rust-agent-middlewares/src/tools/mod.rs`
  - 预期: 无匹配输出（已全部替换为 `GrepTool`）
- [x] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 所有测试通过，无编译错误

---

### Task 7: Agent 重大重构

**背景:**
将 `launch_agent` 工具重命名为 `Agent`，将参数结构从 `agent_id`+`task`+`cwd` 重写为 `prompt`+`description`+`subagent_type`+`name`+`isolation`+`run_in_background`+`cwd`，对齐 Claude Code 的 Agent 工具接口。当前 `SubAgentTool.invoke()` 从 `input["agent_id"]` 和 `input["task"]` 读取参数，需改为从 `input["prompt"]` 和 `input["subagent_type"]` 读取。`isolation` 和 `run_in_background` 为预留字段，解析但不影响执行。Task 8 的 TUI 层工具名和 HITL 工具名匹配依赖本 Task。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`
- 修改: `rust-agent-middlewares/src/subagent/mod.rs`（更新 `build_agents_summary()` 中的工具名引用）

**执行步骤:**

- [x] 修改 SubAgentTool 工具名 — 将 `fn name()` 返回值从 `"launch_agent"` 改为 `"Agent"`
  - 位置: `tool.rs` → `impl BaseTool for SubAgentTool` 的 `fn name()` (~L162)
  - 将 `"launch_agent"` 改为 `"Agent"`
  - 原因: 对齐 Claude Code 工具命名

- [x] 重写参数 schema — 替换 `agent_id`+`task` 为新参数结构
  - 位置: `tool.rs` → `impl BaseTool for SubAgentTool` 的 `fn parameters()` (~L170-L189)
  - 将 `required` 从 `["agent_id", "task"]` 改为 `["prompt"]`
  - 替换整个 `properties` 对象为：
    ```rust
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task description to delegate to the sub-agent. Must be clear and self-contained, as the sub-agent has no access to the parent conversation history. Include all necessary context"
                },
                "description": {
                    "type": "string",
                    "description": "A short description of the task (3-5 words), used for UI display and logging"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The agent type/ID matching an existing agent definition file at .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md. When empty or not provided, creates a fork of the current agent with all tools"
                },
                "name": {
                    "type": "string",
                    "description": "A short alias for the sub-agent, used for UI identification"
                },
                "isolation": {
                    "type": "string",
                    "description": "Isolation mode for the sub-agent. Use 'worktree' to create an isolated git worktree. Currently reserved for future use"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run the sub-agent in the background. Currently reserved for future use"
                },
                "cwd": {
                    "type": "string",
                    "description": "The working directory for the sub-agent. Defaults to inheriting the parent agent's current working directory if not specified"
                }
            }
        })
    }
    ```
  - 原因: 对齐 Claude Code 的 Agent 参数结构

- [x] 重写 `invoke()` 方法 — 从新参数名读取输入
  - 位置: `tool.rs` → `impl BaseTool for SubAgentTool` 的 `async fn invoke()` (~L191-L322)
  - 替换参数解析逻辑（~L195-L208）：
    ```rust
    let prompt = match input.get("prompt").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return Ok("错误：缺少必需参数 prompt".to_string()),
    };
    let subagent_type = input.get("subagent_type").and_then(|v| v.as_str()).map(|s| s.to_string());
    let _description = input.get("description").and_then(|v| v.as_str());
    let _name = input.get("name").and_then(|v| v.as_str());
    let _isolation = input.get("isolation").and_then(|v| v.as_str());
    let _run_in_background = input.get("run_in_background").and_then(|v| v.as_bool()).unwrap_or(false);
    let cwd = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(&self.parent_cwd)
        .to_string();
    ```
  - 将 `agent_id` 的后续使用全部改为 `subagent_type`：
    - agent 定义文件查找：`AgentDefineMiddleware::candidate_paths(&cwd, &agent_id)` → 使用 `subagent_type`（~L211）
    - agent_id 为 None 时的处理（~L215-L223）：当 `subagent_type` 为 `None` 时，返回 "fork yourself" 模式的提示信息（暂不实现完整 fork 逻辑，返回错误提示）。将 `None =>` 分支改为：
      ```rust
      None => {
          return Ok("错误：请提供 subagent_type 参数指定要使用的 agent 类型".to_string());
      }
      ```
    - 错误消息中的 `agent_id` 引用改为 `subagent_type`（~L218-L222）
    - `agent_path` 的 `find()` 和匹配逻辑保持不变，只需将变量名 `agent_id` 改为 `subagent_type.unwrap_or_default()` 或在 `Some(id)` 解构时直接用
  - 将 `task` 变量改为 `prompt`：`AgentInput::text(task)` → `AgentInput::text(prompt)`（~L309）
  - 原因: 参数名从 agent_id/task 改为 subagent_type/prompt

- [x] 更新 `filter_tools()` 方法中的递归排除名 — 将 `"launch_agent"` 改为 `"Agent"`
  - 位置: `tool.rs` → `impl SubAgentTool` 的 `fn filter_tools()` (~L123-L157)
  - 将 `if name == "launch_agent"` 改为 `if name == "Agent"`（~L137）
  - 原因: 防递归排除需匹配新工具名

- [x] 替换 `LAUNCH_AGENT_DESCRIPTION` 常量为 `AGENT_DESCRIPTION`
  - 位置: `tool.rs` → `LAUNCH_AGENT_DESCRIPTION` 常量 (~L25-L42)
  - 删除 `LAUNCH_AGENT_DESCRIPTION` 常量
  - 新增 `AGENT_DESCRIPTION` 常量：
    ```rust
    const AGENT_DESCRIPTION: &str = r#"Launch a sub-agent with an independent context to handle a specialized sub-task. The sub-agent executes based on the configuration defined in .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md.

    Usage:
    - Provide a clear, self-contained task description via the prompt parameter. The sub-agent has no access to the parent conversation history
    - Specify subagent_type matching an existing agent definition file. When not provided, creates a fork of the current agent
    - The sub-agent inherits the parent's tool set by default, excluding Agent itself (to prevent recursion)
    - Agent definitions may restrict available tools via the tools and disallowedTools fields in frontmatter
    - The sub-agent executes in isolated state — it cannot access the parent's message history or intermediate results

    When to use:
    - For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
    - For tasks requiring specialized persona or behavior defined in agent configuration files
    - For parallelizable sub-tasks that do not depend on each other's results
    - When you need to break a complex task into smaller, independently executable pieces

    Return format:
    - If the sub-agent made tool calls, the result includes a summary of tools used followed by the final response
    - If no tool calls were made, only the final response text is returned"#;
    ```
  - 更新 `fn description()` 引用：`LAUNCH_AGENT_DESCRIPTION` → `AGENT_DESCRIPTION`
  - 原因: 描述文本需引用新的参数名

- [x] 更新 `build_agents_summary()` 中的工具名引用 — 将 `launch_agent` 改为 `Agent`
  - 位置: `mod.rs` → `fn build_agents_summary()` (~L177-L193)
  - 将 `` "你可以使用 `launch_agent` 工具委派子任务给以下专门 Agent：" `` 改为 `` "你可以使用 `Agent` 工具委派子任务给以下专门 Agent：" ``（~L179）
  - 将 `` "调用时传入 `agent_id` 字段（括号内的标识符）和 `task` 字段（任务描述）。" `` 改为 `` "调用时传入 `subagent_type` 字段（括号内的标识符）和 `prompt` 字段（任务描述）。" ``（~L188-L189）
  - 原因: 提示文本中引用的工具名和参数名需对齐

- [x] 更新 `mod.rs` 中的注释 — 将 `launch_agent` 引用改为 `Agent`
  - 位置: `mod.rs` → `SubAgentMiddleware` 结构体的文档注释 (~L23-L26)
  - 将注释中 `` 向父 agent 注入 `launch_agent` 工具 `` 改为 `` 向父 agent 注入 `Agent` 工具 ``
  - 将 `` 使 LLM 可调用 `launch_agent` 工具 `` 改为 `` 使 LLM 可调用 `Agent` 工具 ``
  - 原因: 注释需反映新工具名

- [x] 重写所有测试用例 — 更新工具名和参数格式
  - 位置: `tool.rs` → `mod tests` 块 (~L349-L858)
  - `test_tool_name` (~L406): `assert_eq!(t.name(), "launch_agent")` 改为 `assert_eq!(t.name(), "Agent")`
  - `test_tool_parameters_has_required_fields` (~L411): `names.contains(&"agent_id")` 和 `names.contains(&"task")` 改为 `names.contains(&"prompt")`，断言 required 中不再包含 `"agent_id"` 和 `"task"`
  - `test_tool_agent_not_found` (~L421): `{"agent_id": "nonexistent-agent", "task": "do something", "cwd": "/tmp"}` 改为 `{"subagent_type": "nonexistent-agent", "prompt": "do something", "cwd": "/tmp"}`
  - `test_tool_filter_inherit_all` (~L435): `make_tool("launch_agent")` 改为 `make_tool("Agent")`；断言 `!names.contains(&"launch_agent")` 改为 `!names.contains(&"Agent")`；注释 `"launch_agent 不应被继承"` 改为 `"Agent 不应被继承"`
  - `test_tool_filter_allowlist` (~L455): 保持不变（使用 `read_file`/`write_file`/`glob_files`，与过滤逻辑无关）
  - `test_tool_filter_disallow` (~L478): 保持不变
  - `test_tool_executes_with_valid_agent_file` (~L504): `{"agent_id": "test-agent", "task": "hello", ...}` 改为 `{"subagent_type": "test-agent", "prompt": "hello", ...}`
  - `test_launch_agent_tool_in_list` (~L528): 重命名为 `test_agent_tool_in_list`；`assert_eq!(t.name(), "launch_agent")` 改为 `assert_eq!(t.name(), "Agent")`；`assert_eq!(def.name, "launch_agent")` 改为 `assert_eq!(def.name, "Agent")`
  - `test_launch_agent_excluded_even_when_explicitly_allowed` (~L538): 重命名为 `test_agent_excluded_even_when_explicitly_allowed`；`make_tool("launch_agent")` 改为 `make_tool("Agent")`（2 处）；`ToolsValue::List(vec!["launch_agent".to_string(), ...])` 改为 `ToolsValue::List(vec!["Agent".to_string(), ...])`；断言中 `"launch_agent"` 改为 `"Agent"`（3 处）；注释同步更新
  - `test_tool_filter_case_insensitive` (~L560): 保持不变（测试的是 `read_file`/`write_file`/`glob_files`）
  - `test_launch_agent_excluded_when_in_disallowed` (~L603): 重命名为 `test_agent_excluded_when_in_disallowed`；`make_tool("launch_agent")` 改为 `make_tool("Agent")`（2 处）；`ToolsValue::List(vec!["launch_agent".to_string()])` 改为 `ToolsValue::List(vec!["Agent".to_string()])`；断言 `"launch_agent"` 改为 `"Agent"`
  - `test_system_builder_injects_system_message` (~L618): `{"agent_id": "tone-test", "task": "hello", ...}` 改为 `{"subagent_type": "tone-test", "prompt": "hello", ...}`
  - `test_skill_preload_registered` (~L676): `{"agent_id": "skill-user", "task": "test task", ...}` 改为 `{"subagent_type": "skill-user", "prompt": "test task", ...}`
  - `test_launch_agent_description_extended` (~L746): 重命名为 `test_agent_description_extended`
  - `test_cancel_token_interrupts_subagent` (~L792): `{"agent_id": "forever", "task": "run", ...}` 改为 `{"subagent_type": "forever", "prompt": "run", ...}`
  - 原因: 所有测试需使用新工具名和新参数格式

- [x] 更新 `mod.rs` 中测试引用的工具名
  - 位置: `mod.rs` → `mod tests` 块 (~L270, ~L281, ~L353)
  - `test_middleware_collect_tools` (~L270): `assert_eq!(tools[0].name(), "launch_agent")` 改为 `assert_eq!(tools[0].name(), "Agent")`
  - `test_build_tool_returns_subagent_tool` (~L281): `assert_eq!(tool.name(), "launch_agent")` 改为 `assert_eq!(tool.name(), "Agent")`
  - `test_before_agent_injects_summary` (~L353): `assert!(content.contains("launch_agent"))` 改为 `assert!(content.contains("Agent"))`
  - 原因: 测试断言需匹配新工具名

- [x] 为 Agent 工具新参数编写单元测试
  - 测试文件: `tool.rs` → `mod tests`
  - 新增测试函数 `test_agent_parameters_required_is_prompt_only()`:
    ```rust
    #[test]
    fn test_agent_parameters_required_is_prompt_only() {
        let t = make_subagent_tool(vec![]);
        let params = t.parameters();
        let required = params["required"].as_array().unwrap();
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(names, vec!["prompt"]);
    }
    ```
  - 新增测试函数 `test_agent_prompt_missing_returns_error()`:
    ```rust
    #[tokio::test]
    async fn test_agent_prompt_missing_returns_error() {
        let t = make_subagent_tool(vec![]);
        let result = t.invoke(serde_json::json!({
            "subagent_type": "some-agent",
            "cwd": "/tmp"
        })).await.unwrap();
        assert!(result.contains("prompt"), "应返回缺少 prompt 的错误: {}", result);
    }
    ```
  - 新增测试函数 `test_agent_subagent_type_missing_returns_error()`:
    ```rust
    #[tokio::test]
    async fn test_agent_subagent_type_missing_returns_error() {
        let t = make_subagent_tool(vec![]);
        let result = t.invoke(serde_json::json!({
            "prompt": "do something"
        })).await.unwrap();
        assert!(result.contains("subagent_type") || result.contains("agent"), "应返回缺少 subagent_type 的错误: {}", result);
    }
    ```
  - 新增测试函数 `test_agent_reserved_fields_parsed()`:
    ```rust
    #[tokio::test]
    async fn test_agent_reserved_fields_parsed() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("test-agent.md"),
            "---\nname: test-agent\ndescription: A test agent\n---\n\nYou are a test agent.\n",
        ).unwrap();

        let t = make_subagent_tool(vec![]);
        let result = t.invoke(serde_json::json!({
            "prompt": "hello",
            "subagent_type": "test-agent",
            "description": "test desc",
            "name": "test-alias",
            "isolation": "worktree",
            "run_in_background": true,
            "cwd": dir.path().to_str().unwrap()
        })).await.unwrap();
        // 预留字段不影响执行，仍应返回正常结果
        assert!(result.contains("echo"), "应正常执行: {}", result);
    }
    ```
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- subagent::tool::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 SubAgentTool 工具名为 `"Agent"`
  - `cargo test -p rust-agent-middlewares --lib -- test_tool_name`
  - 预期: 断言 `assert_eq!(t.name(), "Agent")` 通过
- [x] 验证 `filter_tools()` 中排除 `"Agent"` 而非 `"launch_agent"`
  - `cargo test -p rust-agent-middlewares --lib -- test_agent_excluded_even_when_explicitly_allowed`
  - 预期: 测试通过
- [x] 验证旧工具名 `launch_agent` 和旧参数名 `agent_id`/`task` 在活跃代码中无残留
  - `grep -n 'launch_agent\|"agent_id"\|"task"' rust-agent-middlewares/src/subagent/tool.rs | grep -v '//' | grep -v 'test' | grep -v 'mod.rs'`
  - 预期: 活跃代码（非注释、非测试）中无匹配
- [x] 验证 `build_agents_summary()` 引用新工具名
  - `grep -n 'launch_agent\|agent_id' rust-agent-middlewares/src/subagent/mod.rs`
  - 预期: 无匹配输出（已全部替换为 `Agent` 和 `subagent_type`/`prompt`）
- [x] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 所有测试通过，无编译错误

---

### Task 8: TUI 层和提示词全局更新

**背景:**
Task 1-7 已将所有 9 个工具的名称和参数结构对齐 Claude Code（`read_file`→`Read`、`write_file`→`Write`、`edit_file`→`Edit`、`glob_files`→`Glob`、`search_files_rg`→`Grep`、`bash`→`Bash`、`todo_write`→`TodoWrite`、`ask_user_question`→`AskUserQuestion`、`launch_agent`→`Agent`）。TUI 层的 UI 显示、事件路由、工具参数格式化、HITL 审批匹配、提示词段落文件仍引用旧工具名，需全部更新为新名称。`message_pipeline.rs` 中 SubAgent 事件路由依赖 `input["agent_id"]`/`input["task"]` 字段，需改为 `input["subagent_type"]`/`input["prompt"]` 以适配 Agent 工具的新参数结构。`skill_preload.rs` 中 fake tool call 使用的 `"read_file"` 需改为 `"Read"`。本 Task 依赖 Task 1-7 全部完成。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/tool_display.rs`
- 修改: `rust-agent-tui/src/ui/message_view.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/app/message_pipeline.rs`
- 修改: `rust-agent-tui/src/app/events.rs`
- 修改: `rust-agent-tui/src/ui/headless.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/popups/hitl.rs`
- 修改: `rust-agent-tui/prompts/sections/05_using_tools.md`
- 修改: `rust-agent-tui/prompts/sections/07_communicating.md`
- 修改: `rust-agent-tui/prompts/sections/10_hitl.md`
- 修改: `rust-agent-tui/prompts/sections/11_subagent.md`
- 修改: `rust-agent-tui/prompts/sections/06_tone_style.md`
- 修改: `rust-agent-middlewares/src/hitl/mod.rs`
- 修改: `rust-agent-middlewares/src/hitl/auto_classifier.rs`
- 修改: `rust-agent-middlewares/src/subagent/skill_preload.rs`

**执行步骤:**

- [x] 更新 `tool_display.rs` — 工具名匹配分支全部替换为新名称
  - 位置: `rust-agent-tui/src/app/tool_display.rs`
  - `format_tool_name()` 函数（~L19-L33）：将 match 分支从旧名改为新名
    - `"bash"` → `"Bash"`（保持返回 `"Shell"`，显示名不变）
    - `"read_file"` → `"Read"`（保持返回 `"Read"`）
    - `"write_file"` → `"Write"`（保持返回 `"Write"`）
    - `"edit_file"` → `"Edit"`（保持返回 `"Edit"`）
    - `"glob_files"` → `"Glob"`（保持返回 `"Glob"`）
    - `"search_files_rg"` → `"Grep"`（返回值从 `"Search"` 改为 `"Grep"`，因为工具名已变，display name 应与新工具名一致）
    - `"todo_write"` → `"TodoWrite"`（保持返回 `"Todo"`）
    - `"ask_user_question"` → `"AskUserQuestion"`（保持返回 `"Ask"`）
    - `"launch_agent"` → `"Agent"`（保持返回 `"Agent"`）
  - `format_tool_args()` 函数（~L42-L65）：将 match 分支从旧名改为新名
    - `"bash"` → `"Bash"`
    - `"read_file" | "write_file" | "edit_file"` → `"Read" | "Write" | "Edit"`（字段 `file_path` 不变）
    - `"glob_files"` → `"Glob"`（字段 `pattern` 不变）
    - `"search_files_rg"` → `"Grep"`：将 `input["args"].as_array()` 改为 `input["pattern"].as_str().map(|s| truncate(s, 60))`（Grep 工具不再使用 args 数组，改为 pattern 字段）
  - 原因: 工具显示逻辑需匹配新工具名

- [x] 更新 `message_view.rs` — ToolCategory 枚举和工具颜色映射全部替换为新名称
  - 位置: `rust-agent-tui/src/ui/message_view.rs`
  - `ToolCategory::from_tool_name()`（~L19-L24）：
    - `"read_file"` → `"Read"`
    - `"search_files_rg"` → `"Grep"`
    - `"glob_files"` → `"Glob"`
  - `ToolCategory::summary_for_tools()`（~L57-L59）：
    - `t.tool_name == "search_files_rg"` → `t.tool_name == "Grep"`
    - `t.tool_name == "read_file"` → `t.tool_name == "Read"`
    - `t.tool_name == "glob_files"` → `t.tool_name == "Glob"`
  - `from_base_message_with_cwd()` 中 launch_agent 恢复逻辑（~L343-L361）：
    - `if tool_name == "launch_agent"` → `if tool_name == "Agent"`
    - `input["agent_id"]` → `input["subagent_type"]`（适配 Agent 工具新参数）
    - `input["task"]` → `input["prompt"]`（适配 Agent 工具新参数）
  - `tool_color()` 函数（~L534-L549）：
    - `"read_file" | "glob_files" | "search_files_rg"` → `"Read" | "Glob" | "Grep"`
    - `"write_file" | "edit_file" | ...` → `"Write" | "Edit" | ...`
    - `"bash"` → `"Bash"`
    - `"launch_agent" | "ask_user_question" | "todo_write"` → `"Agent" | "AskUserQuestion" | "TodoWrite"`
  - 注释中 `// read_file` → `// Read`、`// search_files_rg` → `// Grep`、`// glob_files` → `// Glob`（~L11-L13）
  - 原因: 分类匹配、颜色分配、SubAgent 恢复逻辑均依赖工具名

- [x] 更新 `agent.rs` — ExecutorEvent 映射中的工具名匹配
  - 位置: `rust-agent-tui/src/app/agent.rs` → `fn map_executor_event()`（~L280-L360）
  - 注释 `// launch_agent ToolStart` → `// Agent ToolStart`（~L284）
  - `if name == "launch_agent"` → `if name == "Agent"`（~L285）
  - `input["agent_id"]` → `input["subagent_type"]`（~L286，适配新参数名）
  - `input["task"]` → `input["prompt"]`（~L287，适配新参数名）
  - 注释 `// launch_agent ToolEnd` → `// Agent ToolEnd`（~L310）
  - `if name == "launch_agent"` → `if name == "Agent"`（~L316）
  - `if name == "ask_user_question"` → `if name == "AskUserQuestion"`（~L327）
  - 注释 `// 成功的 ToolEnd（非 launch_agent / ask_user_question / error）` → `// 成功的 ToolEnd（非 Agent / AskUserQuestion / error）`（~L351）
  - 原因: 事件路由需匹配新工具名和新参数字段

- [x] 更新 `message_pipeline.rs` — 流式事件处理中的工具名匹配和参数字段
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs`
  - `handle_event()` 中 `SubAgentStart` 分支（~L211-L213）：
    - `serde_json::json!({"agent_id": &agent_id, "task": &task_preview})` → `serde_json::json!({"subagent_type": &agent_id, "prompt": &task_preview})`
    - `self.tool_start(&tc_id, "launch_agent", input)` → `self.tool_start(&tc_id, "Agent", input)`
  - `SubAgentEnd` 分支（~L222）：
    - `self.tool_end(&tc_id, "launch_agent", &result, is_error)` → `self.tool_end(&tc_id, "Agent", &result, is_error)`
  - `tool_start()` 中 SubAgent 检测（~L279-L299）：
    - `if name == "launch_agent"` → `if name == "Agent"`
    - `input["agent_id"]` → `input["subagent_type"]`
    - `input["task"]` → `input["prompt"]`
  - `tool_end()` 中（~L337-L390）：
    - `if name == "launch_agent"` → `if name == "Agent"`（~L338）
    - `if name == "ask_user_question"` → `if name == "AskUserQuestion"`（~L356）
    - `format_tool_args("ask_user_question", ...)` → `format_tool_args("AskUserQuestion", ...)`（~L357）
    - `"ask_user_question".to_string()` → `"AskUserQuestion".to_string()`（~L359, ~L361, ~L366 共 3 处）
    - `format_tool_name("ask_user_question")` → `format_tool_name("AskUserQuestion")`（~L361）
    - `tool_color("ask_user_question")` → `tool_color("AskUserQuestion")`（~L366）
    - `if name == "todo_write"` → `if name == "TodoWrite"`（~L375）
    - `"todo_write".to_string()` → `"TodoWrite".to_string()`（~L377, ~L379, ~L384 共 3 处）
    - `format_tool_name("todo_write")` → `format_tool_name("TodoWrite")`（~L379）
    - `tool_color("todo_write")` → `tool_color("TodoWrite")`（~L384）
  - 原因: 流式管线需匹配新工具名和新参数字段以正确路由事件

- [x] 更新 `events.rs` — 注释中的工具名引用
  - 位置: `rust-agent-tui/src/app/events.rs`（~L45, ~L50）
  - `/// SubAgent 开始执行（由 launch_agent ToolStart 映射而来）` → `/// SubAgent 开始执行（由 Agent ToolStart 映射而来）`
  - `/// SubAgent 执行结束（由 launch_agent ToolEnd 映射而来）` → `/// SubAgent 执行结束（由 Agent ToolEnd 映射而来）`
  - 原因: 注释需反映新工具名

- [x] 更新 `headless.rs` — 测试中的工具名引用
  - 位置: `rust-agent-tui/src/ui/headless.rs`
  - `test_tool_call_renders`（~L109）：`name: "read_file".into()` → `name: "Read".into()`
  - `test_tool_call_renders`（~L124）：`.any(|l| l.contains("Read") || l.contains("read_file"))` → `.any(|l| l.contains("Read") || l.contains("Read"))`（去重后为 `.any(|l| l.contains("Read"))`）
  - `test_subagent_rendering`（~L498）：`name: "read_file".into()` → `name: "Read".into()`
  - `test_subagent_rendering`（~L505）：`name: "bash".into()` → `name: "Bash".into()`
  - `test_subagent_sliding_window`（~L562）：`name: "read_file".into()` → `name: "Read".into()`
  - `test_tool_call_message_visible_when_toggled`（~L637）：`name: "bash".into()` → `name: "Bash".into()`
  - `test_tool_call_message_visible_when_toggled`（~L659）：`.any(|l| l.contains("Shell") || l.contains("bash"))` → `.any(|l| l.contains("Shell") || l.contains("Bash"))`
  - `test_tool_call_without_assistant_chunk_no_bubble`（~L741）：`name: "bash".into()` → `name: "Bash".into()`
  - `test_tool_call_widget_renders_completed`（~L1689-L1691）：`tool_name: "bash".to_string()` → `tool_name: "Bash".to_string()`，`display_name: "bash".to_string()` → `display_name: "Shell".to_string()`
  - `test_tool_call_widget_renders_completed`（~L1710）：`handle.contains("bash")` → `handle.contains("Shell")`（display_name 为 Shell）
  - `test_tool_then_text_preserves_tool_block`（~L2023）：`name: "bash".into()` → `name: "Bash".into()`
  - 原因: 测试中使用的工具名需与实际工具名一致

- [x] 更新 `message_view.rs` 测试 — 工具名引用
  - 位置: `rust-agent-tui/src/ui/message_view.rs` → `mod tests`（~L552-L673）
  - `test_ai_message_with_only_tool_calls_renders_tool_use`（~L565-L592）：
    - `ToolCallRequest::new("toolu_001", "bash", ...)` → `ToolCallRequest::new("toolu_001", "Bash", ...)`
    - `ToolCallRequest::new("toolu_002", "read_file", ...)` → `ToolCallRequest::new("toolu_002", "Read", ...)`
    - `assert!(names.contains(&"bash"))` → `assert!(names.contains(&"Bash"))`
    - `assert!(names.contains(&"read_file"))` → `assert!(names.contains(&"Read"))`
  - `test_ai_message_with_text_and_tool_calls_renders_both`（~L605）：
    - `"bash"` → `"Bash"`
  - `test_no_duplicate_tool_use_from_tool_calls`（~L638）：
    - `"bash"` → `"Bash"`
  - 原因: 测试断言需匹配新工具名

- [x] 更新 `hitl.rs` 测试 — HITL 面板测试中的工具名
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/hitl.rs` → `mod tests`
  - `render_headless_hitl_single`（~L176）：`tool_name: "bash".to_string()` → `tool_name: "Bash".to_string()`
  - `render_headless_hitl_multi`（~L193）：`tool_name: "bash".to_string()` → `tool_name: "Bash".to_string()`
  - `render_headless_hitl_multi`（~L197）：`tool_name: "write_file".to_string()` → `tool_name: "Write".to_string()`
  - 原因: 测试数据需使用新工具名

- [x] 更新 `message_pipeline.rs` 测试 — 全部工具名引用
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs` → `mod tests`（~L690-L1082）
  - `test_tool_args_cwd_consistency`（~L710）：`"read_file"` → `"Read"`（ToolCallRequest 和 json! 中）
  - `test_pipeline_tool_end_no_duplicate`（~L789-L810）：`"read_file"` → `"Read"`（4 处：tool_start、tool_end、ToolCallRequest×2）
  - `test_handle_event_tool_lifecycle`（~L857-L880）：`name: "read_file".into()` → `name: "Read".into()`（2 处）
  - `test_subagent_parallel_same_tool_matches_by_call_id`（~L908-L930）：`name: "read_file".into()` → `name: "Read".into()`（4 处）
  - `test_reconcile_tail_with_tools`（~L1057）：`name: "read_file".to_string()` → `name: "Read".to_string()`
  - 原因: 测试需使用新工具名

- [x] 更新提示词段落文件 — 所有旧工具名引用替换为新名称
  - 位置: `rust-agent-tui/prompts/sections/05_using_tools.md`（~L7-L9）
    - `` `search_files_rg` `` → `` `Grep` ``
    - `` `glob_files` `` → `` `Glob` ``
    - `` `bash` commands like `grep` or `find` `` → `` `Bash` commands like `grep` or `find` ``
    - `` `read_file` instead of `bash` commands like `cat` `` → `` `Read` instead of `Bash` commands like `cat` ``
    - `` `write_file` or `edit_file` instead of `bash` commands like `echo` or `sed` `` → `` `Write` or `Edit` instead of `Bash` commands like `echo` or `sed` ``
  - 位置: `rust-agent-tui/prompts/sections/07_communicating.md`（~L4）
    - `` "I will use the read_file tool to..." `` → `` "I will use the Read tool to..." ``
  - 位置: `rust-agent-tui/prompts/sections/10_hitl.md`（~L5-L7）
    - `` - `bash` — shell command execution `` → `` - `Bash` — shell command execution ``
    - `` - `launch_agent` — sub-agent delegation `` → `` - `Agent` — sub-agent delegation ``
    - `` - `write_*` — any file write operation `` → `` - `Write` — file write operation ``
    - `` - `edit_*` — any file edit operation `` → `` - `Edit` — file edit operation ``
  - 位置: `rust-agent-tui/prompts/sections/11_subagent.md`（~L3, ~L14-L15）
    - `` `launch_agent` tool `` → `` `Agent` tool ``
    - `` `agent_id` matching `` → `` `subagent_type` matching ``
    - `` .claude/agents/{agent_id}.md `` → `` .claude/agents/{subagent_type}.md ``（2 处）
    - `` `task` description `` → `` `prompt` description ``
    - `` `agent_id` matching `` → `` `subagent_type` matching ``
    - `` excluding `launch_agent` itself `` → `` excluding `Agent` itself ``
    - `` Ensure the `task` parameter `` → `` Ensure the `prompt` parameter ``
  - 位置: `rust-agent-tui/prompts/sections/06_tone_style.md`（~L57）
    - `When you run a non-trivial bash command` → `When you run a non-trivial shell command`（此处 "bash" 是通用术语而非工具名，但保持一致性改为 "shell command"）
  - 原因: 提示词中引用的工具名和参数名必须与实际工具定义一致

- [x] 更新 `hitl/mod.rs` — 默认审批规则和编辑工具判断中的工具名
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`
  - `default_requires_approval()` 函数（~L40-L48）：
    - `tool_name == "bash"` → `tool_name == "Bash"`
    - `tool_name == "launch_agent"` → `tool_name == "Agent"`
    - `tool_name.starts_with("write_")` → `tool_name == "Write"`（工具名已从 `write_file` 改为 `Write`，不再有 `write_` 前缀）
    - `tool_name.starts_with("edit_")` → `tool_name == "Edit"`（同上）
    - `tool_name.starts_with("delete_")` 和 `tool_name.starts_with("rm_")` 保持不变（这些是通用前缀匹配，用于未来可能的删除工具）
  - `is_edit_tool()` 函数（~L54-L58）：
    - `tool_name.starts_with("write_")` → `tool_name == "Write"`
    - `tool_name.starts_with("edit_")` → `tool_name == "Edit"`
  - 注释（~L35-L39, ~L53）：
    - `` - `bash` `` → `` - `Bash` ``
    - `` - `write_*` `` → `` - `Write` ``
    - `` - `edit_*` `` → `` - `Edit` ``
    - `` - `launch_agent` `` → `` - `Agent` ``
    - `` `bash`、`launch_agent` `` → `` `Bash`、`Agent` ``
  - 原因: HITL 审批匹配逻辑需使用新工具名

- [x] 更新 `hitl/mod.rs` 测试 — 审批测试中的工具名
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs` → `mod tests`
  - `test_default_requires_approval`（~L456-L467）：
    - `default_requires_approval("bash")` → `default_requires_approval("Bash")`
    - `default_requires_approval("write_file")` → `default_requires_approval("Write")`
    - `default_requires_approval("edit_file")` → `default_requires_approval("Edit")`
    - `default_requires_approval("launch_agent")` → `default_requires_approval("Agent")`
    - `!default_requires_approval("read_file")` → `!default_requires_approval("Read")`
    - `!default_requires_approval("glob_files")` → `!default_requires_approval("Glob")`
    - `!default_requires_approval("search_files_rg")` → `!default_requires_approval("Grep")`
    - `!default_requires_approval("todo_write")` → `!default_requires_approval("TodoWrite")`
  - `test_is_edit_tool`（~L537-L544）：
    - `is_edit_tool("write_file")` → `is_edit_tool("Write")`
    - `is_edit_tool("edit_file")` → `is_edit_tool("Edit")`
    - `!is_edit_tool("bash")` → `!is_edit_tool("Bash")`
    - `!is_edit_tool("launch_agent")` → `!is_edit_tool("Agent")`
    - `!is_edit_tool("read_file")` → `!is_edit_tool("Read")`
  - 其余测试中 `make_tool_call("bash")` → `make_tool_call("Bash")`、`make_tool_call("write_file")` → `make_tool_call("Write")`、`make_tool_call("read_file")` → `make_tool_call("Read")`（所有 ~L419-L709 中的引用）
  - `test_accept_edits_allows_write_file` 重命名为 `test_accept_edits_allows_Write`
  - 断言中 `result.name == "bash"` → `result.name == "Bash"`、`result.name == "write_file"` → `result.name == "Write"`、`result.name == "read_file"` → `result.name == "Read"`
  - 原因: 测试断言需使用新工具名

- [x] 更新 `hitl/auto_classifier.rs` 测试 — 分类器测试中的工具名
  - 位置: `rust-agent-middlewares/src/hitl/auto_classifier.rs` → `mod tests`
  - 所有 `LlmAutoClassifier::cache_key("bash", ...)` → `LlmAutoClassifier::cache_key("Bash", ...)`（~L232, ~L233, ~L242, ~L243, ~L316, ~L330 共 6 处）
  - 所有 `classifier.classify("bash", ...)` → `classifier.classify("Bash", ...)`（~L254, ~L266, ~L278, ~L290, ~L302, ~L314, ~L327, ~L343, ~L356, ~L373, ~L385, ~L397 共 12 处）
  - 原因: 分类器接收的工具名应与新工具名一致

- [x] 更新 `subagent/skill_preload.rs` — fake tool call 中的工具名
  - 位置: `rust-agent-middlewares/src/subagent/skill_preload.rs`
  - 注释（~L11）：`` fake read_file 工具调用 `` → `` fake Read 工具调用 ``
  - 注释（~L21）：`` [ToolUse{read_file, ...}] `` → `` [ToolUse{Read, ...}] ``
  - `ContentBlock::tool_use(format!("skill_preload_{}", i), "read_file", ...)` → `"Read"`（~L103）
  - 测试（~L287）：`assert_eq!(tool_calls[0].name, "read_file")` → `assert_eq!(tool_calls[0].name, "Read")`
  - 原因: fake tool call 使用的工具名需与实际工具名一致，否则 LLM 会收到不一致的上下文

- [x] 为 TUI 层工具名更新编写单元测试
  - 测试文件: `rust-agent-tui/src/app/tool_display.rs` → 在文件末尾新增 `#[cfg(test)] mod tests`
  - 新增测试函数:
    ```rust
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_format_tool_name_new_names() {
            assert_eq!(format_tool_name("Read"), "Read");
            assert_eq!(format_tool_name("Write"), "Write");
            assert_eq!(format_tool_name("Edit"), "Edit");
            assert_eq!(format_tool_name("Glob"), "Glob");
            assert_eq!(format_tool_name("Grep"), "Grep");
            assert_eq!(format_tool_name("Bash"), "Shell");
            assert_eq!(format_tool_name("TodoWrite"), "Todo");
            assert_eq!(format_tool_name("AskUserQuestion"), "Ask");
            assert_eq!(format_tool_name("Agent"), "Agent");
        }

        #[test]
        fn test_format_tool_args_grep_uses_pattern() {
            let input = serde_json::json!({"pattern": "needle", "output_mode": "content"});
            let result = format_tool_args("Grep", &input, None);
            assert!(result.is_some(), "Grep 工具应返回 pattern 摘要");
            assert!(result.unwrap().contains("needle"), "应包含 pattern 内容");
        }

        #[test]
        fn test_format_tool_args_bash_uses_command() {
            let input = serde_json::json!({"command": "cargo test"});
            let result = format_tool_args("Bash", &input, None);
            assert!(result.is_some());
            assert!(result.unwrap().contains("cargo test"));
        }

        #[test]
        fn test_old_tool_names_not_matched() {
            // 验证旧工具名不再被匹配（fallback 到 to_pascal）
            assert_eq!(format_tool_name("bash"), "Bash"); // fallback
            assert_eq!(format_tool_name("read_file"), "ReadFile"); // fallback to_pascal
            assert_eq!(format_tool_name("write_file"), "WriteFile"); // fallback to_pascal
            assert_eq!(format_tool_name("search_files_rg"), "SearchFilesRg"); // fallback to_pascal
            assert_eq!(format_tool_name("launch_agent"), "LaunchAgent"); // fallback to_pascal
        }
    }
    ```
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::tool_display::tests`
  - 预期: 所有测试通过

- [x] 为 HITL 工具名更新编写单元测试
  - 测试文件: `rust-agent-middlewares/src/hitl/mod.rs` → `mod tests`
  - 新增测试函数 `test_new_tool_names_approval_consistency`:
    ```rust
    #[test]
    fn test_new_tool_names_approval_consistency() {
        // 新工具名：需审批
        assert!(default_requires_approval("Bash"), "Bash 应需审批");
        assert!(default_requires_approval("Write"), "Write 应需审批");
        assert!(default_requires_approval("Edit"), "Edit 应需审批");
        assert!(default_requires_approval("Agent"), "Agent 应需审批");
        assert!(default_requires_approval("folder_operations"), "folder_operations 应需审批");
        // 新工具名：不需审批
        assert!(!default_requires_approval("Read"), "Read 不应需审批");
        assert!(!default_requires_approval("Glob"), "Glob 不应需审批");
        assert!(!default_requires_approval("Grep"), "Grep 不应需审批");
        assert!(!default_requires_approval("TodoWrite"), "TodoWrite 不应需审批");
        assert!(!default_requires_approval("AskUserQuestion"), "AskUserQuestion 不应需审批");
    }
    ```
  - 新增测试函数 `test_is_edit_tool_new_names`:
    ```rust
    #[test]
    fn test_is_edit_tool_new_names() {
        assert!(is_edit_tool("Write"), "Write 应为编辑工具");
        assert!(is_edit_tool("Edit"), "Edit 应为编辑工具");
        assert!(!is_edit_tool("Bash"), "Bash 不应为编辑工具");
        assert!(!is_edit_tool("Agent"), "Agent 不应为编辑工具");
        assert!(!is_edit_tool("Read"), "Read 不应为编辑工具");
    }
    ```
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- hitl::tests::test_new_tool_names_approval_consistency hitl::tests::test_is_edit_tool_new_names`
  - 预期: 所有测试通过

- [x] 为 ToolCategory 新名称编写单元测试
  - 测试文件: `rust-agent-tui/src/ui/message_view.rs` → `mod tests`
  - 新增测试函数:
    ```rust
    #[test]
    fn test_tool_category_new_names() {
        assert_eq!(ToolCategory::from_tool_name("Read"), Some(ToolCategory::Read));
        assert_eq!(ToolCategory::from_tool_name("Grep"), Some(ToolCategory::Search));
        assert_eq!(ToolCategory::from_tool_name("Glob"), Some(ToolCategory::Glob));
        assert_eq!(ToolCategory::from_tool_name("Write"), None);
        assert_eq!(ToolCategory::from_tool_name("Bash"), None);
        assert_eq!(ToolCategory::from_tool_name("Agent"), None);
    }
    ```
  - 新增测试函数:
    ```rust
    #[test]
    fn test_tool_color_new_names() {
        use super::tool_color;
        // 读取/搜索 — SAGE
        assert_eq!(tool_color("Read"), theme::SAGE);
        assert_eq!(tool_color("Glob"), theme::SAGE);
        assert_eq!(tool_color("Grep"), theme::SAGE);
        // 写入/编辑 — WARNING
        assert_eq!(tool_color("Write"), theme::WARNING);
        assert_eq!(tool_color("Edit"), theme::WARNING);
        // 执行 — MODEL_INFO
        assert_eq!(tool_color("Bash"), theme::MODEL_INFO);
        // 代理/交互 — THINKING
        assert_eq!(tool_color("Agent"), theme::THINKING);
        assert_eq!(tool_color("AskUserQuestion"), theme::THINKING);
        assert_eq!(tool_color("TodoWrite"), theme::THINKING);
    }
    ```
  - 运行命令: `cargo test -p rust-agent-tui --lib -- ui::message_view::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 TUI 层无旧工具名残留
  - `grep -rn '"bash"\|"read_file"\|"write_file"\|"edit_file"\|"glob_files"\|"search_files_rg"\|"todo_write"\|"ask_user_question"\|"launch_agent"' rust-agent-tui/src/ --include='*.rs'`
  - 预期: 无匹配输出（旧名称已全部替换）
- [x] 验证提示词段落无旧工具名残留
  - `grep -rn 'read_file\|write_file\|edit_file\|glob_files\|search_files_rg\|launch_agent' rust-agent-tui/prompts/sections/ --include='*.md'`
  - 预期: 无匹配输出（`bash` 作为通用术语保留在 `06_tone_style.md` 的 "shell command" 上下文中）
- [x] 验证 HITL 模块无旧工具名残留
  - `grep -rn '"bash"\|"launch_agent"\|"write_file"\|"edit_file"\|"read_file"' rust-agent-middlewares/src/hitl/ --include='*.rs'`
  - 预期: 无匹配输出
- [x] 验证 `skill_preload.rs` 无旧工具名残留
  - `grep -n 'read_file' rust-agent-middlewares/src/subagent/skill_preload.rs`
  - 预期: 无匹配输出
- [x] 验证 TUI 层编译和测试通过
  - `cargo test -p rust-agent-tui`
  - 预期: 所有测试通过，无编译错误
- [x] 验证 middlewares 层编译和测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 所有测试通过，无编译错误
- [x] 验证全量构建通过
  - `cargo build`
  - 预期: 构建成功，无编译错误或警告

---

### Task 9: 工具对齐（下）总验收

**前置条件:**
- plan-1 Task 1-4（Write/Edit/Glob/Read/AskUserQuestion/TodoWrite）全部完成
- plan-1 Task 5（工具对齐（上）验收）通过
- plan-2 Task 5（Bash timeout 迁移 + 新参数）完成
- plan-2 Task 6（Grep 重大重构）完成
- plan-2 Task 7（Agent 重大重构）完成
- plan-2 Task 8（TUI 层和提示词全局更新）完成

**端到端验证:**

1. 运行全量 workspace 构建确保无编译错误
   - `cargo build`
   - 预期: 构建成功，无编译错误或警告
   - 失败排查: 检查编译错误指向的文件，对照对应 Task 的执行步骤

2. 运行完整测试套件确保无回归
   - `cargo test`
   - 预期: 全部测试通过
   - 失败排查: 按失败测试所属 crate 定位 — `rust-agent-middlewares` 测试失败排查 plan-2 Task 5/6/7，`rust-agent-tui` 测试失败排查 plan-2 Task 8

3. 验证所有旧工具名在整个代码库中无残留（comprehensive grep）
   - `grep -rn '"bash"\|"read_file"\|"write_file"\|"edit_file"\|"glob_files"\|"search_files_rg"\|"todo_write"\|"ask_user_question"\|"launch_agent"' rust-agent-middlewares/src/ rust-agent-tui/src/ --include='*.rs'`
   - 预期: 无匹配输出（所有源码中的旧工具名字符串已替换）
   - 失败排查: 按文件路径定位 — `terminal.rs` → plan-2 Task 5，`grep.rs` → plan-2 Task 6，`tool.rs`/`mod.rs` → plan-2 Task 7，TUI 文件 → plan-2 Task 8，hitl 文件 → plan-2 Task 8

4. 验证新工具名在工具定义中正确生效
   - `grep -n 'fn name()' rust-agent-middlewares/src/tools/filesystem/write.rs rust-agent-middlewares/src/tools/filesystem/edit.rs rust-agent-middlewares/src/tools/filesystem/glob.rs rust-agent-middlewares/src/tools/filesystem/read.rs rust-agent-middlewares/src/middleware/terminal.rs rust-agent-middlewares/src/tools/filesystem/grep.rs rust-agent-middlewares/src/tools/todo.rs rust-agent-middlewares/src/tools/ask_user_tool.rs rust-agent-middlewares/src/subagent/tool.rs`
   - 预期: 各文件分别返回 `"Write"`、`"Edit"`、`"Glob"`、`"Read"`、`"Bash"`、`"Grep"`、`"TodoWrite"`、`"AskUserQuestion"`、`"Agent"`
   - 失败排查: 检查对应 Task 的 `fn name()` 修改步骤

5. 验证 HITL 审批列表使用新工具名
   - `grep -n 'default_requires_approval\|is_edit_tool' rust-agent-middlewares/src/hitl/mod.rs | head -30`
   - 预期: 函数体中出现 `"Bash"`、`"Write"`、`"Edit"`、`"Agent"` 等新工具名，无旧名称
   - 失败排查: 检查 plan-2 Task 8 的 `hitl/mod.rs` 更新步骤

6. 验证提示词段落文件使用新工具名
   - `grep -rn 'read_file\|write_file\|edit_file\|glob_files\|search_files_rg\|launch_agent\|todo_write\|ask_user_question' rust-agent-tui/prompts/sections/ --include='*.md'`
   - 预期: 无匹配输出（`bash` 作为通用术语保留在 `06_tone_style.md` 的 "shell command" 上下文中不算残留）
   - 失败排查: 检查 plan-2 Task 8 的提示词段落更新步骤

7. 验证工具显示和颜色映射使用新名称
   - `cargo test -p rust-agent-tui --lib -- app::tool_display::tests`
   - 预期: 所有测试通过（包括 `test_format_tool_name_new_names`、`test_old_tool_names_not_matched`）
   - 失败排查: 检查 plan-2 Task 8 的 `tool_display.rs` 更新步骤
