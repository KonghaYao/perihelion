# relay-server 协议层解耦 人工验收清单

**生成时间:** 2026-03-28 00:00
**关联计划:** spec-plan.md
**关联设计:** ../../Plan-H1-relay-decouple.md（见根目录，已随执行归档）

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 检查有效的 LLM API Key 已设置（运行时端到端验证所需）: `env | grep -E "ANTHROPIC_API_KEY|OPENAI_API_KEY" | head -1`

### 说明

- 场景 1–3 为纯静态验证，无需启动服务，直接运行命令即可
- 场景 4 需要启动 relay-server，并由人工操作 TUI + 浏览器完成端到端验证

---

## 验收项目

### 场景 1：代码结构解耦验证

#### - [x] 1.1 relay-server 源码无 rust_create_agent 引用

- **来源:** Task 1/2 检查步骤
- **操作步骤:**
  1. [A] `grep -r "rust_create_agent" rust-relay-server/src/` → 期望: 无任何输出（零匹配）
  2. [A] `grep "rust_create_agent" rust-relay-server/Cargo.toml` → 期望: 无任何输出
- **异常排查:**
  - 如果出现匹配：检查 `rust-relay-server/src/protocol.rs` 是否仍有 `AgentEvent` 废弃变体残留，或 `client/mod.rs` 是否遗漏了类型替换

