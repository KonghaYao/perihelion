# TUI Bug Fixes 执行计划

**目标:** 修复弹窗内容超长溢出、粘贴换行符触发提交、loading 状态输入框锁死三个 TUI bug

**技术栈:** Rust, ratatui, crossterm, ratatui-textarea

**设计文档:** spec-design.md

---

### Task 1: 弹窗/面板滚动支持

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/model_panel.rs`
- 修改: `rust-agent-tui/src/app/agent_panel.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**

- [x] 在 `AskUserBatchPrompt` 结构体中新增 `scroll_offset: u16` 字段，默认 0
  - 修改 `from_request` 初始化 `scroll_offset: 0`
- [x] 在 `ModelPanel` 结构体中新增 `scroll_offset: u16` 字段，默认 0
- [x] 在 `AgentPanel` 结构体中新增 `scroll_offset: u16` 字段，默认 0
- [x] 实现 `ensure_cursor_visible` 辅助函数
  - 接收 `(cursor_row: u16, scroll_offset: u16, visible_height: u16) -> u16`
  - 光标在视口上方：返回 cursor_row
  - 光标在视口下方：返回 cursor_row - visible_height + 1
  - 光标在视口内：返回原 scroll_offset
- [x] 修改 `render_ask_user_popup`：popup_height 增加 `.min(area.height * 4 / 5)` 上限；content_area 的 Paragraph 使用 `.scroll((prompt.scroll_offset, 0))`
  - 在 `ask_user_move` 操作后调用 `ensure_cursor_visible` 更新 scroll_offset
- [x] 修改 `render_model_panel`：popup_height 增加 `.min(area.height * 4 / 5)` 上限；Browse 模式下内容超出时使用 Paragraph::scroll
- [x] 修改 `render_agent_panel`：popup_height 增加 `.min(area.height * 4 / 5)` 上限；列表超出时使用 Paragraph::scroll + 光标跟随
- [x] 修改 `render_thread_browser`：popup_height 增加 `.min(area.height * 4 / 5)` 上限；列表超出时使用 Paragraph::scroll + 光标跟随

**检查步骤:**

- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出 `Finished` 无 error
- [x] 确认 scroll_offset 字段存在
  - `grep -n 'scroll_offset' rust-agent-tui/src/app/mod.rs rust-agent-tui/src/app/model_panel.rs rust-agent-tui/src/app/agent_panel.rs`
  - 预期: 每个文件中至少有 1 处 scroll_offset 定义
- [x] 确认 ensure_cursor_visible 函数存在
  - `grep -n 'fn ensure_cursor_visible' rust-agent-tui/src/app/mod.rs`
  - 预期: 找到该函数定义
- [x] 确认所有弹窗的 popup_height 有 80% 上限
  - `grep -n 'area.height \* 4 / 5' rust-agent-tui/src/ui/main_ui.rs`
  - 预期: render_ask_user_popup, render_model_panel, render_agent_panel, render_thread_browser 各出现 1 次

---

### Task 2: Bracketed Paste Mode

**涉及文件:**
- 修改: `rust-agent-tui/src/main.rs`
- 修改: `rust-agent-tui/src/event.rs`

**执行步骤:**

- [x] 在 `main.rs` 终端初始化中添加 `EnableBracketedPaste`
  - 在 `execute!(stdout, EnterAlternateScreen, EnableMouseCapture)` 后追加 `EnableBracketedPaste`
  - 导入 `crossterm::event::{EnableBracketedPaste, DisableBracketedPaste}`
- [x] 在 `main.rs` 终端恢复中添加 `DisableBracketedPaste`
  - 在 `execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)` 后追加 `DisableBracketedPaste`
- [x] 在 `event.rs` 的 `match ev` 中新增 `Event::Paste(text)` 分支
  - 粘贴文本直接 `app.textarea.insert_str(&text)` 插入 textarea（保留换行）
  - 不区分 loading/非 loading 状态，一律插入（配合 Task 3 的输入缓冲）

**检查步骤:**

- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出 `Finished` 无 error
- [x] 确认 EnableBracketedPaste 在初始化中
  - `grep -n 'EnableBracketedPaste' rust-agent-tui/src/main.rs`
  - 预期: 至少 2 处（Enable + Disable）
