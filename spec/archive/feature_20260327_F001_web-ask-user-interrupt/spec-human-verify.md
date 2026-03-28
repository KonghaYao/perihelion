# web-ask-user-interrupt 人工验收清单

**生成时间:** 2026-03-27 (对话续接)
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 全量编译（含所有 crate）: `cargo build 2>&1 | grep -c '^error'`
- [ ] [AUTO/SERVICE] 启动 Relay Server: `cargo run -p rust-relay-server --features server` (port: 8080)
- [ ] [AUTO/SERVICE] 启动 TUI（连接 Relay）: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token test --relay-name verify-agent` (port: N/A)

### 测试数据准备
- [ ] [MANUAL] 准备一个带 `ask_user` 工具调用的 Agent 任务（例如：让 Agent 执行一个需要询问用户的步骤）
- [ ] [MANUAL] 浏览器打开: `http://localhost:8080/web/?token=test`，确认能看到 verify-agent 已上线

---

## 验收项目

### 场景 1：协议层扩展验证

#### - [x] 1.1 AskUserQuestion 结构体字段对齐
- **来源:** Task 1 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n 'question' rust-relay-server/src/protocol.rs` → 期望: 无 `question: String` 字段（旧字段已删除；仅允许在注释或变量名前缀中出现）
  2. [A] `grep -n 'tool_call_id\|description\|multi_select\|allow_custom_input\|placeholder' rust-relay-server/src/protocol.rs | grep -v '//'` → 期望: 至少 5 行，包含全部字段定义
  3. [A] `grep -n 'AskUserOption' rust-relay-server/src/protocol.rs` → 期望: 至少 2 行（struct 定义 + AskUserQuestion 中引用）
- **异常排查:**
  - 如果 `question` 字段仍存在: 检查 Task 1 `AskUserQuestion` 结构体定义，确认字段已重命名为 `description`

#### - [x] 1.2 CancelAgent 序列化与单元测试
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n 'CancelAgent' rust-relay-server/src/protocol.rs` → 期望: 至少 2 行（enum 变体声明 + 测试中引用）
  2. [A] `cargo test -p rust-relay-server -- test_ask_user_question_serialization test_cancel_agent_serialization 2>&1 | tail -10` → 期望: 输出包含 `2 passed`，无 `FAILED`
  3. [A] `cargo test -p rust-relay-server -- test_cancel_agent_serialization --nocapture 2>&1 | grep -E 'ok|FAILED'` → 期望: `test test_cancel_agent_serialization ... ok`
- **异常排查:**
  - 如果测试失败: 运行 `cargo test -p rust-relay-server -- test_cancel_agent_serialization --nocapture 2>&1` 查看完整错误输出；检查 `WebMessage::CancelAgent` 是否有 `rename_all = "snake_case"` 修饰

---

### 场景 2：TUI 数据层验证

#### - [x] 2.1 AskUserBatch 全字段发送映射
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -A 15 'AskUserBatch(req)' rust-agent-tui/src/app/agent_ops.rs | grep -E 'tool_call_id|allow_custom_input|placeholder'` → 期望: 3 行匹配（每个字段各一行）
  2. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望: 无 `error`，输出包含 `Finished` 或 `Compiling`
- **异常排查:**
  - 如果字段缺失: 检查 `agent_ops.rs` 中 `AskUserBatch(req)` 分支的 `serde_json::json!` 构建逻辑

#### - [x] 2.2 CancelAgent 中断处理
- **来源:** Task 3 检查步骤 + spec-design.md
- **操作步骤:**
  1. [A] `grep -n 'CancelAgent' rust-agent-tui/src/app/relay_ops.rs` → 期望: 至少 1 行（match 分支）
  2. [A] `grep -n 'tool_call_id' rust-agent-tui/src/app/relay_ops.rs` → 期望: 至少 1 行（AskUserResponse 匹配逻辑中使用）
  3. [A] `grep -n 'data\.description' rust-agent-tui/src/app/relay_ops.rs` → 期望: 无输出（旧的 description 匹配已改为 tool_call_id）
- **异常排查:**
  - 如果 `data.description` 仍存在: 说明 Task 3 的匹配键修改未完成，需检查 `AskUserResponse` 分支

---

### 场景 3：Web AskUser 弹窗增强

#### - [x] 3.1 代码层字段覆盖验证
- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -n 'q\.description' rust-relay-server/web/js/dialog.js` → 期望: 至少 1 行（问题标题读取）
  2. [A] `grep -n 'allow_custom_input' rust-relay-server/web/js/dialog.js` → 期望: 至少 1 行
  3. [A] `grep -n 'tool_call_id' rust-relay-server/web/js/dialog.js` → 期望: 至少 1 行（提交 key 构建）
  4. [A] `grep -n 'opt\.description' rust-relay-server/web/js/dialog.js` → 期望: 至少 1 行（选项副标题渲染）
- **异常排查:**
  - 如果 `q.description` 缺失: 检查 `showAskUserDialog` 函数中 `label.textContent =` 那一行

