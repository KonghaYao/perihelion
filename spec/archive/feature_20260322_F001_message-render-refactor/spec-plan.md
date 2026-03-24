# 消息渲染层重构 执行计划

**目标:** 引入 ViewModel 中间层，将消息数据处理与 UI 渲染完全解耦，支持 Markdown 渲染、Block 级别渲染和工具折叠

**技术栈:** Rust, ratatui, tui-markdown, tokio

**设计文档:** spec/feature_20260322_F001_message-render-refactor/spec-design.md

---

### Task 1: 添加 tui-markdown 依赖

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`

**执行步骤:**
- [x] 在 `[dependencies]` 中添加 `tui-markdown = "0.3"`
- [x] 运行 `cargo build -p rust-agent-tui` 确认依赖拉取成功

**检查步骤:**
- [x] 确认依赖编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出 `Finished` 无 error

---

### Task 2: MessageViewModel 数据模型

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/mod.rs`
- 新建: `rust-agent-tui/src/ui/message_view.rs`

**执行步骤:**
- [x] 创建 `rust-agent-tui/src/ui/` 目录和 `mod.rs`
  - 声明 `pub mod message_view;` `pub mod message_render;` `pub mod markdown;`
- [x] 在 `message_view.rs` 中定义 `MessageViewModel` 枚举
  - 五个变体：`UserBubble`、`AssistantBubble`、`ToolBlock`、`SystemNote`、`TodoStatus`
  - 每个变体包含设计文档中定义的字段
- [x] 定义 `ContentBlockView` 枚举
  - 三个变体：`Text`（含 raw + rendered + dirty）、`Reasoning`（char_count）、`ToolUse`（name + input_preview）
- [x] 实现 `MessageViewModel::from_base_message(msg: &BaseMessage) -> Self`
  - Human → `UserBubble`，内容通过 markdown 模块解析为 `rendered`
  - Ai → `AssistantBubble`，遍历 `content_blocks()` 转为 `Vec<ContentBlockView>`
  - Tool → `ToolBlock`，从 `tool_call_id` 提取 tool_name，`collapsed: true`，通过 `tool_color()` 计算颜色
  - System → `SystemNote`
- [x] 实现 `MessageViewModel::append_chunk(&mut self, chunk: &str)`
  - 仅对 `AssistantBubble` 生效：找到最后一个 `ContentBlockView::Text`，追加到 raw，标记 `dirty = true`
  - 如果没有 Text block，创建新的
- [x] 实现 `MessageViewModel::toggle_collapse(&mut self)`
  - 仅对 `ToolBlock` 生效：翻转 `collapsed` 字段
- [x] 实现 `MessageViewModel::is_assistant(&self) -> bool` 辅助方法
- [x] 将 `tool_color(name: &str) -> Color` 函数从 `ui.rs` 搬到 `message_view.rs` 并改为 pub

**检查步骤:**
- [x] 模块声明正确，编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: `Finished` 无 error
- [x] from_base_message 覆盖所有 BaseMessage 变体
  - `grep -c 'BaseMessage::' rust-agent-tui/src/ui/message_view.rs`
  - 预期: 至少 4（Human/Ai/Tool/System 各一个 match arm）

---

### Task 3: Markdown 封装模块

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/markdown.rs`

**执行步骤:**
- [x] 创建 `markdown.rs`，封装 tui-markdown 的调用
- [x] 实现 `pub fn parse_markdown(input: &str) -> Text<'static>`
  - 调用 `tui_markdown::from_str(input)` 将 markdown 文本转为 ratatui `Text`
  - 处理空字符串边界情况
- [x] 实现 `pub fn ensure_rendered(block: &mut ContentBlockView)`
  - 如果 `dirty == true`，调用 `parse_markdown(raw)` 更新 `rendered`，设置 `dirty = false`
  - 非 Text 变体忽略

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: `Finished` 无 error

---

### Task 4: 渲染层重构

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/message_render.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 在 `message_render.rs` 中实现 `pub fn render_view_model(vm: &MessageViewModel, width: usize) -> Vec<Line<'static>>`
  - `UserBubble`：`"▶ 你  "` 前缀（绿色粗体）+ rendered 内容的各行
  - `AssistantBubble`：`"◆ Agent  "` 前缀（青色粗体），遍历 blocks：
    - `Text`：使用 `rendered` 的行，前缀 `"  "`
    - `Reasoning`：`"  💭 思考 (N chars)"` 样式（紫色）
    - `ToolUse`：`"  🔧 name"` 样式
  - `ToolBlock { collapsed: true }`：单行 `"⚙ display_name ▸"`（带颜色）
  - `ToolBlock { collapsed: false }`：header `"⚙ display_name ▾"` + 内容行（`"  │ "` 前缀，灰色）
  - `SystemNote`：`"ℹ "` 前缀（蓝色）+ 内容（灰色）
  - `TodoStatus`：直接按行渲染 rendered 文本
- [x] 修改 `ui.rs` 中的 `render_messages()` 函数
  - 将 `for msg in &app.messages` 改为 `for vm in &mut app.view_messages`
  - 渲染前调用 `markdown::ensure_rendered()` 处理 dirty blocks
  - 调用 `message_render::render_view_model()` 替代 `message_to_lines()`
  - 对话类型的空行间隔逻辑保持不变（UserBubble 和 AssistantBubble 前后加空行）
- [x] 删除 `ui.rs` 中的旧函数 `message_to_lines()` 和 `tool_color()`
- [x] `visual_rows()` 函数保留在 `ui.rs` 中（滚动条计算仍需要）

