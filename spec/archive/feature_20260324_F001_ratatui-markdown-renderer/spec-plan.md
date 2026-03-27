# ratatui-markdown-renderer 执行计划

**目标:** 用 pulldown-cmark 替换占位实现，为 TUI 提供完整 Markdown 渲染能力

**技术栈:** Rust 2021、pulldown-cmark 0.12、ratatui ≥0.30、tokio（headless 测试）

**设计文档:** ./spec-design.md

---

### Task 1: 添加依赖与更新约束文档

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`
- 修改: `spec/global/constraints.md`

**执行步骤:**
- [x] 在 `rust-agent-tui/Cargo.toml` 的 `[dependencies]` 中追加 `pulldown-cmark = "0.12"`
- [x] 在 `spec/global/constraints.md` 的技术栈一节，将 `tui-markdown 0.3` 替换为 `pulldown-cmark 0.12`

**检查步骤:**
- [x] 验证依赖已写入
  - `grep 'pulldown-cmark' rust-agent-tui/Cargo.toml`
  - 预期: 输出 `pulldown-cmark = "0.12"`
- [x] 验证约束文档已更新
  - `grep 'pulldown-cmark' spec/global/constraints.md`
  - 预期: 输出包含 `pulldown-cmark 0.12`
- [x] 验证 tui-markdown 已从约束文档移除
  - `grep 'tui-markdown' spec/global/constraints.md`
  - 预期: 无输出（exit code 1）
- [x] 验证依赖可正常解析（不引入冲突）
  - `cargo fetch -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 无 error 输出

---

### Task 2: 实现 MarkdownRenderer

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/markdown.rs`

**执行步骤:**
- [x] 引入 pulldown-cmark，定义辅助类型
  - `use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind, HeadingLevel};`
  - 定义 `ListType` 枚举：`Ordered(u64)` / `Unordered`
  - 定义 `ListState` 结构体：`{ list_type: ListType }`
- [x] 定义 `RenderState` 结构体
  - 字段：`lines: Vec<Line<'static>>`、`current_spans: Vec<Span<'static>>`
  - 字段：`inline_style: Style`（累积行内样式，Strong/Emphasis/Strikethrough 叠加）
  - 字段：`list_stack: Vec<ListState>`（嵌套列表深度与类型）
  - 字段：`quote_depth: u32`（引用块嵌套深度）
  - 字段：`in_code_block: bool`、`code_block_lang: String`
  - 字段：`heading_level: Option<HeadingLevel>`（当前标题级别）
- [x] 实现 `flush_line(&mut self)` 方法
  - 将 `current_spans` 封装为 `Line`，push 到 `lines`
  - 清空 `current_spans`，重置 `heading_level`
  - 如 `current_spans` 为空则 push 空行（保留段落间距）
- [x] 实现 `push_span(&mut self, text: String, extra: Style)` 方法
  - 合并 `inline_style` 与 `extra`，构造 `Span::styled(text, merged)`
  - 追加到 `current_spans`
- [x] 实现 Block 事件处理
- [x] 实现 Inline 事件处理
- [x] 实现公共入口函数（保持接口不变）
- [x] 保留 `ensure_rendered(block: &mut ContentBlockView)` 不变（逻辑已正确，直接调用 `parse_markdown`）

**检查步骤:**
- [x] 验证编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Compiling rust-agent-tui` 且无 `error[E`
- [x] 验证接口签名未变
  - `grep -A1 'pub fn parse_markdown\|pub fn ensure_rendered' rust-agent-tui/src/ui/markdown.rs`
  - 预期: 两个函数签名与原 stub 相同

---

### Task 3: 编写 Headless 单元测试

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/headless.rs`

**执行步骤:**
- [x] 在 `headless.rs` 的 `#[cfg(test)] mod tests` 末尾新增 `mod markdown_tests`，引入 `parse_markdown`
- [x] 编写标题渲染测试 `test_md_heading`
- [x] 编写粗体/斜体/删除线测试 `test_md_inline_styles`
- [x] 编写行内代码测试 `test_md_inline_code`
- [x] 编写代码块测试 `test_md_code_block`
- [x] 编写无序列表测试 `test_md_unordered_list`
- [x] 编写有序列表测试 `test_md_ordered_list`
- [x] 编写引用块测试 `test_md_blockquote`
- [x] 编写水平线测试 `test_md_rule`
- [x] 编写容错测试 `test_md_incomplete_does_not_panic`

**检查步骤:**
- [x] 运行全部 markdown 测试
  - `cargo test -p rust-agent-tui markdown -- --nocapture 2>&1 | tail -20`
  - 预期: 输出 `test result: ok. 9 passed; 0 failed`
- [x] 运行全部 headless 测试（回归验证）
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 所有测试通过，无新增 FAILED（test_tool_call_message_visible_when_toggled 为 pre-existing 失败，与本 feature 无关）

---

### Task 4: ratatui-markdown-renderer Acceptance

**前置条件:**
- 构建命令: `cargo build -p rust-agent-tui`
- 全量测试: `cargo test -p rust-agent-tui`

**端到端验证：**

1. [x] 依赖已正确引入 — `pulldown-cmark v0.12.2` ✓
2. [x] 约束文档已同步更新 — `pulldown-cmark 0.12` 已写入，`tui-markdown` 已移除 ✓
3. [x] 接口签名不变 — `message_render.rs` / `message_view.rs` 未修改 ✓
4. [x] 编译全量通过 — `Finished dev profile` 无 error ✓
5. [x] 标题渲染正确 — `test_md_heading` ok ✓
6. [x] 粗体/斜体/删除线 Modifier 正确 — `test_md_inline_styles` ok ✓
7. [x] 代码块渲染正确 — `test_md_code_block` ok ✓
8. [x] 列表渲染正确 — `test_md_unordered_list` + `test_md_ordered_list` ok ✓
9. [x] 不完整 Markdown 不崩溃 — `test_md_incomplete_does_not_panic` ok ✓
