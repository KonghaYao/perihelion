# 替换外部 rg 进程为 ripgrep crate 进程内搜索 执行计划

**目标:** 将 SearchFilesRgTool 从外部 rg 进程调用替换为进程内 grep crate 搜索，消除外部二进制依赖

**技术栈:** Rust, grep 0.4 (re-exports grep-regex/grep-searcher/grep-matcher), ignore 0.4 (已有), tokio

**设计文档:** spec-design.md

## 改动总览

- 本次改动仅涉及 `rust-agent-middlewares` crate，修改 2 个文件（`Cargo.toml` 新增依赖、`grep.rs` 重写实现）
- Task 1 完成全部重写：添加依赖、构建参数解析器 + 并行搜索引擎、重写 `invoke()`、移除旧代码、更新测试
- `SearchFilesRgTool` 的公开接口（`name()`/`description()`/`parameters()`）完全不变，所有调用方（`FilesystemMiddleware`、TUI `tool_display`、compact config）无需修改
- 经代码确认，`ignore` crate 已在依赖中；`grep` 0.4 是 meta-crate，re-export `grep_regex as regex`、`grep_searcher as searcher`、`grep_matcher as matcher`，只需添加一个依赖

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证测试工具可用
  - `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep 2>&1 | tail -10`
  - 预期: 测试框架可用，现有 5 个 grep 测试全部通过（或因 rg 未安装而跳过）

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 error
- [x] 测试命令可用
  - `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep 2>&1 | grep -E "test result|running"`
  - 预期: 输出包含 `test result`，测试可执行

---

### Task 1: 重写 SearchFilesRgTool 为进程内 grep crate 搜索

**背景:**
[业务语境] 当前 `SearchFilesRgTool` 通过 `tokio::process::Command` 外部调用系统安装的 `rg` 二进制进行搜索，要求用户预装 ripgrep 且每次 fork 进程有开销
[修改原因] 替换为进程内 `grep` crate（ripgrep 的底层库）搜索，消除外部依赖，保持等价功能
[上下游影响] 无下游依赖——`SearchFilesRgTool` 的公开接口不变，所有调用方无需感知实现变更

**涉及文件:**
- 修改: `rust-agent-middlewares/Cargo.toml`
- 重写: `rust-agent-middlewares/src/tools/filesystem/grep.rs`

**执行步骤:**
- [x] 添加 `grep` 依赖到 Cargo.toml
  - 位置: `rust-agent-middlewares/Cargo.toml` 的 `[dependencies]` 段末尾（`parking_lot` 之后）
  - 添加一行: `grep = "0.4"`
  - 原因: `grep` 0.4 是 meta-crate，re-export `grep_regex` as `regex`、`grep_searcher` as `searcher`、`grep_matcher` as `matcher`，只需一个依赖即可获得完整搜索能力

- [x] 重写 grep.rs 文件：移除旧 import 和函数，添加新 import
  - 位置: `rust-agent-middlewares/src/tools/filesystem/grep.rs` 文件头部
  - 移除以下 import:
    ```rust
    use std::process::Stdio;
    use std::sync::OnceLock;
    use tokio::process::Command;
    use tokio::time::{timeout, Duration};
    ```
  - 保留 `use rust_create_agent::tools::BaseTool;`、`use serde_json::Value;`、`use std::path::Path;`
  - 添加新 import:
    ```rust
    use grep::regex::RegexMatcher;
    use grep::searcher::{SearcherBuilder, BinaryDetection, Sink, SinkMatch, SinkContext, SinkContextKind};
    use grep::matcher::Matcher;
    use ignore::WalkBuilder;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::cell::Cell;
    use tokio::time::{timeout, Duration};
    ```
  - 原因: 切换到 crate API，移除进程调用相关依赖

