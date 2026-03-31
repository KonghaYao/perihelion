# 历史面板工作区过滤 执行计划

**目标:** 打开历史面板时，只显示当前工作目录下的对话

**技术栈:** Rust, ratatui, rusqlite

**设计文档:** `spec/feature_20260331_F001_history-workspace-tag/spec-design.md`

## 改动总览

- 仅修改 1 个文件：`rust-agent-tui/src/app/thread_ops.rs`
- 改动集中在 `open_thread_browser()` 方法，增加 cwd 过滤逻辑
- 无新文件，无新增依赖，无数据库变更

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链可用。

**执行步骤:**
- [x] 验证构建可用
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`

**检查步骤:**
- [x] 构建成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 无 error

---

### Task 1: 历史面板 cwd 过滤

**背景:**
用户在不同项目目录使用 TUI 时，`/history` 面板显示所有工作区的对话混杂在一起。需要在 `open_thread_browser()` 中按 `app.cwd` 过滤 ThreadMeta，只保留当前工作区的记录。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/thread_ops.rs` (~L183, `open_thread_browser()` 方法)

**执行步骤:**
- [x] 在 `open_thread_browser()` 中增加 cwd 过滤
  - 位置: `rust-agent-tui/src/app/thread_ops.rs:183` (`open_thread_browser()` 方法体)
  - 当前代码直接将 `threads` 传入 `ThreadBrowser::new()`
  - 改为: 先 `let cwd = self.cwd.clone()`，然后 `threads.into_iter().filter(|t| t.cwd == cwd).collect()` 过滤，再传入
  - 原因: ThreadMeta 已有 cwd 字段，无需数据库变更

- [x] 更新 thread_browser 面板标题显示当前工作区
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs:20` (Block title)
  - 将硬编码标题改为包含 `app.cwd` 的动态标题
  - 格式: `📝 选择对话 [cwd]  ↑↓:移动  Enter:确认  d:删除  Esc:关闭`
  - 原因: 让用户明确知道当前过滤的工作区

- [x] 为过滤逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/thread_ops.rs` (内联 `#[cfg(test)]` 模块)
  - 测试场景:
    - [相同 cwd]: 3 个 thread，2 个匹配当前 cwd → 过滤后保留 2 个
    - [无匹配]: 所有 thread 的 cwd 与当前不同 → 过滤后为空列表
    - [全部匹配]: 所有 thread 的 cwd 与当前相同 → 全部保留
  - 运行命令: `cargo test -p rust-agent-tui -- open_thread_browser`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 无 error
- [x] 测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 所有 test passed

---

### Task 2: 验收

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`

**端到端验证:**

1. 运行完整测试套件
   - `cargo test -p rust-agent-tui 2>&1 | tail -5`
   - 预期: 全部 test passed
   - 失败排查: 检查 Task 1 的测试步骤

2. 验证历史面板只显示当前工作区对话
   - 启动 TUI，输入 `/history`
   - 预期: 只显示当前 cwd 下的对话，标题包含当前路径
   - 失败排查: 检查 `thread_ops.rs:183` 的过滤逻辑

3. 验证新建对话不受影响
   - 输入 `/history`，选择"新建对话"
   - 预期: 正常创建新对话
   - 失败排查: 检查 `ThreadBrowser::new()` 调用是否正确
