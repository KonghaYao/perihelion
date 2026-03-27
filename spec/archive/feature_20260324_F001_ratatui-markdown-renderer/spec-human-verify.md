# ratatui-markdown-renderer 人工验收清单

**生成时间:** 2026-03-24
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链已安装: `rustc --version`
- [ ] [AUTO] 检查 cargo 可用: `cargo --version`
- [ ] [AUTO] 确认工作目录为项目根: `test -f Cargo.toml && echo OK`
- [ ] [AUTO] 全量编译通过: `cargo build -p rust-agent-tui 2>&1 | tail -3`

### 测试数据准备

无需额外测试数据，所有测试均为单元测试，由 cargo test 驱动。

---

## 验收项目

### 场景 1：依赖配置与约束文档同步

#### - [x] 1.1 pulldown-cmark 依赖已正确写入 Cargo.toml

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep 'pulldown-cmark' rust-agent-tui/Cargo.toml` → 期望: 输出 `pulldown-cmark = "0.12"`
  2. [A] `cargo tree -p rust-agent-tui 2>/dev/null | grep pulldown` → 期望: 输出含 `pulldown-cmark v0.12`
- **异常排查:**
  - 如果 grep 无输出: 检查 `rust-agent-tui/Cargo.toml` 中 `[dependencies]` 是否包含 `pulldown-cmark = "0.12"`
  - 如果 cargo tree 报错: 运行 `cargo fetch` 确保依赖已下载

#### - [x] 1.2 spec/global/constraints.md 约束文档已同步更新

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep 'pulldown-cmark' spec/global/constraints.md` → 期望: 输出含 `pulldown-cmark 0.12`
  2. [A] `grep 'tui-markdown' spec/global/constraints.md; echo "exit:$?"` → 期望: 输出 `exit:1`（未找到，说明已移除）
- **异常排查:**
  - 如果 tui-markdown 仍存在: 手动编辑 `spec/global/constraints.md`，将 `tui-markdown 0.3` 替换为 `pulldown-cmark 0.12`

---

### 场景 2：编译稳定性与接口不变性

#### - [x] 2.1 rust-agent-tui 编译无错误

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E '^error\[' | head -5` → 期望: 无输出（无编译错误）
- **异常排查:**
  - 如果出现编译错误: 检查 `rust-agent-tui/src/ui/markdown.rs` 中 pulldown-cmark API 用法是否与 0.12 版本匹配

#### - [x] 2.2 parse_markdown / ensure_rendered 接口签名不变

- **来源:** Task 2 检查步骤 + 设计文档约束一致性
- **操作步骤:**
  1. [A] `grep 'pub fn parse_markdown' rust-agent-tui/src/ui/markdown.rs` → 期望: 输出 `pub fn parse_markdown(input: &str) -> Text<'static>`
- **异常排查:**
  - 如果签名不符: 恢复 `parse_markdown(input: &str) -> Text<'static>` 签名，接口是公开契约，不可更改

#### - [x] 2.3 message_render.rs 和 message_view.rs 未被意外修改

- **来源:** Task 4 端到端验证场景 3
- **操作步骤:**
  1. [A] `git diff --name-only | grep -E 'message_render|message_view'; echo "exit:$?"` → 期望: 第一行无文件名输出，`exit:1`
- **异常排查:**
  - 如果出现文件名: 运行 `git diff rust-agent-tui/src/ui/message_render.rs` 检查是否有意外改动，必要时 `git checkout` 还原

---

### 场景 3：行内样式渲染

#### - [x] 3.1 标题（H1/H2/H3/H4+）颜色与粗体正确

- **来源:** Task 3 test_md_heading + 设计文档视觉样式映射
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_heading 2>&1 | grep -E 'ok|FAILED'` → 期望: 输出含 `test_md_heading ... ok`
- **异常排查:**
  - 如果测试失败: 运行 `cargo test -p rust-agent-tui test_md_heading -- --nocapture` 查看断言失败详情
  - 检查 `markdown.rs` 中 `Tag::Heading { level, .. }` 的 H1 处理：颜色应为 `Color::Cyan`，前缀应为 `━━ `

#### - [x] 3.2 粗体 / 斜体 / 删除线 Modifier 正确

- **来源:** Task 3 test_md_inline_styles + 设计文档
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_inline_styles 2>&1 | grep -E 'ok|FAILED'` → 期望: 输出含 `test_md_inline_styles ... ok`
- **异常排查:**
  - BOLD: 检查 `Tag::Strong` → `inline_style.add_modifier(Modifier::BOLD)`
  - ITALIC: 检查 `Tag::Emphasis` → `inline_style.add_modifier(Modifier::ITALIC)`
  - CROSSED_OUT: 检查 `Tag::Strikethrough` → `inline_style.add_modifier(Modifier::CROSSED_OUT)`

#### - [x] 3.3 行内代码 Yellow 颜色 + DarkGray 背景

- **来源:** Task 3 test_md_inline_code + 设计文档
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_inline_code 2>&1 | grep -E 'ok|FAILED'` → 期望: 输出含 `test_md_inline_code ... ok`
- **异常排查:**
  - 检查 `Event::Code(text)` 处理中是否使用 `Style::default().fg(Color::Yellow).bg(Color::DarkGray)`

---

### 场景 4：块级元素渲染

#### - [x] 4.1 代码块含 [lang] 语言标签 + │ 行前缀

- **来源:** Task 3 test_md_code_block + 设计文档
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_code_block 2>&1 | grep -E 'ok|FAILED'` → 期望: 输出含 `test_md_code_block ... ok`
- **异常排查:**
  - `[lang]` 标签：检查 `Start(Tag::CodeBlock(Fenced(lang)))` 中 lang 非空时的首行处理
  - `│ ` 前缀：检查 `Event::Text` 在 `in_code_block=true` 时的按换行分割逻辑

