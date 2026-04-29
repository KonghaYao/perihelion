# syntect 代码块语法高亮 执行计划

**目标:** 为 perihelion-widgets 的 Markdown 渲染器引入 syntect 语法高亮，多行代码块根据语言标签自动着色，未识别语言时回退到统一颜色。

**技术栈:** Rust 2021, syntect 5 (default-fancy), once_cell::sync::Lazy, ratatui Span/Line, cfg feature flag

**设计文档:** spec/feature_20260429_F001_syntect-codeblock-highlight/spec-design.md

## 改动总览

本次改动集中在 `perihelion-widgets` crate 的 Markdown 渲染模块，涉及 4 个文件（2 个 Cargo.toml + highlight.rs 新建 + render_state.rs 修改）。Task 1 建立 feature flag 基础设施，Task 2 创建高亮引擎模块，Task 3 将引擎接入渲染状态机。三个 Task 严格顺序依赖。关键决策：syntect `│` 前缀颜色使用固定灰色（syntect Theme 无 muted() 方法），`highlight_code_block` 返回 `Option` 实现 fallback。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 Cargo 构建可用
  - `cargo build -p perihelion-widgets --features markdown 2>&1 | tail -3`
  - 预期: 编译成功，无错误

**检查步骤:**
- [x] 验证构建命令执行成功
  - `cargo check -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 编译成功
- [x] 验证测试框架可用
  - `cargo test -p perihelion-widgets --features markdown --no-run 2>&1 | tail -3`
  - 预期: 测试编译成功

---

### Task 1: Feature flag 和依赖配置

**背景:**
为 syntect 语法高亮功能建立 feature flag 基础设施。syntect 作为可选依赖引入，通过 `markdown-highlight` feature 启用，不影响默认构建（`default = []`）和仅 markdown 场景。本 Task 的输出被 Task 2（高亮引擎模块）和 Task 3（渲染集成）依赖，它们通过 `#[cfg(feature = "markdown-highlight")]` 条件编译使用 syntect。

**涉及文件:**
- 修改: `perihelion-widgets/Cargo.toml`
- 修改: `rust-agent-tui/Cargo.toml`

**执行步骤:**
- [x] 在 `perihelion-widgets/Cargo.toml` 的 `[dependencies]` 段中新增 syntect 可选依赖和 once_cell 依赖
  - 位置: `perihelion-widgets/Cargo.toml` 的 `[dependencies]` 段，`pulldown-cmark` 行之后
  - 追加内容:
    ```toml
    syntect = { version = "5", default-features = false, features = ["default-fancy"], optional = true }
    once_cell = "1"
    ```
  - 原因: `default-fancy` 使用纯 Rust `fancy-regex`，避免 C 库 oniguruma 编译问题；`once_cell` 用于 `Lazy` 延迟初始化 `SyntaxSet`/`ThemeSet`（syntect 传递依赖中包含 once_cell，但为明确性直接声明）

- [x] 在 `perihelion-widgets/Cargo.toml` 的 `[features]` 段中新增 `markdown-highlight` feature
  - 位置: `perihelion-widgets/Cargo.toml` 的 `[features]` 段，`markdown = ["pulldown-cmark"]` 行之后
  - 追加内容:
    ```toml
    markdown-highlight = ["markdown", "dep:syntect"]
    ```
  - 原因: `markdown-highlight` 依赖 `markdown`（需要 pulldown-cmark 解析）和 `dep:syntect`（语法高亮引擎）。`dep:` 前缀是 Cargo workspace resolver v2 的显式依赖语法。

- [x] 修改 `rust-agent-tui/Cargo.toml` 启用 `markdown-highlight` feature
  - 位置: `rust-agent-tui/Cargo.toml` 第 40 行
  - 将 `features = ["markdown"]` 改为 `features = ["markdown-highlight"]`
  - 完整行: `perihelion-widgets = { path = "../perihelion-widgets", features = ["markdown-highlight"] }`
  - 原因: TUI 应用需要语法高亮功能，启用 `markdown-highlight` 会自动传递启用 `markdown` 和 `syntect`

- [x] 验证编译通过
  - 运行命令: `cargo build -p perihelion-widgets --features markdown-highlight 2>&1 | tail -5`
  - 预期: 编译成功（可能因缺少 highlight.rs 模块报错，但 Cargo.toml 解析和依赖解析应通过）

**检查步骤:**
- [x] 验证 perihelion-widgets 的 feature 定义正确
  - `grep -A2 '\[features\]' perihelion-widgets/Cargo.toml`
  - 预期: 输出包含 `default = []`、`markdown = ["pulldown-cmark"]`、`markdown-highlight = ["markdown", "dep:syntect"]` 三行

