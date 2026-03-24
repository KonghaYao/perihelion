# TUI 剪贴板粘贴图片 执行计划

**目标:** 允许用户通过 Ctrl+V 将剪贴板图片附加到 Human 消息，以 base64 PNG 形式发送给 LLM

**技术栈:** arboard 3、png 0.17、base64 0.22、ratatui、rust-create-agent ContentBlock

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: 依赖与数据结构

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 在 `rust-agent-tui/Cargo.toml` 的 `[dependencies]` 中添加三个 crate
  - `arboard = "3"` — 跨平台剪贴板访问
  - `png = "0.17"` — RGBA bytes → PNG 编码
  - `base64 = { version = "0.22", features = ["alloc"] }` — PNG 二进制 → base64 字符串
- [x] 在 `app/mod.rs` 顶部（`HitlBatchPrompt` 之前）定义 `PendingAttachment` 结构体
  - 字段：`label: String`（如 "clipboard_1.png"）、`media_type: String`（固定 "image/png"）、`base64_data: String`、`size_bytes: usize`
- [x] 在 `App` 结构体中添加字段 `pub pending_attachments: Vec<PendingAttachment>`
- [x] 在 `App::new()` 中初始化 `pending_attachments: Vec::new()`
- [x] 在 `App::new_headless()` 中同样初始化该字段
- [x] 在 `App::new_thread()` 中清空：`self.pending_attachments.clear()`
- [x] 在 `App` impl 中添加两个辅助方法：
  - `pub fn add_pending_attachment(&mut self, att: PendingAttachment)` — push 到列表
  - `pub fn pop_pending_attachment(&mut self)` — 删除最后一个元素（`self.pending_attachments.pop()`）

**检查步骤:**
- [x] 编译通过，无 unused import 警告
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出（无编译错误）
- [x] 字段初始化正确（App::new() 和 new_headless() 均包含 pending_attachments）
  - `grep -n "pending_attachments" rust-agent-tui/src/app/mod.rs | head -20`
  - 预期: 出现 struct 定义、字段声明、new() 初始化、new_headless() 初始化、new_thread() 清空 共 5 处以上

---

### Task 2: 剪贴板读取与事件拦截

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`

**执行步骤:**
- [x] 在 `event.rs` 顶部添加辅助函数 `rgba_to_png_base64(width: u32, height: u32, rgba_bytes: &[u8]) -> Result<(String, usize)>`
  - 使用 `png::Encoder` 将 RGBA bytes 写入 `Vec<u8>`：设置宽高、`ColorType::Rgba`、`BitDepth::Eight`
  - 用 `base64::engine::general_purpose::STANDARD.encode(&png_bytes)` 编码
  - 返回 `(base64_str, png_bytes.len())`
- [x] 在 `Event::Key` 处理的通用输入 `match input { ... }` 分支中，在现有 `Ctrl+C` 处理**之后**、`Esc` 处理**之后**，其他按键处理**之前**，插入 Ctrl+V 拦截分支
- [x] 在通用 `match input` 中，在 `input if input.key != Key::Enter =>` 分支**之前**，添加 Del 键处理
- [x] 在文件顶部添加必要 import：`use crate::app::PendingAttachment;` 以及 `use base64::Engine as _;`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] Ctrl+V 拦截逻辑存在于 event.rs
  - `grep -n "Char('v').*ctrl\|ctrl.*Char('v')" rust-agent-tui/src/event.rs`
  - 预期: 找到至少一处匹配
- [x] rgba_to_png_base64 函数存在
  - `grep -n "rgba_to_png_base64" rust-agent-tui/src/event.rs`
  - 预期: 找到函数定义和调用两处

---

### Task 3: 附件栏 UI 渲染

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 在 `render()` 函数中计算 `attachment_height`（类似 `todo_height`）
- [x] 在 `Layout::default().constraints([...])` 中插入新 slot [3] 附件栏
- [x] 更新所有使用 `chunks[3]`、`chunks[4]` 的地方改为 `chunks[4]`、`chunks[5]`
- [x] 在 `render_todo_panel` 之后添加 `render_attachment_bar(f, app, chunks[3]);`
- [x] 实现 `render_attachment_bar` 函数：height==0 返回；渲染 Blue 边框 Block；内容两行（附件标签 + Del 提示）

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] attachment_height 变量存在
  - `grep -n "attachment_height" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 至少 2 处（定义 + Constraint 使用）
- [x] render_attachment_bar 函数存在
  - `grep -n "render_attachment_bar" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 找到函数定义和调用两处
- [x] Layout constraints 数量正确（原 5 个改为 6 个）
  - `grep -A 8 "Constraint::Length(1).*标题" rust-agent-tui/src/ui/main_ui.rs | grep -c "Constraint"`
  - 预期: 输出 6

---

### Task 4: 多模态消息提交

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/ui/message_view.rs`

**执行步骤:**
- [x] 修改 `app/mod.rs` 中的 `submit_message` 函数：消费附件、构建多模态 AgentInput 和 user_msg、传 agent_input
- [x] 修改 `submit_message` 中 MessageViewModel::user() 调用：有附件时追加 `[🖼 N 张图片]` 摘要
- [x] 修改 `app/agent.rs` 中 `run_universal_agent` 签名：`input: String` → `input: AgentInput`；移除内部 AgentInput::text() 构建
- [x] 确保 `user_msg` 持久化 `store.append_messages` 在 `tokio::spawn` 中执行

**检查步骤:**
- [x] 编译通过（无 unused variable 警告）
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error|^warning.*unused"`
  - 预期: 无 error，no unused 相关警告
- [x] run_universal_agent 签名已更新为 AgentInput
  - `grep -n "fn run_universal_agent" rust-agent-tui/src/app/agent.rs`
  - 预期: 输出包含 `input: AgentInput` 而非 `input: String`
- [x] submit_message 中 attachments 消费逻辑存在
  - `grep -n "pending_attachments\|ContentBlock::image_base64\|AgentInput::blocks" rust-agent-tui/src/app/mod.rs | head -15`
  - 预期: 三者均出现
- [x] 全量测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

---

### Task 5: TUI 剪贴板粘贴图片 Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 测试前置: 终端已启动，剪贴板中准备好一张图片（截图或复制图片）
- 确认 API Key 已配置（Anthropic 或 OpenAI vision 支持的模型）

**端到端验证:**

1. [x] **剪贴板有图片时 Ctrl+V 拦截逻辑存在**
   - `grep -c "add_pending_attachment\|PendingAttachment" rust-agent-tui/src/event.rs`
   - 预期: 输出 ≥ 2 ✅

2. [x] **剪贴板无图片时 fallback get_text 逻辑存在**
   - `grep -n "get_image\|get_text" rust-agent-tui/src/event.rs`
   - 预期: get_image 先行，失败后 fallback get_text ✅

3. [x] **Del 键删除逻辑存在**
   - `grep -n "Delete\|pop_pending" rust-agent-tui/src/event.rs`
   - 预期: Key::Delete → pop_pending_attachment ✅

4. [x] **无附件时布局不占空间**
   - `grep -n "attachment_height" rust-agent-tui/src/ui/main_ui.rs`
   - 预期: is_empty() 时值为 0，Constraint::Length(0) ✅

5. [x] **全量测试通过，多模态构建逻辑正确**
   - `cargo test -p rust-agent-tui 2>&1 | grep "test result"`
   - 结果: test result: ok. 43 passed; 0 failed ✅