#### - [x] 4.2 无序列表 • 前缀 + 有序列表自动递增编号

- **来源:** Task 3 test_md_unordered_list + test_md_ordered_list + 设计文档
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_unordered_list 2>&1 | grep -E 'ok|FAILED'` → 期望: `test_md_unordered_list ... ok`
  2. [A] `cargo test -p rust-agent-tui test_md_ordered_list 2>&1 | grep -E 'ok|FAILED'` → 期望: `test_md_ordered_list ... ok`
- **异常排查:**
  - 无序列表：检查 `Start(Tag::Item)` 对 `ListType::Unordered` 的处理，确认 `• ` 拼接了缩进
  - 有序列表：检查 `ListType::Ordered(n)` 自增逻辑，`*n += 1`

#### - [x] 4.3 引用块 ▍ 前缀 + 水平线 ─ 填充

- **来源:** Task 3 test_md_blockquote + test_md_rule + 设计文档
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_blockquote 2>&1 | grep -E 'ok|FAILED'` → 期望: `test_md_blockquote ... ok`
  2. [A] `cargo test -p rust-agent-tui test_md_rule 2>&1 | grep -E 'ok|FAILED'` → 期望: `test_md_rule ... ok`
- **异常排查:**
  - 引用块：检查 `flush_line()` 中 `quote_depth > 0` 时向 spans 首部插入 `▍ ` 的逻辑
  - 水平线：检查 `Event::Rule` 生成 `"─".repeat(60)` Span

---

### 场景 5：容错处理与全量测试

#### - [x] 5.1 不完整 Markdown 不崩溃并降级为纯文本

- **来源:** Task 3 test_md_incomplete_does_not_panic + 设计文档容错处理
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_md_incomplete_does_not_panic 2>&1 | grep -E 'ok|FAILED'` → 期望: `test_md_incomplete_does_not_panic ... ok`
- **异常排查:**
  - 如果 panic：检查 pulldown-cmark 的 `Options::all()` 是否正确开启容错解析

#### - [x] 5.2 全量 markdown 单元测试 9/9 通过

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui markdown_tests 2>&1 | grep 'test result'` → 期望: 输出 `test result: ok. 9 passed; 0 failed`
- **异常排查:**
  - 如果有失败：运行 `cargo test -p rust-agent-tui markdown_tests -- --nocapture` 查看具体失败信息
  - 注意：`test_tool_call_message_visible_when_toggled` 为 pre-existing 失败，与本 feature 无关，不计入

#### - [x] 5.3 TUI 实机 Markdown 视觉效果验证

- **来源:** 设计文档视觉样式映射（人工验证补充）
- **操作步骤:**
  1. [A] `ANTHROPIC_API_KEY=test cargo run -p rust-agent-tui 2>&1 &; sleep 3; kill %1 2>/dev/null; echo "启动测试完成"` → 期望: 进程正常启动后退出，无 panic
  2. [H] 运行 `cargo run -p rust-agent-tui`（需配置 `.env` 或环境变量），在输入框中发送包含以下内容的消息后确认：
     ```
     # 标题测试
     **粗体** *斜体* ~~删除线~~ `行内代码`
     ```rust
     fn main() {}
     ```
     - 列表项 1
     - 列表项 2
     > 引用文本
     ---
     ```
     观察消息区域，H1 标题是否显示为 `━━ 标题测试`（青色粗体）？ → 是/否
  3. [H] 继续观察同一条消息，粗体文字是否加粗显示、行内代码是否有黄色高亮、代码块是否有 `│ ` 前缀绿色显示？ → 是/否
  4. [H] 观察列表项是否有 `•` 前缀，引用是否有 `▍` 前缀，`---` 是否渲染为一行横线？ → 是/否
- **异常排查:**
  - 如果 TUI 无法启动：检查 `.env` 文件或 `ANTHROPIC_API_KEY` 环境变量是否配置
  - 如果样式未生效：确认 `message_render.rs` 调用了 `ensure_rendered()`，检查 `dirty` flag 是否正确设置

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | pulldown-cmark 依赖写入 | 2 | 0 | ✅ | |
| 场景 1 | 1.2 | constraints.md 约束同步 | 2 | 0 | ✅ | |
| 场景 2 | 2.1 | 编译无错误 | 1 | 0 | ✅ | |
| 场景 2 | 2.2 | 接口签名不变 | 1 | 0 | ✅ | |
| 场景 2 | 2.3 | 相邻文件未被修改 | 1 | 0 | ✅ | |
| 场景 3 | 3.1 | 标题颜色与粗体 | 1 | 0 | ✅ | |
| 场景 3 | 3.2 | 粗体/斜体/删除线 Modifier | 1 | 0 | ✅ | |
| 场景 3 | 3.3 | 行内代码样式 | 1 | 0 | ✅ | |
| 场景 4 | 4.1 | 代码块标签+前缀 | 1 | 0 | ✅ | |
| 场景 4 | 4.2 | 无序/有序列表前缀 | 2 | 0 | ✅ | |
| 场景 4 | 4.3 | 引用块+水平线 | 2 | 0 | ✅ | |
| 场景 5 | 5.1 | 容错不崩溃 | 1 | 0 | ✅ | |
| 场景 5 | 5.2 | 全量测试 9/9 | 1 | 0 | ✅ | |
| 场景 5 | 5.3 | TUI 实机视觉效果 | 1 | 3 | ✅ | 可选 |

**验收结论:** ✅ 全部通过