#### - [x] 3.2 UI 渲染视觉验收
- **来源:** Task 4 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -c 'multi_select' rust-relay-server/web/js/dialog.js` → 期望: 输出 >= 1（有读取 multi_select 字段）
  2. [H] 触发 TUI 执行 `ask_user` 工具（带 `multi_select: true` 参数），打开 `http://localhost:8080/web/?token=test`，观察弹窗选项是否使用 checkbox（方框）而非 radio（圆框）→ 是/否
  3. [H] 触发 TUI 执行带 `options` 且每个选项有 `description` 的 `ask_user`，观察弹窗中选项标题下方是否有灰色小字副标题显示 → 是/否
  4. [H] 触发 TUI 执行带 `allow_custom_input: true` 的 `ask_user`，观察弹窗选项区域下方是否出现文本输入框（带 placeholder 提示） → 是/否
- **异常排查:**
  - checkbox 显示为 radio: 检查 `radio.type = q.multi_select ? 'checkbox' : 'radio'` 逻辑
  - 副标题未显示: 检查 `opt.description` 判断及 `<div>` 灰色文字渲染代码

#### - [x] 3.3 提交逻辑正确性
- **来源:** Task 4 + spec-design.md
- **操作步骤:**
  1. [A] `grep -n 'tool_call_id.*description.*question\|q\.tool_call_id' rust-relay-server/web/js/dialog.js | head -5` → 期望: 至少 1 行包含 `tool_call_id` 优先的 key 构建表达式
  2. [H] 在 Web 弹窗中选择一个选项后点击提交，同时观察浏览器 DevTools → Network 面板中发出的 WebSocket 消息，检查 `ask_user_response` 消息的 `answers` 对象 key 是否为 `tool_call_id` 值（而非问题文本） → 是/否
- **异常排查:**
  - key 仍是问题文本: 检查 `dialog.js` `onSubmit` 函数中 `const key = q.tool_call_id || ...` 赋值逻辑

---

### 场景 4：Web 停止按钮

#### - [x] 4.1 代码与 CSS 层验证
- **来源:** Task 5 检查步骤
- **操作步骤:**
  1. [A] `grep -n 'stop-btn' rust-relay-server/web/js/render.js` → 期望: 至少 2 行（innerHTML 中的 class 和 querySelector）
  2. [A] `grep -n 'cancel_agent' rust-relay-server/web/js/render.js` → 期望: 至少 1 行（sendMessage 调用）
  3. [A] `grep -n 'stop-btn' rust-relay-server/web/style.css` → 期望: 至少 1 行（.stop-btn 样式声明）
- **异常排查:**
  - `stop-btn` 仅 1 行: 检查 `render.js` 中 `loadingEl.querySelector('.stop-btn').addEventListener` 是否存在
  - CSS 缺失: 在 `style.css` 中补充 `.stop-btn { ... }` 样式

#### - [x] 4.2 停止按钮 UI 行为验收
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 打开 `http://localhost:8080/web/?token=test`，向 Agent 发送一条需要较长时间执行的指令（如 `bash sleep 10`），观察消息列表底部是否出现带"■ 停止"文字的按钮 → 是/否
  2. [H] 在 Agent 运行中（loading 气泡可见），若同时有 AskUser 弹窗打开，点击"■ 停止"按钮，观察：(a) AskUser/HITL 弹窗是否立即关闭；(b) loading 气泡是否在 Agent 完成中止后消失；(c) Agent 是否停止执行（TUI 侧退出 ReAct 循环） → 是/否
- **异常排查:**
  - 停止按钮不显示: 检查 `agent.isRunning` 状态是否被正确设置，以及 `renderMessages` 中 `if (agent.isRunning)` 分支
  - 弹窗未关闭: 检查 `closeDialog('askuser')` 和 `closeDialog('hitl')` 是否在点击事件中被调用
  - Agent 未中止: 检查 TUI `relay_ops.rs` 的 `WebMessage::CancelAgent` 分支是否正确调用 `self.interrupt()`

---

### 场景 5：全量构建验证

#### - [x] 5.1 全量编译无错误
- **来源:** Task 6 验收 + Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -c '^error'` → 期望: 输出 `0`（无编译错误）
  2. [A] `cargo build -p rust-relay-server --features server 2>&1 | tail -5` → 期望: 无 `error`，包含 `Finished`
- **异常排查:**
  - 有编译错误: 运行 `cargo build 2>&1 | grep '^error'` 查看完整错误列表，根据 crate 名定位到对应 Task

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | AskUserQuestion 字段对齐 | 3 | 0 | ✅ | |
| 场景 1 | 1.2 | CancelAgent 序列化与测试 | 3 | 0 | ✅ | |
| 场景 2 | 2.1 | AskUserBatch 全字段发送 | 2 | 0 | ✅ | |
| 场景 2 | 2.2 | CancelAgent 中断处理 | 3 | 0 | ✅ | |
| 场景 3 | 3.1 | 代码层字段覆盖验证 | 4 | 0 | ✅ | |
| 场景 3 | 3.2 | UI 渲染视觉验收 | 1 | 3 | ✅ | 修复后通过：events.js 补充 showAskUserDialog 调用 |
| 场景 3 | 3.3 | 提交逻辑正确性 | 1 | 1 | ✅ | |
| 场景 4 | 4.1 | 代码与 CSS 层验证 | 3 | 0 | ✅ | |
| 场景 4 | 4.2 | 停止按钮 UI 行为验收 | 0 | 2 | ✅ | |
| 场景 5 | 5.1 | 全量编译无错误 | 2 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
