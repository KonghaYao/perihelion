# TUI Langfuse Session 机制 人工验收清单

**生成时间:** 2026-03-25
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 编译项目: `cargo build -p rust-agent-tui 2>&1 | tail -2`
- [ ] [MANUAL] 确认已配置 LLM API Key（ANTHROPIC_API_KEY 或 OPENAI_API_KEY）（场景 2 需要）
- [ ] [MANUAL] 确认已配置 Langfuse 凭据：LANGFUSE_PUBLIC_KEY 和 LANGFUSE_SECRET_KEY（场景 2 的 2.1-2.3 需要；若跳过则只验证代码结构）

### 测试数据准备

- 场景 1 无需额外准备（纯代码静态检查 + 编译）
- 场景 2 需要可用的 Langfuse 项目 + 运行中的 TUI（`cargo run -p rust-agent-tui`）

---

## 验收项目

### 场景 1：代码结构与编译验证

#### - [x] 1.1 编译与单元测试

- **来源:** Task 1/2/3 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -2` → 期望: 输出含 `Finished`，无 `error`
  2. [A] `cargo test -p rust-agent-tui 2>&1 | grep "test result"` → 期望: 输出含 `test result: ok`，`0 failed`
- **异常排查:**
  - 如果编译失败: `cargo build -p rust-agent-tui 2>&1 | grep "^error"` 查看具体错误

#### - [x] 1.2 LangfuseSession / LangfuseTracer 代码结构

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "pub struct LangfuseSession" rust-agent-tui/src/langfuse/mod.rs` → 期望: 输出非空，包含结构体定义行
  2. [A] `grep -n "pub async fn new" rust-agent-tui/src/langfuse/mod.rs` → 期望: 输出含 `pub async fn new(config: LangfuseConfig, session_id: String) -> Option<Self>`
  3. [A] `grep -n "pub fn new" rust-agent-tui/src/langfuse/mod.rs` → 期望: 输出含 `pub fn new(session: Arc<LangfuseSession>) -> Self`（不含 async）
  4. [A] `grep -n "fn on_trace_start" rust-agent-tui/src/langfuse/mod.rs` → 期望: 输出含 `pub fn on_trace_start(&mut self, input: &str)`（无 `thread_id` 参数）
- **异常排查:**
  - 如果签名不符: 检查 `rust-agent-tui/src/langfuse/mod.rs` 中对应函数定义

#### - [x] 1.3 App 生命周期字段

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -c "langfuse_session" rust-agent-tui/src/app/mod.rs` → 期望: 输出 >= 6（字段声明 + new + new_headless + new_thread + open_thread + submit_message）
  2. [A] `grep -A8 "fn new_thread" rust-agent-tui/src/app/mod.rs | grep "langfuse_session"` → 期望: 输出含 `self.langfuse_session = None;`
  3. [A] `grep -A35 "fn open_thread\b" rust-agent-tui/src/app/mod.rs | grep "langfuse_session"` → 期望: 输出含 `self.langfuse_session = None;`
  4. [A] `grep -n "on_trace_start" rust-agent-tui/src/app/mod.rs` → 期望: 输出含 `on_trace_start(input.trim())`，不含第二个参数
- **异常排查:**
  - 如果某项为空: 检查对应函数是否漏加 `self.langfuse_session = None;`

#### - [x] 1.4 未配置 Langfuse 时正常运行

- **来源:** Task 3 端到端验证 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `LANGFUSE_PUBLIC_KEY="" LANGFUSE_SECRET_KEY="" cargo test -p rust-agent-tui 2>&1 | grep "test result"` → 期望: 输出含 `test result: ok`，无 FAILED、无 panic
  2. [A] `cargo clippy -p rust-agent-tui 2>&1 | grep "^error"` → 期望: 无输出（无 clippy error）
- **异常排查:**
  - 如果有 panic: 检查 `submit_message()` 中 `langfuse_session` 在未配置时是否保持 None

---

### 场景 2：Langfuse Session 行为验证

> **前置条件：** 需配置 LANGFUSE_PUBLIC_KEY、LANGFUSE_SECRET_KEY，并运行 TUI（`cargo run -p rust-agent-tui`）。
> 若无 Langfuse 凭据，可跳过此场景（场景 1 已覆盖代码结构正确性）。

#### - [ ] 2.1 同一对话多轮消息归属同一 Session

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui`，向 TUI 发送第一条消息（任意内容），等待回复完成。然后再发送第二条消息，等待回复完成。打开 Langfuse 后台（配置的 host），进入 Sessions 页面，查看是否存在一个 Session，其中包含 **两个** Trace（对应两轮对话）→ 是/否
  2. [H] 在 Langfuse Sessions 页面，点开该 Session，确认两个 Trace 的 `session_id` 字段值相同（均等于当前对话的 thread_id）→ 是/否
- **异常排查:**
  - 如果仍有两个独立 Session: 检查 `submit_message()` 中 `langfuse_session` 懒加载逻辑，确认第二轮不会重新创建 Session

#### - [ ] 2.2 新建对话生成新的 Session

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 TUI 中输入 `/clear` 或通过 `/history` 新建对话，然后发送一条消息等待回复。打开 Langfuse Sessions 页面，确认产生了一个**新的 Session**（与 2.1 中的 Session 不同的 session_id）→ 是/否
  2. [H] 在 Langfuse Sessions 列表中，确认现在共有 **两个** Session（旧对话一个，新对话一个），每个 Session 下各有对应的 Trace → 是/否
- **异常排查:**
  - 如果新对话消息仍归入旧 Session: 检查 `new_thread()` 中是否正确执行 `self.langfuse_session = None;`

#### - [ ] 2.3 打开历史对话使用正确 Session

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 TUI 中输入 `/history`，从历史列表中打开 2.1 步骤中创建的旧对话，发送一条新消息，等待回复。打开 Langfuse，确认这条新消息的 Trace 归属于 **旧对话的 Session**（session_id 与 2.1 中相同），而非创建了新的 Session → 是/否
  2. [H] 确认旧对话的 Session 现在包含 **三个** Trace（2.1 中的两条 + 刚才新发的一条）→ 是/否
- **异常排查:**
  - 如果新 Trace 未归入旧 Session: 检查 `open_thread()` 末尾是否有 `self.langfuse_session = None;`，以及 `submit_message()` 中重建时是否正确使用了打开的 thread_id

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 代码结构 | 1.1 | 编译与单元测试 | 2 | 0 | ⬜ | |
| 代码结构 | 1.2 | LangfuseSession/Tracer 结构 | 4 | 0 | ⬜ | |
| 代码结构 | 1.3 | App 生命周期字段 | 4 | 0 | ⬜ | |
| 代码结构 | 1.4 | 未配置 Langfuse 时正常运行 | 2 | 0 | ⬜ | |
| Langfuse 行为 | 2.1 | 同一对话多轮 → 同一 Session | 0 | 2 | ⬜ | 需 Langfuse 凭据 |
| Langfuse 行为 | 2.2 | 新建对话 → 新 Session | 0 | 2 | ⬜ | 需 Langfuse 凭据 |
| Langfuse 行为 | 2.3 | 打开历史对话 → 正确 Session | 0 | 2 | ⬜ | 需 Langfuse 凭据 |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