- [x] 定义 `ParsedArgs` 结构体和解析函数
  - 位置: `grep.rs` 中 `resolve_last_path_arg` 函数的位置（整个函数替换）
  - 删除 `resolve_last_path_arg` 函数（不再需要，路径解析在新逻辑中处理）
  - 定义:
    ```rust
    /// 从 args 数组中解析搜索参数
    struct ParsedArgs {
        pattern: String,
        path: Option<String>,       // 搜索路径，None 表示 cwd
        glob_filters: Vec<String>,  // -g 参数
        type_filters: Vec<String>,  // -t 参数
        type_excludes: Vec<String>, // -T 参数
        output_mode: OutputMode,    // 默认/文件名/计数
        context_lines: usize,       // -C 参数
        case_insensitive: bool,     // -i 参数
        whole_word: bool,           // -w 参数
    }

    enum OutputMode {
        Default,  // 显示匹配行
        FilesOnly, // -l
        CountOnly, // -c
    }
    ```
  - 实现 `parse_args(args: &[String]) -> Result<ParsedArgs, String>`:
    - 遍历 args，遇到 `-g` 取下一个值作为 glob filter
    - 遇到 `-t` 取下一个值作为 type filter
    - 遇到 `-T` 取下一个值作为 type exclude
    - 遇到 `-l` 设置 output_mode 为 FilesOnly
    - 遇到 `-c` 设置 output_mode 为 CountOnly
    - 遇到 `-C` 取下一个值解析为 usize 作为 context_lines
    - 遇到 `-i` 设置 case_insensitive 为 true
    - 遇到 `-n`（行号）忽略（始终开启）
    - 遇到 `-w`（整词匹配）传递给 RegexMatcherBuilder
    - 遇到 `-head_limit` 参数通过 head_limit 单独传入，不在此解析
    - 遇到 `--` 停止解析选项，后续均为位置参数
    - 非选项参数（不以 `-` 开头）：第一个非选项是 PATTERN，第二个非选项是 PATH
    - 如果 args 中只有一个非选项参数，则为 PATTERN，PATH 为 None
    - 如果有两个非选项参数，第一个是 PATTERN，第二个是 PATH
    - 解析失败返回错误字符串
  - 原因: 将 rg 命令行参数映射到结构化配置，比逐个传递给 Command 更可控

- [x] 定义自定义 `SearchSink` 实现 `Sink` trait
  - 位置: `grep.rs` 中 `ParsedArgs` 定义之后
  - 定义:
    ```rust
    /// 自定义 Sink，支持三种输出模式和行数限制
    struct SearchSink {
        output_mode: OutputMode,
        results: Arc<Mutex<Vec<String>>>,  // 收集格式化后的输出行
        total_lines: Arc<AtomicUsize>,      // 全局已收集行数（用于 head_limit 控制）
        max_limit: usize,                   // head_limit
        stopped: Arc<AtomicBool>,           // 全局停止信号
        display_path: String,               // 相对路径（用于输出格式化）
        match_count: Cell<usize>,           // 当前文件匹配计数（CountOnly 模式）
        has_match: Cell<bool>,              // 当前文件是否有匹配（FilesOnly 模式）
    }
    ```
  - 实现 `Sink for SearchSink`:
    - `type Error = std::io::Error;`
    - `matched()`: 根据 `output_mode` 分别处理:
      - `Default`: 从 `SinkMatch` 获取行号 `mat.line_number()` 和行内容 `mat.bytes()`，格式化为 `display_path:line_number: content`（去除行尾换行），追加到 `results`，`total_lines` 加 1，检查是否达到 `max_limit`
      - `CountOnly`: `match_count` 加 1，不追加到 `results`（在文件搜索完成后统一处理）
      - `FilesOnly`: 设置 `has_match = true`，返回 `Ok(false)` 终止当前文件搜索
    - 每次追加结果后检查 `total_lines.load() >= max_limit`，达到则设置 `stopped = true` 并返回 `Ok(false)`
    - `context()`: 当 `context_lines > 0` 时，从 `SinkContext` 获取行号和内容，根据 `kind`（`Before`/`After`）格式化为 `display_path:line_number- content` 或 `display_path:line_number+ content`，追加到 `results`，同样检查 limit
  - 原因: `grep::searcher::sinks::UTF8` 只支持 Default 模式且忽略上下文行，自定义 Sink 可支持 -l/-c/-C 三种模式

