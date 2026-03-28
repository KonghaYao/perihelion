# web-ask-user-interrupt 执行计划

**目标:** 扩展 AskUser 协议字段对齐核心层，Web 端弹窗支持完整交互，新增中断 Agent 能力

**技术栈:** Rust (serde / axum)、JavaScript ES Modules

**设计文档:** ./spec-design.md

---

### Task 1: 协议层字段扩展

**涉及文件:**
- 修改: `rust-relay-server/src/protocol.rs`

**执行步骤:**
- [x] 在 `AskUserQuestion` 之前新增 `AskUserOption` 结构体，派生 `Debug, Clone, Serialize, Deserialize`
  - 字段：`label: String`，`description: Option<String>`
- [x] 将 `AskUserQuestion` 中的 `question: String` 重命名为 `description: String`；`options: Vec<String>` 改为 `Vec<AskUserOption>`；补充 `tool_call_id: String`、`multi_select: bool`、`allow_custom_input: bool`、`placeholder: Option<String>`
  - 保留 `#[serde(default)]` 用于向后兼容的可选字段（multi_select/allow_custom_input 无默认不需要，但 `placeholder` 用 Option 即可）
- [x] 在 `WebMessage` enum 末尾新增 `CancelAgent` 无字段变体
- [x] 在 `#[cfg(test)]` 区块内新增两个测试：
  - `test_ask_user_question_serialization`：验证 `AskUserQuestion` 序列化包含 `description` 字段（不含旧 `question` 字段）
  - `test_cancel_agent_serialization`：验证 `WebMessage::CancelAgent` 序列化为 `{"type":"cancel_agent"}`，并能反序列化

**检查步骤:**
- [x] Rust 编译通过（仅 relay-server crate）
  - `cargo build -p rust-relay-server 2>&1 | tail -5`
  - 预期: 输出包含 `Compiling` 或 `Finished`，无 `error`
- [x] 新增序列化测试通过
  - `cargo test -p rust-relay-server -- test_ask_user_question_serialization test_cancel_agent_serialization 2>&1 | tail -10`
  - 预期: `2 passed`
- [x] 验证 `AskUserQuestion` 不再有 `question` 字段
  - `grep -n '"question"' rust-relay-server/src/protocol.rs`
  - 预期: 无输出（或仅在注释中）
- [x] 验证 `CancelAgent` 变体存在
  - `grep -n 'CancelAgent' rust-relay-server/src/protocol.rs`
  - 预期: 至少 2 行（enum 声明 + test）

---

### Task 2: TUI JSON 发送映射

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 找到 `AgentEvent::AskUserBatch(req)` 分支内构建 `questions: Vec<serde_json::Value>` 的 `serde_json::json!` 块
  - 当前只发送 `question`/`options`(字符串列表)/`multi_select` 三个字段
- [x] 替换为发送全部字段：`tool_call_id`、`description`、`multi_select`、`allow_custom_input`、`placeholder`；`options` 改为对象数组 `[{"label": ..., "description": null}]`
  - `q.options.iter().map(|o| serde_json::json!({"label": o.label, "description": null}))`
  - `allow_custom_input: q.allow_custom_input`
  - `placeholder: q.placeholder`
  - `tool_call_id: q.tool_call_id`
  - `description: q.description`

**检查步骤:**
- [x] 编译通过（含 TUI crate）
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 `error`
- [x] 验证发送代码含新字段
  - `grep -A 12 'AskUserBatch(req)' rust-agent-tui/src/app/agent_ops.rs | grep -E 'tool_call_id|allow_custom_input|placeholder'`
  - 预期: 3 行匹配（每个字段各一行）

---

### Task 3: TUI relay 中断处理

**涉及文件:**
- 修改: `rust-agent-tui/src/app/relay_ops.rs`

**执行步骤:**
- [x] 在 `poll_relay` 的 `match web_msg` 末尾（`WebMessage::Pong => {}` 之后、`WebMessage::SyncRequest` 之前）新增分支：
  ```rust
  WebMessage::CancelAgent => {
      self.interrupt();
      self.ask_user_prompt = None;
      self.hitl_prompt = None;
  }
  ```
- [x] 修改 `WebMessage::AskUserResponse { answers }` 分支中的匹配逻辑：
  - 将 `find(|q| q.data.description == *q_text)` 改为 `find(|q| q.data.tool_call_id == *q_text)`
  - 说明：Web 端提交时将使用 `tool_call_id` 作为 answers 的 key（Task 4 同步修改）

**检查步骤:**
- [x] 全量编译通过
  - `cargo build 2>&1 | tail -5`
  - 预期: 无 `error`（所有 crate）
- [x] 验证 `CancelAgent` 分支存在
  - `grep -n 'CancelAgent' rust-agent-tui/src/app/relay_ops.rs`
  - 预期: 至少 1 行
- [x] 验证匹配键已改为 tool_call_id
  - `grep -n 'tool_call_id' rust-agent-tui/src/app/relay_ops.rs`
  - 预期: 至少 1 行（AskUserResponse 分支中）
- [x] 原先的 `description` 匹配不再存在于该处
  - `grep -n 'data\.description' rust-agent-tui/src/app/relay_ops.rs`
  - 预期: 无输出

---

### Task 4: Web AskUser 弹窗增强

**涉及文件:**
- 修改: `rust-relay-server/web/js/dialog.js`

