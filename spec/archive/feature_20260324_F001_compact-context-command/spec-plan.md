# /compact 上下文压缩指令 执行计划

**目标:** 实现 `/compact [instructions]` TUI 指令，通过单次 LLM 调用将对话历史压缩为结构化摘要，替换 `agent_state_messages` 并更新 TUI 显示

**技术栈:** Rust, Tokio, ratatui, rust-create-agent (BaseModel/LlmRequest), rust-agent-tui (App/Command)

**设计文档:** ./spec-design.md

---

### Task 1: AgentEvent 扩展 + App 核心逻辑

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 在 `AgentEvent` 枚举中新增两个变体
  - `CompactDone(String)` — 压缩成功，携带摘要文本
  - `CompactError(String)` — 压缩失败，携带错误信息
- [x] 在 `App` 上实现 `start_compact(&mut self, instructions: String)` 方法
  - 若 `agent_state_messages` 为空，向 `view_messages` push 一条系统提示"无可压缩的上下文"后直接返回
  - 从 `zen_config`（或环境变量）获取 LlmProvider；获取失败时 push 错误提示并返回
  - 克隆 `agent_state_messages` 和 provider
  - 创建新的 `mpsc::channel::<AgentEvent>(8)`，将 rx 赋值给 `self.agent_rx`
  - 调用 `self.set_loading(true)` 进入 loading 状态
  - `tokio::spawn` 启动 `compact_task(messages, model, instructions, tx)`
- [x] 在 `handle_agent_event` 的 match 分支中新增两个处理臂
  - `CompactDone(summary)`:
    1. `self.agent_state_messages = vec![BaseMessage::system(summary.clone())]`
    2. 截断 view_messages，保留最近 10 条：`if len > 10 { self.view_messages = self.view_messages.split_off(len - 10) }`
    3. 在 `view_messages` 头部 `insert(0, MessageViewModel::system("📦 上下文已压缩（保留最近 10 条显示消息，LLM 历史已替换为摘要）"))`
    4. `self.set_loading(false)`; `self.agent_rx = None`
    5. 同步更新 render_tx（发送 `RenderEvent::Clear` + 重新 AddMessage 所有 view_messages）
  - `CompactError(msg)`:
    1. push `MessageViewModel::system(format!("❌ 压缩失败: {}", msg))`
    2. `self.set_loading(false)`; `self.agent_rx = None`

**检查步骤:**
- [x] 编译通过，无新增警告
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出（无编译错误）
- [x] AgentEvent match 覆盖完整（无 non-exhaustive 警告）
  - `cargo build -p rust-agent-tui 2>&1 | grep "non-exhaustive"`
  - 预期: 无输出

---

### Task 2: compact_task 异步函数

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在文件末尾新增 `pub async fn compact_task` 函数，签名：
  ```rust
  pub async fn compact_task(
      messages: Vec<rust_create_agent::messages::BaseMessage>,
      model: Box<dyn rust_create_agent::llm::BaseModel>,
      instructions: String,
      tx: tokio::sync::mpsc::Sender<super::AgentEvent>,
  )
  ```
- [x] 实现消息格式化：遍历 `messages`，跳过 System 消息，将 Human/Ai/Tool 格式化为 `"[角色] 内容"` 文本行
  - Human → `"[用户] {content}"`
  - Ai（含工具调用）→ `"[助手] {text_content}（调用了工具: {tool_names}）"`
  - Tool → `"[工具结果:{tool_call_id}] {content}"`
  - 若单条内容超过 500 字符，截断并加 `"...(已截断)"`
- [x] 构造 `LlmRequest`：
  - 系统 prompt（固定）：要求生成 Markdown 摘要，分"## 目标"、"## 已完成操作"、"## 关键发现"三节
  - 用户消息：`<conversation>` 标签包裹格式化文本，若有 instructions 追加"压缩时请特别注意: {instructions}"
  - `LlmRequest::new(vec![BaseMessage::human(user_msg)]).with_system(system_prompt)`
- [x] 调用 `model.invoke(request).await`
  - 成功：提取 `response.message.content()` 作为摘要文本，发送 `AgentEvent::CompactDone(summary)`
  - 失败：发送 `AgentEvent::CompactError(e.to_string())`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] compact_task 函数存在且签名正确
  - `grep -n "pub async fn compact_task" rust-agent-tui/src/app/agent.rs`
  - 预期: 输出包含行号和函数签名

