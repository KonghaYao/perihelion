# TUI 状态面板与工具显示优化 执行计划

**目标:** 工具调用颜色分层、路径参数缩短、输入框上方固定 TODO 状态面板

**技术栈:** Rust, ratatui, rust-agent-tui

**设计文档:** ./spec-design.md

---

### Task 1: tool_display 函数拆分与路径缩短

**涉及文件:**
- 修改: `rust-agent-tui/src/app/tool_display.rs`

**执行步骤:**
- [x] 新增辅助函数 `strip_cwd(path: &str, cwd: Option<&str>) -> String`
  - 若 `cwd` 有值，尝试 `path.strip_prefix(&format!("{}/", cwd))` 得到相对路径
  - fallback：`path.rsplit('/').next().unwrap_or(path).to_string()`
- [x] 新增 `format_tool_name(tool: &str) -> String`
  - 直接复用现有 `to_pascal(tool)` 逻辑
- [x] 新增 `format_tool_args(tool: &str, input: &serde_json::Value, cwd: Option<&str>) -> Option<String>`
  - 在原 `extract_display_arg` 基础上，对路径字段调用 `strip_cwd`
  - `bash`、`search_files_rg` 保持原逻辑，不剥离路径
  - 需要缩短的工具：`read_file`、`write_file`、`edit_file`、`glob_files`、`folder_operations`
- [x] 保留旧函数 `format_tool_call_display`（调用新函数组合），保持现有调用点兼容
  - `format_tool_call_display` 内部改为 `format!("{}", format_tool_name(tool))` + 拼接 args（无 cwd，用于历史消息回放）

**检查步骤:**
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出（无编译错误）
- [x] `strip_cwd` 路径剥离逻辑正确
  - `cargo test -p rust-agent-tui --lib -- tool_display 2>&1 | tail -10`
  - 预期: `test result: ok` 或无 tool_display 测试（至少编译通过）
- [x] bash 参数不做路径处理（手动验证逻辑：`format_tool_args("bash", &json!({"command": "cargo build"}), Some("/home/user"))` 返回 `Some("cargo build")`）
  - `cargo test -p rust-agent-tui 2>&1 | grep -E "FAILED|ok"`
  - 预期: 无 FAILED

---