- [x] 验证 syntect 依赖声明正确
  - `grep 'syntect' perihelion-widgets/Cargo.toml`
  - 预期: 输出 `syntect = { version = "5", default-features = false, features = ["default-fancy"], optional = true }`

- [x] 验证 rust-agent-tui 引用 markdown-highlight feature
  - `grep 'perihelion-widgets' rust-agent-tui/Cargo.toml`
  - 预期: 输出包含 `features = ["markdown-highlight"]`

- [x] 验证默认构建（不启用 markdown-highlight）仍能编译
  - `cargo check -p perihelion-widgets 2>&1 | tail -3`
  - 预期: 编译成功，无 syntect 相关错误

- [x] 验证启用 markdown-highlight 后依赖解析成功
  - `cargo check -p perihelion-widgets --features markdown-highlight 2>&1 | tail -3`
  - 预期: 依赖解析成功（可能因后续 Task 未实现而有编译错误，但依赖图正确）


---

### Task 2: 高亮引擎模块

**背景:**
创建 syntect 语法高亮的核心引擎模块，提供 `highlight_code_block()` 函数供 Task 3 的渲染状态机调用。本模块封装 syntect 的 `SyntaxSet`/`ThemeSet` 延迟初始化，将 syntect API 转换为 ratatui `Span`/`Line` 输出。Task 3（渲染集成）依赖本 Task 的 `highlight_code_block` 函数。

**涉及文件:**
- 新建: `perihelion-widgets/src/markdown/highlight.rs`
- 修改: `perihelion-widgets/src/markdown/mod.rs`

**执行步骤:**
- [x] 在 `perihelion-widgets/src/markdown/mod.rs` 中注册 highlight 子模块
  - 位置: `perihelion-widgets/src/markdown/mod.rs` 第 1 行（`mod render_state;` 之后）
  - 插入内容:
    ```rust
    #[cfg(feature = "markdown-highlight")]
    mod highlight;
    ```
  - 原因: highlight 模块仅在 `markdown-highlight` feature 启用时编译

- [x] 新建 `perihelion-widgets/src/markdown/highlight.rs`，实现延迟初始化静态量和 `highlight_code_block` 函数
  - 位置: 新文件 `perihelion-widgets/src/markdown/highlight.rs`
  - 完整文件内容:
    ```rust
    use once_cell::sync::Lazy;
    use ratatui::{
        style::{Color, Style},
        text::{Line, Span},
    };
    use syntect::highlighting::{HighlightLines, ThemeSet};
    use syntect::parsing::SyntaxSet;

    pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
    pub static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

    /// │ 前缀的固定灰色，与 TUI 暗色背景视觉协调
    const PREFIX_COLOR: Color = Color::Rgb(130, 140, 150);

    /// 对多行代码块进行语法高亮，返回着色后的 Line 列表。
    /// 当语言标签未识别时返回 None，调用方应回退到统一颜色渲染。
    pub fn highlight_code_block(lang: &str, lines: &[String]) -> Option<Vec<Line<'static>>> {
        let ss = &*SYNTAX_SET;
        let syntax = ss.find_syntax_by_token(lang)?;
        let theme = &THEME_SET.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut result = Vec::with_capacity(lines.len());
        for line_text in lines {
            let mut spans = Vec::new();
            spans.push(Span::styled("│ ".to_string(), Style::default().fg(PREFIX_COLOR)));

            let ranges = highlighter.highlight_line(line_text, ss).ok()?;
            for (style, text) in ranges {
                let color = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                spans.push(Span::styled(text.to_string(), Style::default().fg(color)));
            }
            result.push(Line::from(spans));
        }
        Some(result)
    }
    ```
  - 关键设计决策:
    - `PREFIX_COLOR` 使用固定灰色 `Rgb(130, 140, 150)`，因为 syntect 的 `Theme` 类型没有 `muted()` 方法，无法从主题中提取通用弱化色
    - `find_syntax_by_token(lang)` 按 fence tag 查找语言定义，空字符串或未识别语言返回 `None`
    - `HighlightLines` 逐行调用时内部维护解析状态，多行注释/字符串等跨行结构可正确高亮
    - `highlight_line` 返回 `Result<Vec<(syntect::highlighting::Style, &str)>>`，`.ok()?` 将错误转为 `None` 触发 fallback