**执行步骤:**
- [x] 修改 `showAskUserDialog` 函数中问题标题读取：将 `q.question || q.text || ...` 改为 `q.description || q.question || q.text || ...`（向后兼容旧格式）
- [x] 修改选项渲染逻辑：当选项是对象 `{label, description}` 时渲染 `opt.label`；若 `opt.description` 有值，在 label 下方追加一个 `<div>` 灰色小字副标题（fontSize: 11px, color: var(--text-muted)）
- [x] 新增 `allow_custom_input` 处理：若 `q.allow_custom_input === true`，在选项列表之后追加一个文本 input，`placeholder` 属性使用 `q.placeholder || ''`，`name` 为 `askuser_custom_${i}`
- [x] 修改 `onSubmit` 中 answers 构建逻辑：
  - key 改为 `q.tool_call_id || q.description || q.question || `q${i}``
  - 若有 custom input 且有值，追加到 selected 结果中（如 `selected.push(customInput.value)`）
- [x] 同步修改 `app.js`（旧的 `showAskUserDialog` 函数）中相同的字段读取逻辑，避免旧代码仍读 `q.question`

**检查步骤:**
- [x] 验证 dialog.js 读取 description 字段
  - `grep -n 'q\.description' rust-relay-server/web/js/dialog.js`
  - 预期: 至少 1 行
- [x] 验证 allow_custom_input 处理存在
  - `grep -n 'allow_custom_input' rust-relay-server/web/js/dialog.js`
  - 预期: 至少 1 行
- [x] 验证提交时使用 tool_call_id 作为 key
  - `grep -n 'tool_call_id' rust-relay-server/web/js/dialog.js`
  - 预期: 至少 1 行
- [x] 验证选项 description 副标题渲染存在
  - `grep -n 'opt\.description' rust-relay-server/web/js/dialog.js`
  - 预期: 至少 1 行

---

### Task 5: Web 停止按钮

**涉及文件:**
- 修改: `rust-relay-server/web/js/render.js`

**执行步骤:**
- [x] 在文件顶部的 import 行中，确认 `closeDialog` 已从 `./dialog.js` 导入（当前已导入）；确认 `sendMessage` 已从 `./connection.js` 导入（当前已导入）
- [x] 在 `renderMessages` 函数内 `if (agent.isRunning)` 块，将 loading 气泡的 `innerHTML` 修改为：
  ```html
  <div class="loading-dots"><span></span><span></span><span></span></div>
  <button class="stop-btn">■ 停止</button>
  ```
  并在设置 innerHTML 之后（不能使用 innerHTML 事件绑定），用 `loadingEl.querySelector('.stop-btn').addEventListener('click', ...)` 绑定事件：
  ```javascript
  sendMessage(sessionId, { type: 'cancel_agent' });
  closeDialog('askuser');
  closeDialog('hitl');
  ```
  注意 `sessionId` 需从外层闭包获取；`renderMessages` 函数当前签名为 `(paneId, agent)`，需从 `state.layout.panes[paneId]` 获取 `sessionId`

**检查步骤:**
- [x] 验证停止按钮 HTML 存在
  - `grep -n 'stop-btn' rust-relay-server/web/js/render.js`
  - 预期: 至少 2 行（innerHTML 中的 class 和 querySelector）
- [x] 验证 cancel_agent 发送存在
  - `grep -n 'cancel_agent' rust-relay-server/web/js/render.js`
  - 预期: 至少 1 行
- [x] 验证 stop-btn 有 CSS 样式（在 style.css 中）
  - `grep -n 'stop-btn' rust-relay-server/web/style.css`
  - 预期: 至少 1 行（如果 style.css 无此样式，需在此步骤内联添加基本样式）
- [x] Relay Server 构建通过（含嵌入前端文件）
  - `cargo build -p rust-relay-server --features server 2>&1 | tail -5`
  - 预期: 无 `error`

---

### Task 6: web-ask-user-interrupt 验收

**前置条件:**
- 启动命令: `cargo run -p rust-relay-server --features server`（默认 :8080）
- 启动 TUI: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token <token> --relay-name test`
- 浏览器打开: `http://localhost:8080/web/?token=<token>`

**端到端验证:**

1. **协议字段完整性**：TUI 侧触发一次 ask_user 工具调用后，检查 Relay 转发的 JSON
   - `cargo test -p rust-relay-server -- test_ask_user_question_serialization test_cancel_agent_serialization --nocapture 2>&1 | tail -15`
   - Expected: 两个测试均 `ok` ✅ (2 passed)
   - On failure: 检查 Task 1 协议结构体定义

2. **AskUser 弹窗渲染**：在 TUI 使用 ask_user 工具（带 multi_select=true 和 options），Web 端接收后弹窗应显示 checkbox 而非 radio
   - `grep -c 'multi_select' rust-relay-server/web/js/dialog.js`
   - Expected: 输出 >= 1（前端有读取 multi_select 字段）✅ (1)
   - On failure: 检查 Task 4 弹窗渲染逻辑

3. **CancelAgent 协议解析**：
   - `echo '{"type":"cancel_agent"}' | cargo test -p rust-relay-server -- test_cancel_agent_serialization --nocapture 2>&1 | tail -5`
   - Expected: `test test_cancel_agent_serialization ... ok` ✅
   - On failure: 检查 Task 1 WebMessage::CancelAgent 定义

4. **停止按钮存在且绑定 cancel_agent**：
   - `grep -c 'cancel_agent' rust-relay-server/web/js/render.js`
   - Expected: 输出 >= 1 ✅ (1)
   - `grep -c 'stop-btn' rust-relay-server/web/js/render.js`
   - Expected: 输出 >= 2（innerHTML 和 querySelector 各一处）✅ (2)
   - On failure: 检查 Task 5 render.js 修改

5. **全量编译无错误**：
   - `cargo build 2>&1 | grep -c '^error'`
   - Expected: 输出 `0` ✅ (0)
   - On failure: 根据错误信息定位至对应 Task
