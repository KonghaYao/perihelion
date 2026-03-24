# 数据管线统一化 人工验收清单

**生成时间:** 2026-03-23
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version && cargo --version`
- [ ] [AUTO] 全量编译通过: `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [ ] [AUTO] 确认 API Key 已配置（需要至少一个 LLM provider 才能运行 TUI）: `test -f rust-agent-tui/.env && echo "ok" || echo "missing .env"`

### 测试数据准备
- [ ] 确保有可用的 LLM API Key（ANTHROPIC_API_KEY 或 OPENAI_API_KEY），用于触发真实工具调用

---

## 验收项目

### 场景 1：代码结构验证

#### - [x] 1.1 ToolStart 事件含 tool_call_id 字段
- **来源:** Task 1 检查步骤, spec-design.md 架构变更 #1
- **操作步骤:**
  1. [A] `grep -n 'tool_call_id' rust-create-agent/src/agent/events.rs` → 期望: ToolStart 变体中包含 `tool_call_id: String` 字段
- **异常排查:**
  - 如果未找到: 检查 events.rs 中 ToolStart 定义是否被正确修改

#### - [x] 1.2 executor emit 传入 tool_call_id
- **来源:** Task 1 检查步骤, spec-design.md 架构变更 #3
- **操作步骤:**
  1. [A] `grep -A3 'AgentEvent::ToolStart' rust-create-agent/src/agent/executor.rs` → 期望: 输出包含 `tool_call_id: modified_call.id.clone()`
- **异常排查:**
  - 如果未找到: 检查 executor.rs 中 emit 调用是否遗漏 tool_call_id

#### - [x] 1.3 全量编译和测试通过
- **来源:** Task 1-3 检查步骤
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -E '(error|Finished)'` → 期望: 输出包含 "Finished" 且无 error
  2. [A] `cargo test -p rust-create-agent --lib 2>&1 | tail -3` → 期望: 输出 "test result: ok" 且无 failure
- **异常排查:**
  - 如果编译失败: 检查所有引用 ToolStart/ToolCall 的地方是否都已更新 tool_call_id 字段
  - 如果测试失败: 运行 `cargo test -p rust-create-agent --lib -- --nocapture` 查看详细输出

### 场景 2：实时流式工具调用显示

#### - [x] 2.1 实时工具调用显示参数
- **来源:** Task 4 E2E #1, spec-design.md 验收标准 #1
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui`，在输入框中发送一条消息触发 `read_file` 工具调用（如 "读取 Cargo.toml 文件"）。观察 ToolBlock 是否显示 `ReadFile(Cargo.toml)` 格式（带文件路径参数），而不是仅显示 `ReadFile` → 是/否
  2. [H] 同一次对话中，观察 `bash` 工具调用是否显示 `Bash(命令内容)` 格式（带命令参数）→ 是/否
- **异常排查:**
  - 如果只显示工具名无参数: 检查 `rust-agent-tui/src/app/agent.rs` 中 `ExecutorEvent::ToolStart` 匹配是否正确传递 `input`
  - 如果 TUI 启动失败: 检查 `.env` 文件中 API Key 配置

#### - [x] 2.2 多工具调用参数正确匹配
- **来源:** Task 4 E2E #3, spec-design.md 验收标准 #3
- **操作步骤:**
  1. [H] 运行 TUI，发送一条消息触发多个工具调用（如 "列出当前目录下所有 .rs 文件，然后读取 Cargo.toml"）。观察 `GlobFiles` 和 `ReadFile` 两个 ToolBlock 是否各自显示正确的参数（GlobFiles 显示 pattern，ReadFile 显示 file_path），无参数错配 → 是/否
  2. [H] 确认每个 ToolBlock 的参数与实际执行的工具调用一一对应，没有参数串位 → 是/否
- **异常排查:**
  - 如果参数错配: 检查 `tool_call_id` 匹配逻辑，确认 `ExecutorEvent::ToolStart` 中 `tool_call_id` 与对应工具调用的 `id` 一致

### 场景 3：历史恢复显示一致性