- [x] 为 `highlight_code_block` 编写单元测试
  - 测试文件: `perihelion-widgets/src/markdown/highlight.rs` 末尾（`#[cfg(test)] mod tests`）
  - 测试场景:
    - `highlight_rust_code`: 输入 `lang="rust"`, `lines=["fn main() {}"]` → 返回 `Some`，验证结果包含 `│` 前缀 span 和非空内容 span，至少有一个 span 的 fg 颜色不是 `PREFIX_COLOR`（证明产生了语法着色）
    - `highlight_unknown_lang`: 输入 `lang="unknown_lang_xyz"`, `lines=["hello"]` → 返回 `None`
    - `highlight_empty_lang`: 输入 `lang=""`, `lines=["hello"]` → 返回 `None`
    - `highlight_multiline`: 输入 `lang="rust"`, `lines=["fn main() {", "    println!(\"hello\");", "}"]` → 返回 `Some`，验证结果行数等于输入行数 3
  - 运行命令: `cargo test -p perihelion-widgets --features markdown-highlight -- highlight::tests 2>&1 | tail -10`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 highlight.rs 文件存在且结构正确
  - `test -f perihelion-widgets/src/markdown/highlight.rs && echo "exists" || echo "missing"`
  - 预期: 输出 "exists"

- [x] 验证 mod.rs 中注册了 highlight 模块
  - `grep 'mod highlight' perihelion-widgets/src/markdown/mod.rs`
  - 预期: 输出 `#[cfg(feature = "markdown-highlight")]` 和 `mod highlight;` 两行

- [x] 验证 highlight.rs 中关键函数签名
  - `grep 'pub fn highlight_code_block' perihelion-widgets/src/markdown/highlight.rs`
  - 预期: 输出 `pub fn highlight_code_block(lang: &str, lines: &[String]) -> Option<Vec<Line<'static>>>`

- [x] 验证启用 markdown-highlight 后编译通过
  - `cargo check -p perihelion-widgets --features markdown-highlight 2>&1 | tail -5`
  - 预期: 编译成功，无错误

- [x] 验证单元测试通过
  - `cargo test -p perihelion-widgets --features markdown-highlight -- highlight::tests 2>&1 | tail -10`
  - 预期: 所有 4 个测试通过


---

### Task 3: 渲染集成

**背景:**
将 Task 2 的高亮引擎接入 Markdown 渲染状态机，使多行代码块具备语法着色能力。当前 `render_state.rs` 的 `Event::End(TagEnd::CodeBlock)` 处理器（L540-571）对多行代码块使用统一的 `theme.text()` 颜色输出。本 Task 在多行分支（`end > 1`）中优先调用 `highlight_code_block()`，syntect 无法识别语言时回退到现有统一颜色逻辑。Task 2 提供的 `highlight_code_block()` 函数是本 Task 的直接依赖。

**涉及文件:**
- 修改: `perihelion-widgets/src/markdown/render_state.rs`

**执行步骤:**
- [x] 在 `render_state.rs` 顶部新增条件导入
  - 位置: `perihelion-widgets/src/markdown/render_state.rs` 第 8 行（`use super::MarkdownTheme;` 之后）
  - 插入内容:
    ```rust
    #[cfg(feature = "markdown-highlight")]
    use super::highlight::highlight_code_block;
    ```
  - 原因: render_state.rs 需要调用 highlight 模块的 `highlight_code_block` 函数，仅在 `markdown-highlight` feature 启用时导入

- [x] 替换多行代码块分支（`end > 1`）为 cfg-gated 实现
  - 位置: `perihelion-widgets/src/markdown/render_state.rs` L557-570（从 `} else if end > 1 {` 到其闭合 `}`）
  - 将整个 `else if end > 1 { ... }` 块替换为以下代码:
    ```rust
                } else if end > 1 {
                    // 多行代码块
                    #[cfg(feature = "markdown-highlight")]
                    if let Some(highlighted) = highlight_code_block(&self.code_block_lang, &lines[..end]) {
                        self.lines.extend(highlighted);
                    } else {
                        // syntect 未识别语言，回退到统一颜色
                        for line_text in &lines[..end] {
                            self.current_spans.push(Span::styled(
                                "│ ".to_string(),
                                Style::default().fg(self.theme.muted()),
                            ));
                            self.current_spans.push(Span::styled(
                                line_text.clone(),
                                Style::default().fg(self.theme.text()),
                            ));
                            self.flush_line();
                        }
                    }

                    #[cfg(not(feature = "markdown-highlight"))]
                    for line_text in &lines[..end] {
                        self.current_spans.push(Span::styled(
                            "│ ".to_string(),
                            Style::default().fg(self.theme.muted()),
                        ));
                        self.current_spans.push(Span::styled(
                            line_text.clone(),
                            Style::default().fg(self.theme.text()),
                        ));
                        self.flush_line();
                    }
                }
    ```
  - 原因: `#[cfg(feature = "markdown-highlight")]` 分支优先调用 `highlight_code_block()`，成功则直接 extend 高亮结果（Line 已包含 `│` 前缀和着色 span），失败则回退到与当前完全相同的统一颜色渲染。`#[cfg(not(...))]` 分支保持现有行为不变。单行分支（`end == 1`）不修改。

