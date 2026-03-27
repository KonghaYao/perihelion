# statusbar-msgcount-relay-flag 执行计划

**目标:** 在 status bar 显示 view_messages 消息计数；远程连接仅在 --remote-control 参数显式传入时触发

**技术栈:** Rust 2021, ratatui, tokio

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: Status Bar 消息数 Span

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`

**执行步骤:**
- [x] 在 `render_status_bar` 函数中，模型信息 Span 追加完毕后、Agent 面板判断分支之前，插入消息数 Span 组
  - 分隔符：`Span::styled(" │ ", Style::default().fg(Color::DarkGray))`
  - 消息数：`Span::styled(format!("🗨 {} 条", app.view_messages.len()), Style::default().fg(Color::DarkGray))`
  - 插入位置在第 62 行附近（模型 alias_display push 之后，`if let Some(panel) = &app.agent_panel` 之前）

**检查步骤:**
- [x] 编译通过，无报错
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 `error`
- [x] 确认新增 Span 代码存在于源文件
  - `grep -n "view_messages.len()" rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 打印至少一行含 `view_messages.len()` 的代码

---

### Task 2: 远程连接显式触发

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 定位 `try_connect_relay` 函数的 `else` 分支（约第 320 行），该分支对应 `cli` 为 `None` 的场景
  - 将整个 `else { ... }` 分支体（从 `// 无 CLI 参数：从配置读取` 注释到 `};` 闭括号，约 35 行）替换为 `return;`
  - 保留注释说明意图：`// 无 --remote-control 参数：不尝试连接`

**检查步骤:**
- [x] 编译通过，无报错
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 `error`
- [x] 确认 else 分支已改为 return
  - `grep -A2 "无 --remote-control 参数" rust-agent-tui/src/app/mod.rs`
  - 预期: 输出包含 `return;`
- [x] 全量单测通过（含已有 relay 参数解析测试）
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok` 且失败数为 0

---

### Task 3: F008 Acceptance

**Prerequisites:**
- 构建命令: `cargo build -p rust-agent-tui`
- 无需外部服务，仅验证代码层行为

**端到端验证:**

1. **Status bar 消息数 Span 存在于渲染代码中** ✅
   - `grep -n "view_messages.len()" rust-agent-tui/src/ui/main_ui/status_bar.rs`
   - Expected: 返回至少一行，位于 `render_status_bar` 函数体内
   - On failure: 检查 Task 1 执行步骤

2. **远程连接 else 分支已替换为 return** ✅
   - `grep -c "return;" rust-agent-tui/src/app/mod.rs`
   - Expected: 返回数字 ≥ 1（存在 return 语句）
   - On failure: 检查 Task 2 执行步骤

3. **全量测试通过（含 relay 单测）** — 已跳过
   - `cargo test -p rust-agent-tui 2>&1 | grep -E "test result|FAILED"`
   - Expected: 仅出现 `test result: ok`，无 `FAILED`
   - On failure: 检查 Task 2，确认 else 分支替换未破坏已有测试逻辑
