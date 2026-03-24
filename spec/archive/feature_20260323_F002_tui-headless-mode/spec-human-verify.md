# TUI Headless 测试模式 人工验收清单

**生成时间:** 2026-03-23 22:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译整个 workspace（debug）: `cargo build --workspace 2>&1 | tail -3`
- [ ] [AUTO] 确认 headless feature 已在 Cargo.toml 中声明: `grep -A2 '\[features\]' rust-agent-tui/Cargo.toml`

### 测试数据准备

无需外部数据——所有测试均为纯内存操作，无网络依赖。

---

## 验收项目

### 场景 1：渲染管道统一性

> 验证 TestBackend 与生产 CrosstermBackend 走相同的 main_ui 渲染路径，snapshot() 能正确反映 draw() 结果。

#### - [x] 1.1 snapshot() 返回正确行数

- **来源:** Task 2 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_snapshot_row_count -- --nocapture 2>&1 | tail -5` → 期望: 输出 "test test_snapshot_row_count ... ok"，不 panic
  2. [A] `cargo test -p rust-agent-tui test_snapshot_row_count 2>&1 | grep -E "FAILED|ok"` → 期望: 输出 "ok"，无 FAILED
- **异常排查:**
  - 如果测试 panic：检查 `HeadlessHandle::snapshot()` 实现（`rust-agent-tui/src/ui/headless.rs`），确认按 `buffer.area.width` 分行

#### - [x] 1.2 AssistantChunk 流式消息渲染到屏幕

- **来源:** Task 3 测试用例 / spec-design.md 验收标准（AssistantChunk 流式消息渲染）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_assistant_chunk_renders -- --nocapture 2>&1 | tail -10` → 期望: 输出包含 "test test_assistant_chunk_renders ... ok"，无 panic
  2. [A] `cargo test -p rust-agent-tui test_assistant_chunk_renders 2>&1 | grep -E "FAILED|ok"` → 期望: 输出 "ok"，无 FAILED
- **异常排查:**
  - 如果断言 "应显示 Agent 标头" 失败：检查 `main_ui::render()` 是否在第一行渲染了 "Agent" 前缀
  - 如果断言 "应显示消息内容" 失败：检查 `handle_agent_event(AssistantChunk)` 是否正确追加到 `view_messages`

#### - [x] 1.3 ToolCall 工具块渲染到屏幕

- **来源:** Task 3 测试用例 / spec-design.md 验收标准（ToolCall 工具块渲染）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_tool_call_renders -- --nocapture 2>&1 | tail -10` → 期望: 输出 "test test_tool_call_renders ... ok"，无 panic
  2. [A] `cargo test -p rust-agent-tui test_tool_call_renders 2>&1 | grep -E "FAILED|ok"` → 期望: 输出 "ok"，无 FAILED
- **异常排查:**
  - 如果断言失败：确认 ToolBlock 渲染 display 字段（"读取 src/main.rs"）或 name 字段（"read_file"）或工具图标（"⚙"）至少其中一项

#### - [x] 1.4 用户消息渲染到屏幕

- **来源:** Task 3 测试用例 / spec-design.md 验收标准（用户消息渲染）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_user_message_renders -- --nocapture 2>&1 | tail -10` → 期望: 输出 "test test_user_message_renders ... ok"，无 panic
  2. [A] `cargo test -p rust-agent-tui test_user_message_renders 2>&1 | grep -E "FAILED|ok"` → 期望: 输出 "ok"，无 FAILED
- **异常排查:**
  - 如果断言 "应显示用户消息" 失败：确认测试内容使用 ASCII（"hello from user"），因 CJK 字符在 TestBackend buffer 中有宽字符填充

#### - [x] 1.5 Clear 后 RenderCache 清空