- [x] 实现核心搜索函数 `execute_search`
  - 位置: `grep.rs` 中 `SearchSink` 实现之后
  - 签名:
    ```rust
    fn execute_search(
        parsed: &ParsedArgs,
        cwd: &str,
        head_limit: usize,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
    ```
  - 实现:
    1. 构建搜索路径: 如果 `parsed.path` 是相对路径，用 `Path::new(cwd).join()` 转为绝对路径
    2. 构建 RegexMatcher:
       ```rust
       let matcher = RegexMatcherBuilder::new()
           .case_insensitive(parsed.case_insensitive)
           .word(parsed.whole_word)
           .build(&parsed.pattern)?;
       ```
       注意: `RegexMatcherBuilder` 来自 `grep::regex::RegexMatcherBuilder`
    3. 构建 WalkBuilder:
       ```rust
       let mut builder = WalkBuilder::new(&search_path);
       builder
           .hidden(true)        // 跳过隐藏文件（对齐 rg 默认行为）
           .git_ignore(true)    // 遵守 .gitignore
           .git_exclude(true)   // 遵守 .git/info/exclude
           .ignore(true)        // 遵守 .ignore
           .parents(true)       // 遵守父目录 ignore 文件
           .threads(num_cpus::get());  // 并行线程数
       ```
    4. 预编译 glob 过滤器:
       在并行遍历回调外预编译 glob 模式，在回调中对文件名进行匹配检查:
       ```rust
       let glob_filters: Vec<glob::Pattern> = parsed.glob_filters.iter()
           .filter_map(|g| glob::Pattern::new(g).ok())
           .collect();
       ```
       注意: `glob` crate 已在依赖中。`-t`/`-T` 类型过滤暂不实现，在 `parse_args` 中解析后记录日志但跳过（ripgrep 的类型定义表较大，首期不内置）。
    5. 构建共享状态:
       ```rust
       let results = Arc::new(Mutex::new(Vec::new()));
       let total_lines = Arc::new(AtomicUsize::new(0));
       let stopped = Arc::new(AtomicBool::new(false));
       let matcher = Arc::new(matcher);
       let cwd = Arc::new(cwd.to_string());
       ```
    6. 并行搜索:
       ```rust
       builder.build_parallel().run(|| {
           let matcher = Arc::clone(&matcher);
           let total_lines = Arc::clone(&total_lines);
           let stopped = Arc::clone(&stopped);
           let cwd = Arc::clone(&cwd);
           let glob_filters = glob_filters.clone();

           Box::new(move |entry: &ignore::DirEntry| -> ignore::Result<()> {
               if stopped.load(Ordering::Relaxed) {
                   return Ok(());
               }
               if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                   return Ok(());
               }

               // -g glob 过滤
               if !glob_filters.is_empty() {
                   let file_name = entry.file_name().to_string_lossy();
                   if !glob_filters.iter().any(|p| p.matches(&file_name)) {
                       return Ok(());
                   }
               }

               // 显示路径：相对于 cwd 的路径
               let display_path = entry.path().strip_prefix(cwd.as_str())
                   .unwrap_or(entry.path())
                   .to_string_lossy()
                   .to_string();

               let searcher = SearcherBuilder::new()
                   .line_number(true)
                   .binary_detection(BinaryDetection::quit(b'\x00'))
                   .build();

               let file_results = Arc::clone(&results);
               let file_stopped = Arc::clone(&stopped);
               let file_limit = Arc::clone(&total_lines);

               let mut sink = SearchSink {
                   output_mode: parsed.output_mode.clone(),
                   results: Arc::clone(&file_results),
                   total_lines: Arc::clone(&file_limit),
                   max_limit: head_limit,
                   stopped: Arc::clone(&file_stopped),
                   display_path: display_path.clone(),
                   match_count: Cell::new(0),
                   has_match: Cell::new(false),
               };

               match searcher.search_path(&matcher, entry.path(), &mut sink) {
                   Ok(_) => {},
                   Err(_) => {
                       // 二进制文件等错误，跳过
                       return Ok(());
                   }
               }

               // 收集当前文件结果（FilesOnly / CountOnly 模式在搜索完成后处理）
               if matches!(parsed.output_mode, OutputMode::FilesOnly) && sink.has_match.get() {
                   let mut r = file_results.lock().unwrap();
                   r.push(display_path.clone());
               } else if matches!(parsed.output_mode, OutputMode::CountOnly) && sink.match_count.get() > 0 {
                   let mut r = file_results.lock().unwrap();
                   r.push(format!("{}:{}", display_path, sink.match_count.get()));
               }
               // Default 模式: results 已在 sink.matched() 中直接追加

               Ok(())
           })
       });
       ```
    7. 格式化输出:
       - Default 模式: 拼接所有匹配行（`path:line_num: content`）
       - FilesOnly 模式: 每行一个匹配文件路径
       - CountOnly 模式: 每行 `path: count`
       - 无匹配返回 `"No matches found."`
       - 超过 head_limit 截断
  - 注意: `WalkParallel::run()` 的回调签名是 `Fn() -> Box<dyn FnMut(&DirEntry) -> Result<()>> + Send + Sync`，每个线程获得独立的闭包
  - 原因: 并行搜索是高性能场景的核心，`ignore::WalkParallel` 自动管理线程池