- [x] 确认 Paste 事件处理存在
  - `grep -n 'Event::Paste' rust-agent-tui/src/event.rs`
  - 预期: 至少 1 处匹配

---

### Task 3: Loading 输入缓冲

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/event.rs`

**执行步骤:**

- [x] 在 `App` 结构体中新增 `pending_messages: Vec<String>` 字段，默认空
- [x] 修改 `build_textarea` 函数签名：新增 `buffered_count: usize` 参数
  - `disabled=true && buffered_count > 0` 时，标题显示 `" 处理中… (已缓存 N 条) "`
  - 所有调用点更新（`build_textarea(false)` → `build_textarea(false, 0)`，`build_textarea(true)` → `build_textarea(true, self.pending_messages.len())`）
- [x] 修改 `set_loading`：loading=true 时调用 `build_textarea(true, self.pending_messages.len())` 但不禁用输入（边框变黄但可编辑）
- [x] 修改 `event.rs` 中 Enter 提交逻辑：
  - 去掉 Enter 分支的 `!app.loading` guard
  - loading 时 Enter：提取文本 → `app.pending_messages.push(text)` → 重建 textarea 带缓冲计数 → 返回 `Action::Redraw`（不返回 `Action::Submit`）
  - 非 loading 时 Enter：保持原有 `Action::Submit` 行为
- [x] 修改 `event.rs`：去掉普通字符输入、Alt+Enter、Tab、Esc 等的 `!app.loading` guard，允许 loading 时编辑输入框
  - 注意：Esc 在 loading 时不应退出程序（保留 `!app.loading` guard 或改为 noop）
- [x] 在 `handle_agent_event` 的 `Done` 和 `Error` 分支中，`set_loading(false)` 之后检查 `pending_messages`
  - 若非空：`let combined = self.pending_messages.join("\n\n"); self.pending_messages.clear(); self.submit_message(combined);`
- [x] 在 `App::new()` 和 headless `new_headless` 中初始化 `pending_messages: Vec::new()`

**检查步骤:**

- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出 `Finished` 无 error
- [x] 确认 pending_messages 字段存在
  - `grep -n 'pending_messages' rust-agent-tui/src/app/mod.rs`
  - 预期: 至少 3 处（声明、初始化、使用）
- [x] 确认缓冲标题逻辑
  - `grep -n '已缓存' rust-agent-tui/src/app/mod.rs`
  - 预期: 1 处匹配（build_textarea 函数中）
- [x] 确认 Done 分支的自动发送逻辑
  - `grep -A15 'AgentEvent::Done' rust-agent-tui/src/app/mod.rs | grep -c 'pending_messages'`
  - 预期: 至少 1

---

### Task 4: TUI Bug Fixes Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 确保已配置 API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）

**End-to-end verification:**

1. [x] 弹窗滚动：触发一个会生成多选项的 AskUser 弹窗，验证内容超过屏幕时可通过 ↑↓ 滚动
   - `cargo build -p rust-agent-tui 2>&1 | tail -3`
   - Expected: 编译成功；运行时 AskUser 弹窗高度不超过屏幕 80%，可滚动
   - On failure: check Task 1 [弹窗/面板滚动支持]

2. [x] 粘贴换行：在输入框中粘贴含换行符的多行文本
   - `grep -c 'Event::Paste' rust-agent-tui/src/event.rs`
   - Expected: 粘贴后文本完整保留在输入框内，不触发提交；grep 结果 >= 1
   - On failure: check Task 2 [Bracketed Paste Mode]

3. [x] Loading 缓冲：Agent 运行中在输入框键入内容并按 Enter
   - `grep -c 'pending_messages' rust-agent-tui/src/app/mod.rs`
   - Expected: 消息进入缓冲区，输入框标题显示 "已缓存 N 条"；Agent 完成后自动合并发送；grep 结果 >= 3
   - On failure: check Task 3 [Loading 输入缓冲]

4. [x] 全量测试通过
   - `cargo test -p rust-agent-tui 2>&1 | tail -5`
   - Expected: 所有测试通过（40 passed），无 panic
   - On failure: check all Tasks
