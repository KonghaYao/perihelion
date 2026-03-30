# sticky-human-message-header 执行计划

**目标:** 在聊天区顶部新增 sticky header，始终显示最后一条 Human 消息

**技术栈:** ratatui 0.30，rust-agent-tui

**设计文档:** spec-design.md

---

### Task 1: AppCore 新增状态字段

**涉及文件:**
- 修改: `rust-agent-tui/src/app/core.rs`

**执行步骤:**
- [x] 在 `AppCore` struct 中新增 `last_human_message: Option<String>` 字段
  - 与 `pending_attachments` 字段相邻，保持语义一致性
  - `None` 表示无 Human 消息（header 高度为 0）

**检查步骤:**
- [x] 编译通过，无字段缺失错误
  - `cd rust-agent-tui && cargo build -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 无 error 输出

---

### Task 2: 状态更新——submit_message

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 在 `submit_message()` 函数中，用户消息推入 `view_messages` 后，更新 `self.core.last_human_message = Some(display)`
  - 插入位置：`self.core.view_messages.push(user_vm.clone())` 之后
  - 使用 `display`（含附件摘要）而非原始 `input`

**检查步骤:**
- [x] `submit_message` 后 `last_human_message` 有值
  - `grep -n "last_human_message" rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 至少 1 处赋值

---

### Task 3: 状态更新——/clear 和 open_thread

**涉及文件:**
- 修改: `rust-agent-tui/src/app/thread_ops.rs`
- 修改: `rust-agent-tui/src/app/core.rs`

**执行步骤:**
- [x] 在 `new_thread()` 函数末尾，添加 `self.core.last_human_message = None`
  - 插入位置：`let _ = self.core.render_tx.send(RenderEvent::Clear)` 之后
- [x] 在 `open_thread()` 函数中，加载历史消息后，从 `base_msgs` 找到最后一条 Human 消息，赋值给 `self.core.last_human_message`
  - 扫描方向：正向遍历，保留最后一个 `BaseMessage::Human` 的 Text 内容

**检查步骤:**
- [x] `/clear` 后 `last_human_message` 为 None
  - `grep -n "last_human_message" rust-agent-tui/src/app/thread_ops.rs`
  - 预期: `new_thread` 中有赋值 `None`，`open_thread` 中有赋值 `Some`
- [x] `open_thread` 正确提取 Human 消息文本
  - 逻辑确认：`base_msgs.iter().filter_map(|m| if let Human { content }| ...)` 模式

---

### Task 4: 新建 sticky_header.rs 渲染模块

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/main_ui/sticky_header.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 新建 `sticky_header.rs`，导出 `pub fn render_sticky_header(f: &mut Frame, app: &App, area: Rect)`
  - `area.height == 0` 时直接返回（guard）
  - 计算 `header_height`（1-3 行，超长截断）
  - 渲染格式：`"> "`（ACCENT+BOLD）+ 消息文本 + 底部分隔线
- [x] 在 `main_ui.rs` 顶部添加 `mod sticky_header;`
- [x] 实现行数估算函数：`fn estimate_header_lines(msg: &str, width: u16) -> usize`
  - `chars / width + 1`，clamp 到 `[1, 3]`
- [x] 实现截断函数：`fn wrap_message(msg: &str, max_chars: usize) -> String`
  - 优先在空格处截断，末尾加 `…`

**检查步骤:**
- [x] 新文件已创建且语法正确
  - `ls rust-agent-tui/src/ui/main_ui/sticky_header.rs`
  - 预期: 文件存在

---

### Task 5: main_ui.rs Layout 拆分与渲染集成

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 计算 `sticky_header_height`
  ```rust
  let sticky_header_height = app.core.last_human_message
      .as_ref()
      .map(|msg| estimate_header_lines(msg, chunks[0].width - 2).min(3) as u16)
      .unwrap_or(0);
  ```
- [x] 在 Layout constraints 中，将原来的 `Constraint::Min(3)` 拆分为两个约束：
  ```rust
  Constraint::Length(sticky_header_height),  // sticky header
  Constraint::Min(1),                          // scrollable messages
  ```
  插入位置：原 `Constraint::Min(3)` 替换为上述两个约束
- [x] 将 `render_messages(f, app, chunks[0])` 改为 `render_messages(f, app, header_area, messages_area)`
  - `header_area = chunks[0]`，`messages_area = chunks[1]`
- [x] 修改 `render_messages` 函数签名，增加 `header_area: Rect` 参数
  - 内部调用 `render_sticky_header(f, app, header_area)`

**检查步骤:**
- [x] Layout constraints 包含 sticky header 约束
  - `grep -n "sticky_header_height\|Length.*header" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 有 `Length(sticky_header_height)` 约束
- [x] 编译通过（零新增错误）
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -20`
  - 预期: 无 error（warning 可以）

---

### Task 6: sticky-human-message-header Acceptance

**Prerequisites:**
- Start command: `cargo run -p rust-agent-tui`

**End-to-end verification:**

1. **空消息时无 header**
   - `cargo test -p rust-agent-tui --lib -- test_sticky_header_hidden_when_no_messages`
   - Expected: test passes, no "> " in snapshot
   - On failure: 检查 Task 4 `area.height == 0` guard + Task 5 Layout `Length(sticky_header_height)`

2. **发送消息后显示 header**
   - `cargo test -p rust-agent-tui --lib -- test_sticky_header_shows_after_submit`
   - Expected: test passes, "> " + "hello from" visible
   - On failure: 检查 Task 2 `submit_message` 中 `last_human_message` 更新 + Task 5 `render_sticky_header` 调用

3. **滚动时 sticky 效果**
   - `cargo run -p rust-agent-tui` → 发送消息 → Agent 回复后滚动 → header 固定不动
   - Expected: header 始终可见，不随消息滚动
   - On failure: 检查 Task 5 Layout 中 sticky header 在 scrollable messages 上方

4. **连续发消息显示最后一条**
   - `cargo test -p rust-agent-tui --lib -- test_sticky_header_shows_last_message_not_first`
   - Expected: test passes, "second" visible, "first" absent
   - On failure: 检查 Task 2 `submit_message` 每次都覆盖更新

5. **/clear 后 header 消失**
   - `cargo test -p rust-agent-tui --lib -- test_sticky_header_hidden_after_clear`
   - Expected: test passes, " > " absent after new_thread
   - On failure: 检查 Task 3 `new_thread` 中 `last_human_message = None`

6. **打开历史 thread 恢复 header**
   - `cargo run -p rust-agent-tui` → 发送消息 → `/history` → 选择 thread → header 恢复
   - Expected: header 显示该 thread 最后一条 Human 消息
   - On failure: 检查 Task 3 `open_thread` 中 `last_human_message` 恢复逻辑

7. **长消息截断**
   - `cargo test -p rust-agent-tui --lib -- test_sticky_header_truncation_long_message`
   - Expected: test passes, long message wrapped and truncated
   - On failure: 检查 Task 4 `estimate_header_lines` clamp 到 3 + `wrap_message` 函数