#### - [x] 3.1 历史恢复显示相同参数格式
- **来源:** Task 4 E2E #2, spec-design.md 验收标准 #2
- **操作步骤:**
  1. [H] 在场景 2 完成后，按 `Ctrl+C` 退出 TUI。重新运行 `cargo run -p rust-agent-tui`，输入 `/history`，选择刚才的对话 thread。观察恢复后的 ToolBlock 是否显示与实时一致的 `ReadFile(文件路径)` 格式（带参数），而不是仅显示 `ReadFile` → 是/否
  2. [H] 对比恢复后的多个 ToolBlock，确认每个工具的参数与实时对话时显示的完全一致 → 是/否
- **异常排查:**
  - 如果历史恢复只显示工具名无参数: 检查 `open_thread()` 中 `prev_ai_tool_calls` 是否正确存储了 `(id, name, arguments)` 三元组
  - 如果部分工具有参数部分无: 检查 `from_base_message` 中 `tool_call_id` 查找逻辑，确认 `prev_ai_tool_calls` 在遇到新 Ai 消息时正确重置

#### - [x] 3.2 无匹配 tool_call_id 降级处理
- **来源:** Task 4 E2E #4, spec-design.md 边界情况处理
- **操作步骤:**
  1. [A] 确认 threads.db 路径: `ls ~/.zen-core/threads/threads.db` → 期望: 文件存在
  2. [A] 创建测试 thread（孤立 Tool 消息，无对应 Ai 消息）:
     ```
     sqlite3 ~/.zen-core/threads/threads.db "INSERT OR IGNORE INTO threads (id, title, cwd, created_at, updated_at) VALUES ('test-orphan-tool', 'orphan test', '/tmp', datetime('now'), datetime('now'));"
     ```
     → 期望: 命令成功执行无报错
  3. [A] 插入一条孤立 Tool 消息（无对应 Ai 消息的 tool_calls）:
     ```
     sqlite3 ~/.zen-core/threads/threads.db "INSERT INTO messages (thread_id, role, content, seq) VALUES ('test-orphan-tool', 'tool', json_object('tool_call_id', 'orphan-id-123', 'content', json_object('type', 'text', 'text', 'some result'), 'is_error', json('false')), 1);"
     ```
     → 期望: 命令成功执行无报错
  4. [A] 验证测试数据已插入: `sqlite3 ~/.zen-core/threads/threads.db "SELECT count(*) FROM messages WHERE thread_id='test-orphan-tool';"` → 期望: 输出 1
  5. [H] 运行 `cargo run -p rust-agent-tui`，输入 `/history`，选择 "orphan test" thread。观察 TUI 是否正常加载且不崩溃，ToolBlock 显示工具名或 tool_call_id（无参数）→ 是/否
  6. [A] 清理测试数据: `sqlite3 ~/.zen-core/threads/threads.db "DELETE FROM threads WHERE id='test-orphan-tool'; DELETE FROM messages WHERE thread_id='test-orphan-tool';"` → 期望: 命令成功
- **异常排查:**
  - 如果 TUI 崩溃: 检查 `from_base_message` 中 `unwrap_or_else` 降级逻辑，确认当 `prev_ai_tool_calls` 中找不到匹配时使用 `tool_call_id` 作为工具名
  - 如果 sqlite3 命令失败: 检查数据库路径和 schema，运行 `sqlite3 ~/.zen-core/threads/threads.db ".schema"`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | ToolStart 含 tool_call_id | 1 | 0 | ⬜ | |
| 场景 1 | 1.2 | executor emit 传入 tool_call_id | 1 | 0 | ⬜ | |
| 场景 1 | 1.3 | 全量编译和测试通过 | 2 | 0 | ⬜ | |
| 场景 2 | 2.1 | 实时工具调用显示参数 | 0 | 2 | ⬜ | |
| 场景 2 | 2.2 | 多工具调用参数正确匹配 | 0 | 2 | ⬜ | |
| 场景 3 | 3.1 | 历史恢复显示相同参数格式 | 0 | 2 | ⬜ | |
| 场景 3 | 3.2 | 无匹配 tool_call_id 降级处理 | 5 | 1 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
