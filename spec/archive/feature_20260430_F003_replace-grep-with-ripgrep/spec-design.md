# 替换外部 rg 进程为 ripgrep crate 进程内搜索

## 背景

当前 `SearchFilesRgTool` 通过 `tokio::process::Command` 外部调用系统安装的 `rg` 二进制进行文件内容搜索。这要求用户预先安装 ripgrep，且每次搜索都需要 fork 进程，存在外部依赖和进程开销。

## 目标

将 `SearchFilesRgTool` 的底层实现从外部 `rg` 进程调用替换为进程内的 `grep` + `grep-regex` crate，消除对外部 ripgrep 二进制的运行时依赖，同时保持高性能搜索能力。

## 范围

- 仅替换 `rust-agent-middlewares/src/tools/filesystem/grep.rs` 的内部实现
- 工具名称 (`search_files_rg`)、参数 schema、description、输出格式保持不变（LLM 侧无感知）
- 保持等价功能：正则搜索、glob 过滤、type 过滤、上下文行、`-l`/`-c` 输出模式、15 秒超时、500 行上限
- 保持 `.gitignore` 尊重、hidden 文件跳过、二进制文件跳过的默认行为（对齐 rg 默认行为）

## 技术方案

### 新增依赖

| Crate | 版本 | 作用 |
|-------|------|------|
| `grep` | latest | 核心类型：`Searcher`、`Sink` trait、`sinks::UTF8` |
| `grep-regex` | latest | `RegexMatcher`：桥接 `regex` crate，支持 SIMD 加速 + Unicode |

`ignore` crate 已在项目依赖中，直接复用 `WalkBuilder` 做目录遍历。

> 注意：`ripgrep` crate 本身不是可用的库（仅发布 CLI 二进制），必须使用其底层子 crate。

### 架构

```
用户输入 args: ["-n", "pattern", "src/"]
  ↓
参数解析层：从 args 中提取 OPTIONS、PATTERN、PATH
  ↓
构建搜索组件：
  - RegexMatcher::new(pattern)     — 正则编译
  - WalkBuilder(path)              — 目录遍历（自动 gitignore/hidden 过滤）
  - -g GLOB → walker.add_custom_ignore_filename() / type_filter
  ↓
搜索执行（并行）：
  - WalkParallel → 多线程并行搜索
  - 每个文件：Searcher::new().search_file(matcher, path, sink)
  - crossbeam channel 收集结果
  ↓
结果收集：
  - UTF8 sink 收集匹配行（行号 + 内容）
  - 累计行数达 head_limit 或超时则提前终止
  ↓
格式化输出：与当前 rg 输出格式一致
```

### 参数解析

只解析当前工具 description 中提到的常用参数（不追求完整兼容 rg 所有参数）：

| 参数 | 说明 |
|------|------|
| `-n` | 显示行号（默认始终开启） |
| `-l` | 仅输出匹配文件路径 |
| `-c` | 输出每个文件的匹配行数 |
| `-C N` | 显示匹配行前后 N 行上下文 |
| `-g GLOB` | glob 文件过滤（如 `"*.rs"`） |
| `-t TYPE` | 文件类型过滤（映射为 glob，如 `"rust"` → `"*.rs"`） |
| `-T TYPE` | 排除文件类型 |
| `PATTERN` | 正则模式 |
| `PATH` | 搜索路径（可选，默认 cwd） |

### 并行搜索

使用 `ignore::WalkParallel` + `crossbeam::channel` 收集结果：

- `WalkBuilder::build_parallel()` 生成并行迭代器
- 每个文件 entry 在独立线程中执行 `Searcher::search_file()`
- 匹配结果通过 bounded channel 发送到主线程
- 主线程收集并格式化，达到 `head_limit` 后通过 `AtomicBool` 信号通知其他线程停止

### 超时控制

使用 `tokio::time::timeout` 包裹整个搜索过程（15 秒）。搜索任务通过 `tokio::task::spawn_blocking()` 在独立线程池执行，避免阻塞 async runtime。

## 文件变更

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `rust-agent-middlewares/Cargo.toml` | 修改 | 新增 `grep`、`grep-regex` 依赖 |
| `rust-agent-middlewares/src/tools/filesystem/grep.rs` | 重写 | 替换实现：移除 `which_rg()` 和 `Command` 调用，改用 crate API |

**不变更**：
- 工具名、参数 schema、description
- 测试用例（移除 rg 二进制可用性检查的跳过逻辑）
- 其他文件

## 约束合规

- 遵循 `spec/global/constraints.md` 技术栈（Rust、tokio）
- `ignore` crate 复用现有依赖，无新增外部运行时依赖
- 新增 `grep`/`grep-regex` 为纯 Rust crate，与 bundled SQLite 一样编译进二进制

## 风险

- **编译体积增加**：`grep` + `grep-regex` 会增加二进制体积，但相对于整体框架影响较小
- **参数兼容性**：不追求完整兼容 rg 所有参数，未来如需扩展可逐步添加
