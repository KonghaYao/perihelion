# Feature: 20260324_F001 - ratatui-markdown-renderer

## 需求背景

`rust-agent-tui` 的消息渲染模块（`src/ui/markdown.rs`）目前仅有占位实现——`parse_markdown()` 直接返回原始文本，不做任何 Markdown 格式化。这导致 AI 回复中的标题、代码块、粗体等 Markdown 元素全部以源码形式显示，严重影响可读性。

`constraints.md` 曾登记 `tui-markdown 0.3` 作为备选，但该 crate 从未实际引入，且其灵活性不足以满足当前样式需求。因此决定选用 `pulldown-cmark` 作为解析层，自制 ratatui 渲染器。

## 目标

- 将 `parse_markdown(&str) -> Text<'static>` 替换为真正具备 Markdown 解析能力的实现
- 支持全部常用 Markdown 元素：标题、粗体/斜体/删除线、行内代码、代码块、无序/有序列表、引用块、水平分隔线、链接
- 保持与现有 `ContentBlockView::Text { raw, rendered, dirty }` 流式渲染机制兼容，支持实时重解析
- 公共接口不变，不修改 `message_render.rs` / `message_view.rs`

## 方案设计

### 解析库选择

使用 **`pulldown-cmark`**（CommonMark 规范，业界标准）作为 Markdown 解析层：

- 事件驱动（event-based），产出 `Event` 枚举流，便于逐事件映射为 ratatui 结构
- 容错解析：不完整标记（流式时的 `**未闭合`）自动降级为纯文本
- 极低运行时开销（1-3 μs/KB），对话消息 < 10 KB 时单次解析 < 30 μs

在 `rust-agent-tui/Cargo.toml` 中添加：

```toml
pulldown-cmark = "0.12"
```

> **约束变更**：`constraints.md` 中 `tui-markdown 0.3` 替换为 `pulldown-cmark 0.12`。

### 渲染器架构

![渲染器架构](./images/01-architecture.png)

渲染器完整封装在 `rust-agent-tui/src/ui/markdown.rs` 内，外部只暴露两个函数（接口不变）：

```rust
pub fn parse_markdown(input: &str) -> Text<'static>
pub fn ensure_rendered(block: &mut ContentBlockView)
```

内部结构：

```
parse_markdown(input)
    └─ pulldown_cmark::Parser::new_ext(input, Options::all())
         └─ MarkdownRenderer::render(events) -> Text<'static>
              ├─ RenderState
              │    ├─ lines: Vec<Line<'static>>       ← 已完成行
              │    ├─ current_spans: Vec<Span<'static>> ← 当前行内 Span
              │    ├─ inline_style: Style              ← 累积的行内样式
              │    ├─ list_stack: Vec<ListState>       ← 嵌套列表（深度+类型+编号）
              │    ├─ quote_depth: u32                 ← 引用嵌套深度
              │    └─ in_code_block: bool + lang: String
              └─ handle_event(Event)
                   ├─ Block 事件 → flush_line() + 设置块级前缀/样式
                   └─ Inline 事件 → push_span(text, style) 追加到 current_spans
```

### 视觉样式映射

![样式映射一览](./images/02-style-map.png)

| Markdown 元素 | 前缀 | ratatui 样式 |
|---|---|---|
| `# H1` | `━━ ` | BOLD + `Color::Cyan` |
| `## H2` | （无） | BOLD + `Color::Blue` |
| `### H3` | （无） | BOLD + `Color::Magenta` |
| `#### H4+` | （无） | BOLD + `Color::DarkGray` |
| `**bold**` | — | `Modifier::BOLD` |
| `*italic*` | — | `Modifier::ITALIC` |
| `~~strike~~` | — | `Modifier::CROSSED_OUT` |
| `` `code` `` | — | `Color::Yellow` + bg `Color::DarkGray` |
| ` ```lang ` | `│ `（每行） | `Color::Green`，首行输出 `[lang]` 标签 |
| `- item` | `  • ` | 白色，每嵌套一层多 2 空格缩进 |
| `1. item` | `  N.` | 白色，自动递增编号 |
| `> quote` | `▍ ` | `Color::DarkGray`，多层引用叠加前缀 |
| `---` | — | `────...────` 填满宽度，`Color::DarkGray` |
| `[text](url)` | — | `Color::Blue` + `Modifier::UNDERLINED` |

### 流式渲染策略

![流式渲染时序](./images/03-streaming-flow.png)

现有 `dirty` 标志机制完全保留，渲染策略如下：

```
流式 chunk 追加：
  append_chunk(chunk)
    raw.push_str(chunk)     // O(1)
    dirty = true            // 标记需重渲染

每帧渲染前（render_thread 调度）：
  ensure_rendered(block)
    if dirty:
      rendered = parse_markdown(&raw)   // pulldown-cmark 全量解析
      dirty = false

渲染时直接读取 rendered.lines（已解析好）
```

**容错处理**：流式输出中间态（如 `**未完成` 无闭合符）由 pulldown-cmark 自动宽容处理，将未识别标记当作纯文本渲染，不会崩溃或产生乱码。

**性能评估**：10 KB 文本全量重解析约 30 μs，TUI 帧率 60 fps 时每帧预算 16.7 ms，开销可接受。未来如需优化可引入防抖（每 50 ms 最多解析一次）。

## 实现要点

- **`pulldown_cmark::Options::all()`**：开启表格、脚注、删除线等扩展（方便日后支持）
- **列表嵌套**：用 `Vec<ListState>` 栈记录每层（类型 Ordered/Unordered + 当前编号），`List(start)` 事件 push，`End(List)` 事件 pop
- **代码块语言标签**：`CodeBlock(Fenced(lang))` 事件提取 lang，在代码块第一行渲染 `[lang]` 标签（DarkGray）
- **水平线宽度**：`parse_markdown` 收到终端宽度困难，固定生成 60 个 `─` 字符（后续可通过参数传入宽度）
- **`Text<'static>` 要求**：所有字符串在构建 Span/Line 时转为 `String`（owned），满足 `'static` 生命周期约束

## 约束一致性

- **架构约束**：改动仅在 `rust-agent-tui/src/ui/markdown.rs` 和 `Cargo.toml`，完全在应用层内，不修改核心 crate（符合禁止下层依赖上层原则）
- **依赖约束变更**：`tui-markdown 0.3` → `pulldown-cmark 0.12`，需同步更新 `spec/global/constraints.md`
- **接口稳定性**：`parse_markdown` 和 `ensure_rendered` 签名不变，`message_render.rs` / `message_view.rs` 零改动

## 验收标准

- [ ] `# H1` / `## H2` / `### H3` 正确渲染为对应颜色 + 粗体
- [ ] `**粗体**` / `*斜体*` / `~~删除线~~` 应用正确 Modifier
- [ ] `` `行内代码` `` 以黄色 + 深色背景显示
- [ ] ` ```rust\n...\n``` ` 代码块每行以 `│ ` 前缀 + 绿色显示，首行显示 `[rust]`
- [ ] `- 无序` / `1. 有序` 列表渲染正确前缀，嵌套列表增加缩进
- [ ] `> 引用` 显示 `▍ ` 前缀 + DarkGray
- [ ] `---` 渲染为 DarkGray 水平线
- [ ] 流式输出中间态（不完整 Markdown）不崩溃，降级为纯文本显示
- [ ] Headless 测试覆盖上述主要元素