- [x] 验证编译通过（两种 feature 配置）
  - 运行命令: `cargo check -p perihelion-widgets --features markdown-highlight 2>&1 | tail -5`
  - 预期: 编译成功
  - 运行命令: `cargo check -p perihelion-widgets --features markdown 2>&1 | tail -5`
  - 预期: 编译成功（不启用 markdown-highlight 时回退分支编译通过）

- [x] 为渲染集成编写单元测试（追加到 `perihelion-widgets/src/markdown/mod.rs` 的 `#[cfg(test)] mod tests` 块中）
  - 测试文件: `perihelion-widgets/src/markdown/mod.rs`（在现有 `#[cfg(test)] mod tests` 块末尾，`parse_markdown_respects_width` 测试之后）
  - 注意: 集成测试需标注 `#[cfg(feature = "markdown-highlight")]`，因为高亮功能仅在启用 feature 时可用
  - 新增测试:
    ```rust
    #[cfg(feature = "markdown-highlight")]
    #[test]
    fn parse_multiline_code_block_rust_highlight() {
        let text = parse_markdown("```rust\nfn main() {\n    println!(\"hello\");\n}\n```", &default_theme(), 80);
        // 3 行代码内容
        assert!(text.lines.len() >= 3, "多行代码块应至少产生 3 行");
        // 验证非单行模式：至少有一行包含 │ 前缀
        let has_prefix = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains('│')));
        assert!(has_prefix, "多行代码块应有 │ 前缀");
        // 验证语法高亮产生了多种颜色（不全是统一 text 颜色）
        let all_colors: std::collections::HashSet<_> = text.lines.iter()
            .flat_map(|l| l.spans.iter().filter_map(|s| s.style.fg))
            .collect();
        assert!(all_colors.len() > 1, "语法高亮应产生多种颜色，实际颜色数: {}", all_colors.len());
    }

    #[cfg(feature = "markdown-highlight")]
    #[test]
    fn parse_multiline_code_block_unknown_lang_fallback() {
        let text = parse_markdown("```unknown_lang_xyz\ncode here\nmore code\n```", &default_theme(), 80);
        assert!(text.lines.len() >= 2, "未识别语言仍应输出代码行");
        // 回退模式：每行应有 │ 前缀
        let has_prefix = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains('│')));
        assert!(has_prefix, "回退模式应有 │ 前缀");
        // 回退模式：所有代码文本使用统一 text 颜色
        let code_spans: Vec<_> = text.lines.iter()
            .flat_map(|l| l.spans.iter().filter(|s| !s.content.contains('│') && !s.content.trim().is_empty()))
            .collect();
        for span in &code_spans {
            assert_eq!(span.style.fg, Some(default_theme().text()), "回退模式代码应使用 text 颜色");
        }
    }

    #[cfg(feature = "markdown-highlight")]
    #[test]
    fn parse_multiline_code_block_no_lang_fallback() {
        let text = parse_markdown("```\ncode here\nmore code\n```", &default_theme(), 80);
        assert!(text.lines.len() >= 2, "省略语言标签仍应输出代码行");
        let has_prefix = text.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains('│')));
        assert!(has_prefix, "回退模式应有 │ 前缀");
    }
    ```
  - 运行命令: `cargo test -p perihelion-widgets --features markdown-highlight 2>&1 | tail -15`
  - 预期: 所有测试通过（包括新增 3 个和原有测试）
  - 同时验证不启用 feature 时原有测试仍通过: `cargo test -p perihelion-widgets --features markdown 2>&1 | tail -15`
  - 预期: 原有测试全部通过，新增 3 个测试在无 feature 时不编译（符合预期）

**检查步骤:**
- [x] 验证 render_state.rs 中新增了条件导入
  - `grep 'use super::highlight::highlight_code_block' perihelion-widgets/src/markdown/render_state.rs`
  - 预期: 输出 `#[cfg(feature = "markdown-highlight")]` 和 `use super::highlight::highlight_code_block;` 两行

