# Feature: 20260429_F001 - syntect-codeblock-highlight

## 需求背景

当前 `perihelion-widgets` 的 Markdown 渲染器对多行代码块使用统一的 `theme.text()` 颜色输出，缺乏语法高亮。代码块的语言标签（如 `rust`、`python`）已被 `render_state.rs` 捕获到 `code_block_lang` 字段，但未用于差异化渲染。需要引入 syntect 库实现基于语言的语法着色。

## 目标

- 多行代码块根据语言标签自动进行语法高亮，输出多色 ratatui `Span`
- 未识别语言时回退到当前统一颜色行为，不降级
- syntect 作为可选依赖，通过 feature flag 控制，不影响不需要高亮的场景
- `SyntaxSet` / `ThemeSet` 延迟初始化，避免重复加载开销

## 方案设计

### Feature Flag 结构

```toml
[features]
default = []
markdown = ["pulldown-cmark"]
markdown-highlight = ["markdown", "dep:syntect"]
```

`markdown-highlight` 依赖 `markdown` 和 `syntect`。启用 `markdown` 但不启用 `markdown-highlight` 时行为与当前完全一致。

### 依赖声明

```toml
[dependencies]
syntect = { version = "5", default-features = false, features = ["default-fancy"], optional = true }
```

使用 `default-fancy`（纯 Rust `fancy-regex`），避免 C 库 oniguruma 编译问题。内建默认语法集（`default-syntaxes` 包含在 `default-fancy` 中）。

### 高亮引擎初始化

使用 `once_cell::sync::Lazy` 延迟初始化 `SyntaxSet` 和 `ThemeSet`：

```rust
#[cfg(feature = "markdown-highlight")]
mod highlight {
    use once_cell::sync::Lazy;
    use syntect::highlighting::{ThemeSet, Theme};
    use syntect::parsing::SyntaxSet;

    pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
    pub static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

    pub fn default_theme() -> &'static Theme {
        &THEME_SET.themes["base16-ocean.dark"]
    }
}
```

`SyntaxSet::load_defaults_newlines()` 加载约 23ms（含内建几十种语言定义），`ThemeSet::load_defaults()` 加载内建主题。两者均为线程安全，`Lazy` 保证只初始化一次。

### 高亮集成点

修改 `render_state.rs` 中 `Event::End(TagEnd::CodeBlock)` 的多行分支（当前代码 557-569 行）：

**现有逻辑（保留为 fallback）：**
```rust
} else if end > 1 {
    for line_text in &lines[..end] {
        // 统一 theme.text() 颜色
    }
}
```

**新增逻辑（`markdown-highlight` feature 启用时）：**
```rust
#[cfg(feature = "markdown-highlight")]
{
    // 尝试用 syntect 高亮
    if let Some(highlighted) = highlight_code_block(&self.code_block_lang, &lines[..end]) {
        self.lines.extend(highlighted);
    } else {
        // 回退到统一颜色
        fallback_render(...)
    }
}

#[cfg(not(feature = "markdown-highlight"))]
{
    // 当前统一颜色逻辑
}
```

### 高亮函数

```rust
#[cfg(feature = "markdown-highlight")]
fn highlight_code_block(lang: &str, lines: &[String]) -> Option<Vec<Line<'static>>> {
    let ss = &*highlight::SYNTAX_SET;
    let syntax = ss.find_syntax_by_token(lang)?;
    let theme = highlight::default_theme();
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut result = Vec::new();
    for line_text in lines {
        let mut spans = Vec::new();
        // │ 前缀
        spans.push(Span::styled("│ ".to_string(), Style::default().fg(theme.muted())));

        // syntect 高亮
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

- `find_syntax_by_token(lang)` 按 fencetag 查找语言定义，未找到返回 `None` 触发 fallback
- `HighlightLines::new()` 创建逐行高亮器，正确处理跨行语法（如多行字符串、注释）
- `highlight_line()` 返回 `Vec<(Style, &str)>`，每个片段映射为 ratatui `Span`
- syntect `Style.foreground` 是 `(r, g, b, a)` 格式，直接转 `Color::Rgb`

### MarkdownTheme 不变

`MarkdownTheme` trait 的 `code()` / `text()` 方法仍用于行内代码和 fallback 场景。syntect 高亮使用自己的颜色体系（来自主题文件），无需与 `MarkdownTheme` 映射。

当前默认主题 `base16-ocean.dark` 与 TUI 暗色背景视觉协调。未来如需主题切换，可在 `MarkdownTheme` 中新增 `syntect_theme_name() -> &str` 方法。

### 单行代码块

单行代码块保持当前行为（统一 `theme.code()` 颜色），不做语法高亮。理由：单行通常是简短表达式，高亮收益低，且视觉上与行内代码风格一致更协调。

## 实现要点

1. **跨行语法正确性：** `HighlightLines` 逐行调用时内部维护解析状态，多行注释/字符串等跨行结构可正确高亮
2. **性能：** `SyntaxSet` 加载约 23ms（仅首次），之后高亮本身约 16000 行/秒，TUI 场景下代码块通常不超过百行，无性能瓶颈
3. **二进制体积：** `syntect` + `default-fancy` + 内建语法集约增加 1-2 MB
4. **依赖隔离：** `once_cell` 已在 workspace 间接依赖中存在，无需额外引入

## 约束一致性

- 符合 workspace 分层约束：改动仅在 `perihelion-widgets`（独立 widget 库），无跨层依赖
- 符合 feature flag 模式：新功能通过可选 feature 启用，不影响默认构建
- 符合异步优先原则：syntect 为纯 CPU 操作，无 IO，不涉及异步

## 验收标准

- [ ] 启用 `markdown-highlight` feature 后，多行 ` ```rust ` 代码块显示多色语法高亮
- [ ] 未识别的语言标签（如 ` ```unknown_lang `）回退到统一颜色
- [ ] 省略语言标签的代码块（` ``` `）回退到统一颜色
- [ ] 单行代码块行为不变（统一 `code()` 颜色）
- [ ] 不启用 `markdown-highlight` feature 时编译通过，行为与改动前完全一致
- [ ] `SyntaxSet` / `ThemeSet` 仅初始化一次
- [ ] 多行注释/字符串等跨行语法结构正确高亮
