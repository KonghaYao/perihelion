# 数据管线统一化 执行计划

**目标:** 统一实时流式和历史恢复的工具调用参数显示

**技术栈:** Rust, serde_json

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: ExecutorEvent 扩展

**涉及文件:**
- 修改: `rust-create-agent/src/agent/events.rs`
- 修改: `rust-create-agent/src/agent/executor.rs`

**执行步骤:**
- [x] 修改 `AgentEvent::ToolStart` 增加 `tool_call_id: String` 字段
  - 在 events.rs 的 `ToolStart` 变体中添加 `tool_call_id: String`
  - 保持 `name` 和 `input` 字段不变
- [x] 修改 executor.rs 中 `emit(AgentEvent::ToolStart)` 调用
  - 在 `run_before_tool` 后、`modified_calls.push` 前
  - 传入 `modified_call.id.clone()`

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 且无 error
- [x] 验证单元测试通过
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -10`
  - 预期: 所有测试通过，无 failure

---

### Task 2: TUI 事件转换适配

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 修改 `AgentEvent::ToolCall` 增加 `tool_call_id: String` 字段
  - 在 mod.rs 的 `ToolCall` 变体中添加 `tool_call_id: String`
- [x] 修改 agent.rs 中的事件转换逻辑
  - `ExecutorEvent::ToolStart` 匹配时提取 `tool_call_id`
  - 传递给 `AgentEvent::ToolCall`

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 且无 error

---

### Task 3: 历史恢复统一渲染

**涉及文件:**
- 新建: `rust-agent-tui/src/app/tool_display.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/ui/message_view.rs`

**执行步骤:**
- [x] 创建 `tool_display.rs` 提取公共格式化函数
  - 移动 `format_tool_call_display`、`extract_display_arg`、`to_pascal`、`truncate` 函数
  - 添加 `pub` 导出
- [x] 修改 `prev_ai_tool_calls` 类型为 `Vec<(String, String, Value)>`
  - 在 mod.rs 的 `open_thread()` 中
  - 存储 `(id, name, input)` 三元组
- [x] 修改 `MessageViewModel::from_base_message` 使用 input 生成 display
  - 在 message_view.rs 中
  - 从 `prev_ai_tool_calls` 查找 input
  - 调用 `format_tool_call_display` 生成 display
- [x] 在 mod.rs 中引入 tool_display 模块

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 且无 error
- [x] 验证历史恢复逻辑
  - `cargo test -p rust-agent-tui --lib tool_display 2>&1 | tail -10`
  - 预期: 测试通过（如有）

---

### Task 4: Data Pipeline Acceptance

**Prerequisites:**
- Start command: `cargo build -p rust-agent-tui`
- Test data setup: 准备包含工具调用的历史 thread（可手动创建或使用已有测试数据）

**End-to-end verification:**

1. 实时流式工具调用显示参数
   - 运行 TUI，执行一个工具调用（如 `read_file /path/to/file`）
   - 观察 ToolBlock 显示格式
   - Expected: 显示 `ReadFile(/path/to/file)` 格式（带参数）
   - On failure: 检查 Task 1, 2 的 tool_call_id 传递

2. 历史恢复显示相同参数格式
   - 在场景 1 后，退出 TUI
   - 重新启动 TUI，打开 `/history`，选择刚才的 thread
   - 观察 ToolBlock 显示格式
   - Expected: 显示 `ReadFile(/path/to/file)` 格式（与实时一致）
   - On failure: 检查 Task 3 的 prev_ai_tool_calls 和 format_tool_call_display

3. 多工具调用参数匹配正确
   - 运行 TUI，触发包含多个工具调用的操作（如同时 read_file + glob_files）
   - 观察每个 ToolBlock 的参数是否正确
   - Expected: 每个工具显示各自的参数，无错配
   - On failure: 检查 tool_call_id 匹配逻辑

4. 无匹配 tool_call_id 降级处理
   - 构造一个孤立 Tool 消息（无对应 Ai 消息）的历史记录
   - 加载该 thread
   - Expected: ToolBlock 显示工具名（无参数），不崩溃
   - On failure: 检查 from_base_message 的 unwrap_or_else 降级逻辑