### Task 2: ViewModel 与渲染颜色分层

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_view.rs`
- 修改: `rust-agent-tui/src/ui/message_render.rs`

**执行步骤:**
- [x] 在 `MessageViewModel::ToolBlock` 变体中新增 `args_display: Option<String>` 字段
  - 字段位于 `display_name` 之后
- [x] 更新 `MessageViewModel::tool_block` 构造函数签名，增加 `args: Option<String>` 参数
  - 同步更新调用方：`app/mod.rs` 中 `handle_agent_event(ToolCall)` 分支
  - 调用方需传入从 `format_tool_args(&name, &input_json, Some(&cwd))` 计算的结果
- [x] 更新 `MessageViewModel::from_base_message` 中 `BaseMessage::Tool` 分支
  - 同样计算 `args_display`（历史回放无 cwd，传 `None`，路径不剥离）
- [x] 修改 `message_render.rs` 中 `ToolBlock` 标题行渲染
  - 拆为 3 个 Span：
    1. `icon` → 工具颜色
    2. `display_name arrow` → 工具颜色 + BOLD
    3. `(args_display)` → `Color::DarkGray`（仅在 `args_display.is_some()` 时追加）

**检查步骤:**
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出
- [x] `tool_block` 构造函数新签名被正确调用（无遗漏）
  - `grep -n "tool_block(" rust-agent-tui/src/app/mod.rs`
  - 预期: 所有调用处均已更新，参数数量匹配新签名
- [x] headless 测试（如有）通过
  - `cargo test -p rust-agent-tui 2>&1 | grep -E "FAILED|ok"`
  - 预期: 无 FAILED

---

### Task 3: TODO 状态面板（App 状态 + 布局 + 渲染）

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 修改 `App` struct：
  - 新增 `pub todo_items: Vec<TodoItem>`
  - 删除 `pub todo_message_index: Option<usize>`
  - 在 `App::new()` 中初始化 `todo_items: Vec::new()`，删除 `todo_message_index: None`
- [x] 修改 `handle_agent_event` 中 `AgentEvent::TodoUpdate` 分支：
  - 替换原有逻辑：`self.todo_items = todos; (true, false, false)`
  - 删除原有 `render_todos` 调用和 `LoadHistory` 发送逻辑
  - 删除原有 `todo_message_index` 相关更新
- [x] 清理 `new_thread()` 和 `submit_message()` 中 `todo_message_index = None` 的引用，改为 `self.todo_items.clear()`
- [x] 删除 `render_todos` 函数（原 `app/mod.rs` 末尾）
  - 同步删除 `TodoStatus` variant（`message_view.rs`）和相关渲染（`message_render.rs`）
- [x] 修改 `main_ui.rs` 中 `render` 函数：
  - Layout 从 4-slot 改为 5-slot，在消息区与输入框之间插入 `Constraint::Length(todo_height)`
  - `todo_height` 计算：`if app.todo_items.is_empty() { 0 } else { (app.todo_items.len() as u16 + 2).min(10) }`
  - 所有后续 chunks 下标 +1（帮助栏从 `chunks[3]` 变为 `chunks[4]`，命令浮层基准 area 从 `chunks[2]` 变为 `chunks[3]`）
- [x] 新增 `render_todo_panel(f: &mut Frame, app: &App, area: Rect)` 函数（`main_ui.rs`）：
  - `if area.height == 0 { return; }` 防御性判断
  - 边框颜色：`if app.loading { Color::Yellow } else { Color::Cyan }`
  - 标题：`" 📋 TODO "`
  - 按 TodoStatus 分色：`InProgress` → Yellow + BOLD，`Completed` → DarkGray，`Pending` → White
  - 超出截断：最多显示 `area.height.saturating_sub(2) as usize` 条

**检查步骤:**
- [x] 编译无错误
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出
- [x] 无遗漏的 `todo_message_index` 引用
  - `grep -rn "todo_message_index" rust-agent-tui/src/`
  - 预期: 无输出（已全部删除）
- [x] 无遗漏的 `TodoStatus` 引用
  - `grep -rn "TodoStatus" rust-agent-tui/src/`
  - 预期: 无输出（已全部删除）
- [x] chunks 下标正确（帮助栏和浮层基准 area 已更新）
  - `grep -n "chunks\[" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 最大下标为 4（chunks[0]～chunks[4]），无遗漏的旧 [3] 引用
- [x] 全量测试通过
  - `cargo test -p rust-agent-tui 2>&1 | grep -E "FAILED|ok"`
  - 预期: 无 FAILED

---

### Task 4: TUI 状态面板 Acceptance

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`（或 `cargo run -p rust-agent-tui -- -y` YOLO 模式）
- 需要配置好 API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）
- 可选：准备含 `todo_write` 调用的测试 prompt，例如："用 todo_write 工具记录3个任务"

**端到端验证:**

1. 编译验证 ✅
   - `cargo build -p rust-agent-tui 2>&1 | tail -3`
   - Expected: 包含 `Finished` 且无 error
   - On failure: 检查 Task 1-3（编译错误）

2. 工具颜色分层验证（代码检视）✅
   - `grep -A8 "ToolBlock {" rust-agent-tui/src/ui/message_render.rs | grep -E "args_display|DarkGray"`
   - Expected: 渲染代码中含 `args_display` 和 `Color::DarkGray` 的 Span 分支
   - On failure: 检查 Task 2（渲染拆分逻辑）

3. 路径缩短函数存在验证 ✅
   - `grep -n "strip_cwd\|format_tool_args" rust-agent-tui/src/app/tool_display.rs`
   - Expected: 两个函数均存在定义
   - On failure: 检查 Task 1（函数拆分）

4. TODO 面板布局插入验证 ✅
   - `grep -c "Constraint::Length" rust-agent-tui/src/ui/main_ui.rs`
   - Expected: 输出 `4`（标题1 + todo面板1 + 输入框1 + 帮助栏1，Min 不计入）
   - On failure: 检查 Task 3（Layout 5-slot 改造）

5. todo_message_index 已删除验证 ✅
   - `grep -rn "todo_message_index\|TodoStatus" rust-agent-tui/src/`
   - Expected: 无输出
   - On failure: 检查 Task 3（App 状态清理）

6. 全量测试 ✅
   - `cargo test -p rust-agent-tui 2>&1 | tail -5`
   - Expected: `test result: ok` 或无 FAILED
   - On failure: 检查 Task 2-3（ViewModel/App 改动）