- [x] 验证多行分支包含 cfg-gated 逻辑
  - `grep -c 'markdown-highlight' perihelion-widgets/src/markdown/render_state.rs`
  - 预期: 计数 >= 3（条件导入 1 处 + cfg 分支 2 处）

- [x] 验证启用 markdown-highlight 后全量编译通过
  - `cargo build -p perihelion-widgets --features markdown-highlight 2>&1 | tail -5`
  - 预期: 编译成功

- [x] 验证不启用 markdown-highlight 时编译通过且行为不变
  - `cargo test -p perihelion-widgets --features markdown 2>&1 | tail -10`
  - 预期: 原有全部测试通过

- [x] 验证集成测试通过
  - `cargo test -p perihelion-widgets --features markdown-highlight 2>&1 | tail -15`
  - 预期: 所有测试通过（原有 + 新增 3 个高亮集成测试 + Task 2 的 4 个 highlight 模块测试）

- [x] 验证 rust-agent-tui 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功（rust-agent-tui 已启用 markdown-highlight feature）

---

### Task 4: syntect 代码块语法高亮 验收

**前置条件:**
- Task 1-3 全部完成，`perihelion-widgets` 可通过 `--features markdown-highlight` 编译
- `rust-agent-tui` 已启用 `markdown-highlight` feature

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p perihelion-widgets --features markdown-highlight 2>&1 | tail -20`
   - 预期: 所有测试通过（原有 + highlight 模块 4 个 + 集成测试 3 个）
   - 失败排查: 检查 Task 2（highlight 模块测试）和 Task 3（集成测试）

2. 验证多行 ` ```rust ` 代码块显示多色语法高亮
   - `cargo test -p perihelion-widgets --features markdown-highlight -- parse_multiline_code_block_rust_highlight 2>&1 | tail -5`
   - 预期: 测试通过，验证了多种颜色输出
   - 失败排查: 检查 Task 2 的 `highlight_code_block` 函数和 Task 3 的 cfg-gated 分支

3. 验证未识别语言标签回退到统一颜色
   - `cargo test -p perihelion-widgets --features markdown-highlight -- parse_multiline_code_block_unknown_lang_fallback 2>&1 | tail -5`
   - 预期: 测试通过，验证代码文本使用 `theme.text()` 颜色
   - 失败排查: 检查 Task 3 的 fallback 分支逻辑

4. 验证省略语言标签回退到统一颜色
   - `cargo test -p perihelion-widgets --features markdown-highlight -- parse_multiline_code_block_no_lang_fallback 2>&1 | tail -5`
   - 预期: 测试通过
   - 失败排查: 检查 Task 2 的 `find_syntax_by_token("")` 返回 None

5. 验证单行代码块行为不变（统一 `code()` 颜色）
   - `cargo test -p perihelion-widgets --features markdown-highlight -- parse_code_block 2>&1 | tail -5`
   - 预期: 测试通过，单行代码块无 `│` 前缀
   - 失败排查: 检查 Task 3 是否误改了 `end == 1` 分支

6. 验证不启用 `markdown-highlight` feature 时编译通过且行为与改动前完全一致
   - `cargo test -p perihelion-widgets --features markdown 2>&1 | tail -10`
   - 预期: 原有全部测试通过（无 highlight 相关测试编译）
   - 失败排查: 检查 Task 3 的 `#[cfg(not(feature = "markdown-highlight"))]` 分支

7. 验证 `SyntaxSet` / `ThemeSet` 仅初始化一次（Lazy 语义）
   - `cargo test -p perihelion-widgets --features markdown-highlight -- highlight::tests 2>&1 | tail -5`
   - 预期: 所有 highlight 模块测试通过，`Lazy` 静态量通过编译保证单次初始化
   - 失败排查: 检查 Task 2 的 `Lazy::new` 声明

8. 验证多行注释/字符串等跨行语法结构正确高亮
   - `cargo test -p perihelion-widgets --features markdown-highlight -- highlight_multiline 2>&1 | tail -5`
   - 预期: `highlight_multiline` 测试通过（验证 3 行 Rust 代码完整高亮，HighlightLines 跨行状态正确）
   - 失败排查: 检查 Task 2 的 `HighlightLines` 逐行调用逻辑

9. 验证 rust-agent-tui 完整构建
   - `cargo build -p rust-agent-tui 2>&1 | tail -5`
   - 预期: 编译成功
   - 失败排查: 检查 Task 1 的 feature 配置和 Task 3 的集成