**检查步骤:**
- [x] render_view_model 覆盖所有 ViewModel 变体
  - `grep -c 'MessageViewModel::' rust-agent-tui/src/ui/message_render.rs`
  - 预期: 至少 6（每个变体 + collapsed 状态两个 arm）
- [x] 旧渲染函数已删除
  - `grep -c 'fn message_to_lines' rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 0
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: `Finished` 无 error

---

### Task 5: App 层集成

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/command/help.rs`
- 修改: `rust-agent-tui/src/command/agent.rs`
- 修改: `rust-agent-tui/src/command/history.rs`

**执行步骤:**
- [x] 替换 App 结构体中的 `messages` 字段
  - `pub messages: Vec<ChatMessage>` → `pub view_messages: Vec<MessageViewModel>`
  - 删除 `ChatMessage` 结构体和所有 impl
- [x] 重写 `poll_agent()` 方法，使用 ViewModel 转换
  - `AgentEvent::AssistantChunk(chunk)`：找最后一个 AssistantBubble，调用 `append_chunk()`；不存在则 push 新的
  - `AgentEvent::ToolCall { name, display, is_error }`：push `MessageViewModel::ToolBlock { collapsed: true, color: tool_color(&name), ... }`
  - `AgentEvent::Done`：找最后一个 AssistantBubble，设 `is_streaming = false`
  - `AgentEvent::Interrupted`：push `MessageViewModel::SystemNote`
  - `AgentEvent::Error(e)`：push `MessageViewModel::ToolBlock { is_error: true, ... }`
  - `AgentEvent::TodoUpdate(todos)`：更新/创建 `MessageViewModel::TodoStatus`
  - `AgentEvent::StateSnapshot`：逻辑不变（仅更新 `agent_state_messages`）
  - `AgentEvent::ApprovalNeeded` / `AskUserBatch`：逻辑不变
- [x] 重写 `submit_message()` 中 push 用户消息的代码
  - `self.messages.push(ChatMessage::user(input))` → `self.view_messages.push(MessageViewModel::user(input))`
  - 加载失败时 push 的 tool 错误消息也改为 `MessageViewModel::ToolBlock`
- [x] 重写 `open_thread()` 中的历史加载
  - `for msg in base_msgs` 中，将 `ChatMessage { inner, display_name, tool_name }` 替换为 `MessageViewModel::from_base_message(&msg)`
  - 删除 hack 式的 `tool_call_id` / `display_name` 提取逻辑
- [x] 重写 `new_thread()`：`self.messages.clear()` → `self.view_messages.clear()`
- [x] 修改 `event.rs` 中的 `ChatMessage` 引用
  - 更新 import：移除 `ChatMessage`，添加 `MessageViewModel` 的引用
  - 未知命令的 `app.messages.push(ChatMessage::system(...))` → `app.view_messages.push(MessageViewModel::system(...))`
- [x] 修改 `command/help.rs`：`ChatMessage::system(...)` → `MessageViewModel::system(...)`
- [x] 修改 `command/agent.rs`：同上
- [x] 修改 `command/history.rs`：同上
- [x] 添加工具折叠快捷键处理
  - 在 `event.rs` 中添加一个键位（如 `Tab`）用于切换当前可见的 ToolBlock 折叠状态
  - 需要在 App 中跟踪一个 "当前聚焦的工具块索引"，或简化为切换所有工具块的折叠状态
- [x] 更新 `persisted_count` 和 `todo_message_index` 的引用（从 `self.messages.len()` 改为 `self.view_messages.len()`）

**检查步骤:**
- [x] ChatMessage 已完全移除
  - `grep -rn 'ChatMessage' rust-agent-tui/src/`
  - 预期: 0 匹配
- [x] 所有文件中 `app.messages` 引用已迁移
  - `grep -rn '\.messages' rust-agent-tui/src/ | grep -v 'view_messages' | grep -v 'agent_state_messages' | grep -v 'state_messages' | grep -v '//'`
  - 预期: 0 匹配（排除注释和 agent_state_messages）
- [x] 编译通过无 warning
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: `Finished` 无 error 无 warning
- [ ] 全量测试通过
  - `cargo test 2>&1 | tail -5`
  - 预期: `test result: ok`

---

### Task 6: Message Render Refactor Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 环境变量: 需配置有效的 LLM API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）

**End-to-end verification:**

1. 编译和测试通过
   - `cargo build -p rust-agent-tui 2>&1 | tail -3 && cargo test 2>&1 | tail -5`
   - Expected: `Finished` + `test result: ok`
   - On failure: check Task 1-5

2. ChatMessage 完全移除验证
   - `grep -rn 'ChatMessage' rust-agent-tui/src/ | grep -v '//' | wc -l`
   - Expected: 0
   - On failure: check Task 5 集成步骤

3. ViewModel 变体完整性验证
   - `grep -c 'UserBubble\|AssistantBubble\|ToolBlock\|SystemNote\|TodoStatus' rust-agent-tui/src/ui/message_view.rs`
   - Expected: 至少 10（定义 + from_base_message 中各出现一次以上）
   - On failure: check Task 2 数据模型

4. Markdown 集成验证
   - `grep -c 'tui_markdown' rust-agent-tui/src/ui/markdown.rs`
   - Expected: 至少 1
   - On failure: check Task 3 markdown 封装

5. 工具折叠功能验证
   - `grep -c 'toggle_collapse\|collapsed' rust-agent-tui/src/ui/message_view.rs`
   - Expected: 至少 3（字段定义 + toggle 方法 + 默认值设置）
   - On failure: check Task 2 toggle_collapse 实现