- [x] 重写 `invoke()` 方法
  - 位置: `rust-agent-middlewares/src/tools/filesystem/grep.rs` 的 `impl BaseTool for SearchFilesRgTool` 中的 `invoke()` 方法（L85-L152）
  - 保留 `name()`、`description()`、`parameters()` 不变
  - 新 `invoke()` 实现:
    ```rust
    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args_val = input["args"]
            .as_array()
            .ok_or("Missing args parameter (array of strings)")?;

        if args_val.is_empty() {
            return Ok("Error: No arguments provided. Please provide ripgrep arguments.".to_string());
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
        ).await;

        match result {
            Err(_) => Ok("Error: Search timed out after 15 seconds. Please use a more specific pattern.".to_string()),
            Ok(Err(e)) => Ok(format!("Error: {e}")),
            Ok(Ok(output)) => Ok(output),
        }
    }
    ```
  - 原因: 使用 `spawn_blocking` 将同步的 grep 搜索放到独立线程池，避免阻塞 async runtime；保留 15 秒超时和 500 行上限

- [x] 移除 `which_rg()` 函数
  - 位置: `rust-agent-middlewares/src/tools/filesystem/grep.rs` 文件末尾（L235-L256）
  - 删除整个 `which_rg()` 函数
  - 原因: 不再需要查找外部 rg 二进制

- [x] 更新测试用例
  - 位置: `rust-agent-middlewares/src/tools/filesystem/grep.rs` 的 `#[cfg(test)] mod tests` 段（L155-L232）
  - 修改所有测试：移除 `if result.starts_with("Error executing ripgrep") { return; }` 的跳过逻辑（不再需要 rg 二进制）
  - 保留 `test_search_files_rg_hit`、`test_search_files_rg_no_match`、`test_search_files_rg_empty_args`、`test_search_files_rg_regex`、`test_description_extended` 五个测试
  - 新增测试:
    - `test_search_files_rg_files_only`: 使用 `["-l", "needle", "./"]` 参数，验证返回结果包含文件路径且不包含行内容
    - `test_search_files_rg_count`: 使用 `["-c", "needle", "./"]` 参数，验证返回结果包含匹配计数
    - `test_search_files_rg_case_insensitive`: 使用 `["-i", "NEEDLE", "./"]` 参数，验证大小写不敏感匹配
    - `test_search_files_rg_glob_filter`: 使用 `["-n", "-g", "*.txt", "needle", "./"]` 参数，创建 .rs 和 .txt 文件，验证只搜索 .txt 文件
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 grep 依赖添加成功
  - `grep "grep = " rust-agent-middlewares/Cargo.toml`
  - 预期: 输出包含 `grep = "0.4"`
- [x] 验证旧代码已移除
  - `grep -c "which_rg\|tokio::process::Command\|OnceLock\|Stdio" rust-agent-middlewares/src/tools/filesystem/grep.rs`
  - 预期: 输出为 0
- [x] 验证新代码包含关键 API 调用
  - `grep -c "RegexMatcher\|WalkBuilder\|SearcherBuilder\|SearchSink\|spawn_blocking" rust-agent-middlewares/src/tools/filesystem/grep.rs`
  - 预期: 输出大于 0
- [x] 验证构建成功
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 error
- [x] 验证测试通过
  - `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep 2>&1 | tail -5`
  - 预期: 所有测试通过，无 skipped

---

### Task 2: 功能验收

**前置条件:**
- Task 0 和 Task 1 已完成
- 构建成功：`cargo build -p rust-agent-middlewares`

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10`
   - 预期: 全部测试通过
   - 失败排查: 检查 Task 1 的测试步骤

2. 验证 grep crate 搜索功能与原 rg 行为一致
   - `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep 2>&1`
   - 预期: 所有 grep 测试通过（包括新增的 -l/-c/-i/-g 测试）
   - 失败排查: 检查 Task 1 中 `execute_search` 的输出格式化逻辑

3. 验证全 workspace 构建无破坏
   - `cargo build 2>&1 | tail -5`
   - 预期: 全部三个 crate 构建成功（rust-create-agent, rust-agent-middlewares, rust-agent-tui）
   - 失败排查: 检查 `rust-agent-middlewares` 的公开 API 是否有破坏性变更

4. 验证 TUI 层无编译错误
   - `cargo build -p rust-agent-tui 2>&1 | tail -5`
   - 预期: 构建成功
   - 失败排查: `SearchFilesRgTool` 的公开接口未变，TUI 层不应有编译问题

5. 验证全 workspace 测试通过
   - `cargo test 2>&1 | tail -15`
   - 预期: 全部测试通过
   - 失败排查: 检查各 crate 测试输出