---

### Task 3: CompactCommand 注册

**涉及文件:**
- 新建: `rust-agent-tui/src/command/compact.rs`
- 修改: `rust-agent-tui/src/command/mod.rs`

**执行步骤:**
- [x] 新建 `rust-agent-tui/src/command/compact.rs`，实现 `CompactCommand` struct：
  ```rust
  use crate::app::App;
  use super::Command;

  pub struct CompactCommand;

  impl Command for CompactCommand {
      fn name(&self) -> &str { "compact" }
      fn description(&self) -> &str { "压缩对话上下文（调用 LLM 生成摘要）" }
      fn execute(&self, app: &mut App, args: &str) {
          app.start_compact(args.to_string());
      }
  }
  ```
- [x] 在 `command/mod.rs` 头部添加 `pub mod compact;`，并在 `default_registry()` 中注册：
  - `r.register(Box::new(compact::CompactCommand));`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] `/compact` 命令已注册（文件中存在）
  - `grep -n "compact" rust-agent-tui/src/command/mod.rs`
  - 预期: 输出包含 `pub mod compact` 和 `CompactCommand`
- [x] 前缀匹配逻辑验证：`/co` 唯一匹配 compact（无其他 `co` 前缀命令）
  - `grep -rn "fn name.*co" rust-agent-tui/src/command/`
  - 预期: 仅 compact.rs 中 name 返回 "compact"
- [x] `/help` 能列出 compact 命令
  - `cargo test -p rust-agent-tui -- help 2>&1 | grep -i compact`
  - 预期: 能找到相关测试或编译结果包含 compact

---

### Task 4: /compact 指令验收

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui -- -y`（YOLO 模式，跳过 HITL 审批）
- 需要配置 `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`
- 确保 Task 1~3 全部完成且编译通过

**端到端验证:**

1. **全量编译测试通过**
   - `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - 预期: 输出包含 "test result: ok" 或所有 test 通过，无 FAILED
   - 失败时: 检查 Task 1（AgentEvent match 覆盖）和 Task 3（注册）
   - [x] ✅ 43 passed; 0 failed

2. **命令注册验证（前缀匹配）**
   - `grep -c "CompactCommand" rust-agent-tui/src/command/mod.rs`
   - 预期: 输出 `1`（恰好注册一次）
   - 失败时: 检查 Task 3
   - [x] ✅ 输出 1

3. **命令文件存在且结构完整**
   - `grep -n "fn name\|fn description\|fn execute" rust-agent-tui/src/command/compact.rs`
   - 预期: 输出包含 3 行，分别对应三个方法实现
   - 失败时: 检查 Task 3
   - [x] ✅ 3 行均存在

4. **start_compact 空历史分支**
   - `grep -n "无可压缩的上下文" rust-agent-tui/src/app/mod.rs`
   - 预期: 找到对应字符串（空历史保护逻辑存在）
   - 失败时: 检查 Task 1 中 start_compact 的空历史判断
   - [x] ✅ 第 1278 行找到

5. **CompactDone 处理：agent_state_messages 替换逻辑**
   - `grep -n "CompactDone\|agent_state_messages.*system\|system.*summary" rust-agent-tui/src/app/mod.rs`
   - 预期: 输出至少 2 行（CompactDone 分支 + 替换逻辑）
   - 失败时: 检查 Task 1 中 handle_agent_event 的 CompactDone 处理
   - [x] ✅ 3 行输出

6. **compact_task 消息格式化函数存在**
   - `grep -n "compact_task\|\[用户\]\|\[助手\]\|\[工具结果" rust-agent-tui/src/app/agent.rs`
   - 预期: 输出包含函数定义和中文角色标签
   - 失败时: 检查 Task 2
   - [x] ✅ 5 行输出，函数和标签均存在

7. **全量 Cargo 测试（含现有测试不回归）**
   - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
   - 预期: 输出包含 "test result: ok"，无 FAILED
   - 失败时: 检查 Task 1~2 中是否破坏了现有消息处理逻辑
   - [x] ✅ 32 passed; 0 failed