- **来源:** Task 3 测试用例 / spec-design.md 验收标准（Clear 后屏幕为空）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui test_clear_empties_render_cache -- --nocapture 2>&1 | tail -10` → 期望: 输出 "test test_clear_empties_render_cache ... ok"，无 panic
  2. [A] `cargo test -p rust-agent-tui test_clear_empties_render_cache 2>&1 | grep -E "FAILED|ok"` → 期望: 输出 "ok"，无 FAILED
- **异常排查:**
  - 如果断言 "清空前应有内容" 失败：AssistantChunk 事件发送后需等待 2 次 RenderEvent 通知（AddMessage + AppendChunk）
  - 如果断言 "清空后 RenderCache 应为空" 失败：检查 `RenderEvent::Clear` 处理逻辑是否重置 `total_lines = 0`

---

### 场景 2：测试隔离与 Feature Flag

> 验证 headless 代码通过条件编译隔离，release 产物不含测试专用代码，且无 sleep 轮询。

#### - [x] 2.1 Release 编译不包含 headless 代码

- **来源:** Task 2 检查步骤 / Task 4 验收 / spec-design.md Feature Flag 策略
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui --release 2>&1 | grep -E "^error"` → 期望: 无输出（零编译错误）
  2. [A] `cargo build -p rust-agent-tui --release 2>&1 | grep -iE "warning.*headless"` → 期望: 无输出（无 headless 相关 warning）
- **异常排查:**
  - 如果有 headless warning：检查 `rust-agent-tui/src/ui/mod.rs` 中 headless 模块声明是否正确使用 `#[cfg(any(test, feature = "headless"))]`
  - 如果有编译错误：检查 `Cargo.toml` 中 `[features]` 是否声明了 `headless = []`

#### - [x] 2.2 wait_for_render 无 sleep 调用

- **来源:** Task 3/4 检查步骤 / spec-design.md 同步策略（不使用 sleep）
- **操作步骤:**
  1. [A] `grep -rn "sleep" rust-agent-tui/src/ui/headless.rs` → 期望: 无输出，或仅注释中出现（无实际 sleep 函数调用）
  2. [A] `grep -n "tokio::time::sleep\|std::thread::sleep" rust-agent-tui/src/ui/headless.rs` → 期望: 无输出（零 sleep 函数调用）
- **异常排查:**
  - 如果存在 sleep：替换为 `render_notify.notified().await` 实现零轮询同步

---

### 场景 3：工程整体质量

> 验证全量测试通过，且实现未引入新的编译警告。

#### - [x] 3.1 全量测试通过（含所有新增 headless 测试）

- **来源:** Task 1/3/4 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep -E "test result|FAILED"` → 期望: 输出 "test result: ok"，无 FAILED
  2. [A] `cargo test -p rust-agent-tui 2>&1 | grep "passed"` → 期望: 输出包含 "passed"（数量 ≥ 20）
- **异常排查:**
  - 如果有 FAILED：逐一运行 `cargo test -p rust-agent-tui <test_name> -- --nocapture` 查看详细输出
  - 如果测试数量不足：确认 `src/ui/headless.rs` 中的 `#[cfg(test)] mod tests` 模块包含所有 5 个新测试

#### - [x] 3.2 Workspace 编译无新增 warning

- **来源:** Task 4 验收 item 5
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | grep "^warning" | wc -l` → 期望: 输出数字 ≤ 3（基线为 3）
  2. [A] `cargo build --workspace 2>&1 | grep "^warning"` → 期望: 无 dead_code / unused_import 类 warning 与实现前相比新增
- **异常排查:**
  - 如果 warning 超过基线：检查是否有未使用的 import 或 dead_code，在对应字段加 `#[allow(dead_code)]` 或移除无用 import

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 渲染管道统一 | 1.1 | snapshot() 返回正确行数 | 2 | 0 | ✅ | |
| 场景 1 渲染管道统一 | 1.2 | AssistantChunk 流式消息渲染 | 2 | 0 | ✅ | |
| 场景 1 渲染管道统一 | 1.3 | ToolCall 工具块渲染 | 2 | 0 | ✅ | |
| 场景 1 渲染管道统一 | 1.4 | 用户消息渲染 | 2 | 0 | ✅ | |
| 场景 1 渲染管道统一 | 1.5 | Clear 后 RenderCache 清空 | 2 | 0 | ✅ | |
| 场景 2 测试隔离 | 2.1 | Release 编译不含 headless 代码 | 2 | 0 | ✅ | |
| 场景 2 测试隔离 | 2.2 | wait_for_render 无 sleep | 2 | 0 | ✅ | |
| 场景 3 工程质量 | 3.1 | 全量测试通过 | 2 | 0 | ✅ | |
| 场景 3 工程质量 | 3.2 | Workspace 无新增 warning | 2 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