#### - [x] 1.2 Cargo.toml 依赖正确配置

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep "rust-create-agent" rust-relay-server/Cargo.toml` → 期望: 无任何输出（依赖已完全移除）
  2. [A] `grep "tracing-subscriber" rust-relay-server/Cargo.toml` → 期望: 输出包含 `env-filter`（因移除 rust-create-agent 后需显式声明此 feature）
- **异常排查:**
  - 如果第 1 步有输出：手动删除 `rust-create-agent = { path = "../rust-create-agent" }` 行
  - 如果第 2 步无 env-filter：在 Cargo.toml 中将 `tracing-subscriber = "0.3"` 改为 `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`

#### - [x] 1.3 TUI 适配器路径正确

- **来源:** Task 3 执行步骤
- **操作步骤:**
  1. [A] `grep -n "send_message" rust-agent-tui/src/app/agent.rs` → 期望: 输出行包含 `serde_json::to_value(msg)`
  2. [A] `grep -n "relay_adapter::to_relay_event" rust-agent-tui/src/app/agent.rs` → 期望: 至少有 1 行匹配
  3. [A] `grep -n "StateSnapshot" rust-agent-tui/src/relay_adapter.rs` → 期望: 输出包含 `return None` 路径
- **异常排查:**
  - 如果 `send_message` 调用无 `serde_json::to_value`：说明 agent.rs 未正确更新调用签名
  - 如果 `relay_adapter::to_relay_event` 无输出：说明 agent.rs 中仍使用旧的 `send_agent_event(&event)` 直接调用

---

### 场景 2：编译与构建验证

#### - [x] 2.1 relay-server 独立编译

- **来源:** Task 1/2 检查步骤
- **操作步骤:**
  1. [A] `cargo check -p rust-relay-server 2>&1 | tail -5` → 期望: 最后几行包含 `Finished`，无 `error[` 字样
- **异常排查:**
  - 如果出现编译错误：查看错误行，通常是类型不匹配（`AgentEvent` vs `RelayAgentEvent`）或 `rust_create_agent` 残留引用

#### - [x] 2.2 TUI crate 编译

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo check -p rust-agent-tui 2>&1 | tail -5` → 期望: 最后几行包含 `Finished`，无 `error[` 字样
- **异常排查:**
  - 如果出现 `method to_string` 错误：检查 `relay_adapter.rs` 中 `message_id` 转换是否使用 `.as_uuid().to_string()`
  - 如果出现 `send_message` 类型不匹配：检查调用处是否传 `&serde_json::Value`

#### - [x] 2.3 全量构建

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望: 包含 `Finished`，无 `error[` 字样
- **异常排查:**
  - 如果全量构建失败但单 crate 通过：检查 workspace 依赖关系中是否有 relay-server 依赖项顺序问题

---

### 场景 3：协议类型格式验证

#### - [x] 3.1 RelayAgentEvent serde 格式符合前端期望

- **来源:** 设计文档风险点（序列化兼容性）
- **操作步骤:**
  1. [A] `grep -B1 "pub enum RelayAgentEvent" rust-relay-server/src/protocol_types.rs` → 期望: 包含 `#[serde(tag = "type", rename_all = "snake_case")]`
  2. [A] `grep -E "TextChunk|ToolStart|ToolEnd|StepDone|LlmCallStart|LlmCallEnd|AiReasoning" rust-relay-server/src/protocol_types.rs` → 期望: 7 个变体均有匹配
- **异常排查:**
  - 如果 serde 注解不正确：前端将无法解析 `type` 字段，检查 `protocol_types.rs` 顶部的 derive 宏

#### - [x] 3.2 protocol_types 模块公开导出

- **来源:** Task 1 执行步骤
- **操作步骤:**
  1. [A] `grep "pub mod protocol_types" rust-relay-server/src/lib.rs` → 期望: 输出该行，确认模块已导出（TUI crate 可通过 `rust_relay_server::protocol_types::RelayAgentEvent` 引用）
- **异常排查:**
  - 如果无输出：手动在 `lib.rs` 第一行添加 `pub mod protocol_types;`

---

### 场景 4：运行时端到端验证

#### - [x] 4.1 TUI 连接 relay 后工具事件正常转发到 Web 前端

- **来源:** Task 4 Acceptance 场景3
- **操作步骤:**
  1. [AUTO/SERVICE] 在后台启动 relay-server: `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server` (port: 8080)
  2. [H] 在另一个终端启动 TUI 并连接 relay：运行 `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token test-token`。等待 TUI 完全启动后，观察 TUI 界面左下角或状态栏是否显示 relay 已连接的提示（无报错信息） → 是/否
  3. [H] 打开浏览器访问 `http://localhost:8080/web/`，页面加载完成后，观察左侧 Agent 列表中是否出现刚刚连接的 TUI 实例（显示连接时间或名称） → 是/否
  4. [H] 点击 Agent 实例进入会话页面，在底部输入框输入 `列出当前目录下的文件` 并发送。等待 Agent 完成响应，观察 Web 前端消息列表中是否出现工具调用卡片（显示 `bash` 或 `glob_files` 等工具名称，以及执行结果） → 是/否
  5. [H] 检查工具卡片的展示内容是否正常：工具名称可读（非 `[object Object]`），输入参数和输出结果能正常显示，无 JSON 解析错误或乱码 → 是/否
- **异常排查:**
  - 如果步骤2 TUI 连接失败：确认 relay-server 已启动（`curl http://localhost:8080/agents`）且 token 一致
  - 如果步骤4 工具卡片不显示：检查 `relay_adapter.rs` 中 `ToolStart`/`ToolEnd` 变体映射是否完整
  - 如果工具卡片字段显示异常：前端可能使用旧的字段名（`tool_call_id` vs `toolCallId`），检查 `protocol_types.rs` 的 serde `rename_all` 配置

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景1 代码结构解耦 | 1.1 | relay-server 源码无 rust_create_agent 引用 | 2 | 0 | ✅ | |
| 场景1 | 1.2 | Cargo.toml 依赖正确配置 | 2 | 0 | ✅ | |
| 场景1 | 1.3 | TUI 适配器路径正确 | 3 | 0 | ✅ | |
| 场景2 编译与构建 | 2.1 | relay-server 独立编译 | 1 | 0 | ✅ | |
| 场景2 | 2.2 | TUI crate 编译 | 1 | 0 | ✅ | |
| 场景2 | 2.3 | 全量构建 | 1 | 0 | ✅ | |
| 场景3 协议类型格式 | 3.1 | RelayAgentEvent serde 格式 | 2 | 0 | ✅ | |
| 场景3 | 3.2 | protocol_types 模块公开导出 | 1 | 0 | ✅ | |
| 场景4 运行时端到端 | 4.1 | TUI 连接 relay 工具事件转发 | 1 | 4 | ✅ | |

**验收结论:** ✅ 全部通过
