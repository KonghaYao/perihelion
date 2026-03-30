# sticky-human-message-header 人工验收清单

**生成时间:** 2026-03-30
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version && cargo --version`
- [ ] [AUTO] 编译项目（无错误）: `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -20` → 期望: 无输出（warning 允许）
- [ ] [AUTO] 运行 sticky header 全量测试: `cargo test -p rust-agent-tui --lib -- test_sticky_header 2>&1 | tail -15` → 期望: `5 passed`

---

## 验收项目

### 场景 1：基础显示逻辑

#### - [x] 1.1 空消息时无 sticky header
- **来源:** spec-design.md 验收标准 / Task 6 E2E 1
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_sticky_header_hidden_when_no_messages` → 期望: `ok`
- **异常排查:**
  - 如果测试失败：检查 `sticky_header.rs` 中 `if area.height == 0 { return; }` guard 是否存在
  - 确认 `app.core.last_human_message.is_none()` 时 `sticky_header_height` 返回 0

#### - [x] 1.2 发送消息后 header 立即显示
- **来源:** spec-design.md 验收标准 / Task 6 E2E 2
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_sticky_header_shows_after_submit` → 期望: `ok`
  2. [H] 运行 `cargo run -p rust-agent-tui` → 在终端输入任意文字（如 `hello`）按 Enter → 观察聊天区顶部是否出现 `> hello` 字样（`>` 为 ACCENT 色加粗，消息文本为主文字色） → 是/否
- **异常排查:**
  - 如果测试失败：检查 `agent_ops.rs` 中 `self.core.last_human_message = Some(display)` 是否在 `submit_message` 中被调用
  - 如果人工观察失败：检查 `main_ui.rs` 中 `render_messages` 是否调用了 `sticky_header::render_sticky_header`

#### - [x] 1.3 连续发消息显示最后一条
- **来源:** spec-design.md 验收标准 / Task 6 E2E 4
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_sticky_header_shows_last_message_not_first` → 期望: `ok`
- **异常排查:**
  - 如果测试失败：确认 `submit_message` 每次都是覆盖赋值（`= Some(...)`），而非追加

#### - [x] 1.4 长消息截断
- **来源:** spec-design.md 验收标准 / Task 6 E2E 7
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_sticky_header_truncation_long_message` → 期望: `ok`
  2. [H] 运行 `cargo run -p rust-agent-tui` → 在终端输入一长段文字（超 120 字符）按 Enter → 观察 header 区域，确认超过 3 行后末尾出现 `…` → 是/否
- **异常排查:**
  - 如果测试失败：检查 `sticky_header.rs` 中 `wrap_message` 函数的多行截断逻辑和 `estimate_header_lines` clamp 到 3

---

### 场景 2：状态生命周期

#### - [x] 2.1 /clear 后 header 消失
- **来源:** spec-design.md 验收标准 / Task 6 E2E 5
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_sticky_header_hidden_after_clear` → 期望: `ok`
  2. [H] 运行 `cargo run -p rust-agent-tui` → 输入消息 → 输入 `/clear` → 观察聊天区，header 应完全消失，回复到 welcome 界面 → 是/否
- **异常排查:**
  - 如果测试失败：检查 `thread_ops.rs` 中 `new_thread` 函数是否包含 `self.core.last_human_message = None`

#### - [x] 2.2 打开历史 thread 恢复 header
- **来源:** spec-design.md 验收标准 / Task 6 E2E 6
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui` → 输入消息 → 输入 `/history` → 用方向键选择一个已有 thread（需确保该 thread 包含消息）按 Enter 打开 → 观察聊天区顶部 header 是否显示该 thread 最后一条 Human 消息 → 是/否
- **异常排查:**
  - 如果 header 未恢复：检查 `thread_ops.rs` 中 `open_thread` 函数，`base_msgs.iter().filter_map(...)` 是否正确提取了 Human 消息

---

### 场景 3：布局与 sticky 行为

#### - [x] 3.1 滚动时 header 固定不动（sticky 效果）
- **来源:** spec-design.md 验收标准 / Task 6 E2E 3
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui` → 输入消息并发送，等待 Agent 回复产生多行输出 → 向上滚动（按 `↑` 或 `PageUp`）多次 → 观察聊天区顶部，确认 `> 消息` header **始终固定在聊天区最上方不动**，只有下方消息列表滚动 → 是/否
- **异常排查:**
  - 如果 header 随消息滚动：检查 `main_ui.rs` Layout constraints，确认 `Constraint::Length(sticky_header_height)` 在 `Constraint::Min(1)` **上方**（顺序决定渲染位置）
  - 正确的约束顺序: `[0]=sticky_header → [1]=scrollable messages`

#### - [x] 3.2 终端宽度变化时行数重新计算
- **来源:** spec-design.md 验收标准 / Task 6 E2E 8
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui`（默认宽度） → 输入一条中等长度消息（如 50 字符） → 记住 header 占用的行数 → 手动调整终端宽度变宽（如将终端拉宽） → 观察 header 行数是否相应减少（消息可在更宽的行中放下） → 是/否
  2. [H] 将终端宽度缩小 → 观察 header 行数是否增加（消息需要更多行） → 是/否
- **异常排查:**
  - 如果行数不变：检查 `main_ui.rs` 中 `sticky_header_height` 的计算是否在每次 `render()` 时重新调用 `estimate_header_lines`，而非缓存

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] 步骤 | [H] 步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 空消息时无 header | 1 | 0 | ✓ | 自动验证通过 |
| 场景 1 | 1.2 | 发送消息后显示 header | 1 | 1 | ✓ | 自动+人工验证通过 |
| 场景 1 | 1.3 | 连续发消息显示最后一条 | 1 | 0 | ✓ | 自动验证通过 |
| 场景 1 | 1.4 | 长消息截断 | 1 | 1 | ✓ | 自动+人工验证通过 |
| 场景 2 | 2.1 | /clear 后 header 消失 | 1 | 1 | ✓ | 自动+人工验证通过 |
| 场景 2 | 2.2 | 打开历史 thread 恢复 header | 0 | 1 | ✓ | 人工验证通过 |
| 场景 3 | 3.1 | 滚动时 sticky 效果 | 0 | 1 | ✓ | 人工验证通过 |
| 场景 3 | 3.2 | 终端宽度变化时重新计算 | 0 | 2 | ✓ | 人工验证通过 |

**验收结论:** ✓ 全部通过
