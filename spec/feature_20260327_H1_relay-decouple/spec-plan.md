# relay-server 协议层解耦 执行计划

**目标:** 消除 rust-relay-server 对 rust-create-agent 内部类型的直接依赖，由 rust-agent-tui 负责类型转换

**技术栈:** Rust、serde_json、tokio-tungstenite

**设计文档:** ../../Plan-H1-relay-decouple.md

---

### Task 1: 新建 protocol_types.rs（独立协议类型）

**涉及文件:**
- 新建: `rust-relay-server/src/protocol_types.rs`
- 修改: `rust-relay-server/src/lib.rs`
- 修改: `rust-relay-server/src/protocol.rs`

**执行步骤:**
- [x] 新建 `protocol_types.rs`，定义 `RelayAgentEvent` 枚举，涵盖 `AiReasoning`、`TextChunk`、`ToolStart`、`ToolEnd`、`StepDone`、`MessageAdded`、`LlmCallStart`、`LlmCallEnd` 变体
  - 所有字段使用基本类型（`String`、`bool`、`usize`、`serde_json::Value`），不引用 `rust-create-agent` 类型
  - serde 配置 `#[serde(tag = "type", rename_all = "snake_case")]`，保证与前端现有 JSON 解析兼容
- [x] 在 `rust-relay-server/src/lib.rs` 添加 `pub mod protocol_types;` 导出，使 TUI crate 可引用
- [x] 删除 `protocol.rs` 中废弃的 `RelayMessage::AgentEvent` 变体（已标注 deprecated，前端使用 `MessageBatch` 替代）
  - 同步移除该变体对 `rust_create_agent::agent::AgentEvent` 的 `use` 引用

**检查步骤:**
- [x] relay-server 编译无 rust-create-agent 残留引用（Task 2 完成后联合验证）
  - `grep -r "rust_create_agent" rust-relay-server/src/`
  - 预期: 无输出
- [x] protocol_types 模块可被外部 crate 引用
  - `cargo check -p rust-relay-server`
  - 预期: 编译通过，无错误

---

### Task 2: 更新 RelayClient（消除对 rust-create-agent 的直接引用）

**涉及文件:**
- 修改: `rust-relay-server/src/client/mod.rs`
- 修改: `rust-relay-server/Cargo.toml`

**执行步骤:**
- [x] 修改 `send_agent_event` 签名：入参从 `&rust_create_agent::agent::AgentEvent` 改为 `&RelayAgentEvent`
  - 内部直接调用 `serde_json::to_value(event)` 序列化，通过 `send_with_seq` 发送
- [x] 修改 `send_message` 签名：入参从 `&rust_create_agent::messages::BaseMessage` 改为 `&serde_json::Value`
  - 序列化职责移至调用方（rust-agent-tui），relay-server 不再感知 BaseMessage 类型
  - 调整内部实现：直接将传入的 Value 包装为 `MessageBatch` 格式发送
- [x] 从 `rust-relay-server/Cargo.toml` 移除 `rust-create-agent = { path = "../rust-create-agent" }` 依赖

**检查步骤:**
- [x] Cargo.toml 无 rust-create-agent 路径引用
  - `grep "rust-create-agent" rust-relay-server/Cargo.toml`
  - 预期: 无输出
- [x] relay-server 全量无 rust_create_agent 引用
  - `grep -r "rust_create_agent" rust-relay-server/src/`
  - 预期: 无输出
- [x] relay-server 独立编译通过
  - `cargo check -p rust-relay-server`
  - 预期: 编译通过，无错误

---

### Task 3: TUI 侧适配器与调用更新

**涉及文件:**
- 新建: `rust-agent-tui/src/relay_adapter.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/main.rs`（或 `lib.rs`，视模块注册位置而定）

**执行步骤:**
- [x] 新建 `relay_adapter.rs`，实现 `pub fn to_relay_event(event: &ExecutorEvent) -> Option<RelayAgentEvent>`
  - 逐一映射每个 `ExecutorEvent` 变体到对应的 `RelayAgentEvent`
  - `StateSnapshot` 返回 `None`（不转发到 relay，避免大量历史数据推送）
  - `MessageAdded(msg)` 映射为 `RelayAgentEvent::MessageAdded { message: serde_json::to_value(msg).unwrap_or(Value::Null) }`
- [x] 在 TUI crate 模块入口注册 `mod relay_adapter;`
- [x] 更新 `agent.rs` 事件回调中的 relay 转发逻辑：
  - `MessageAdded(msg)` → `relay.send_message(&serde_json::to_value(msg).unwrap_or_default())`
  - 其他变体 → `if let Some(ev) = relay_adapter::to_relay_event(&event) { relay.send_agent_event(&ev); }`
  - 移除旧的 `relay.send_agent_event(&event)` 直接调用

**检查步骤:**
- [x] TUI crate 编译通过
  - `cargo check -p rust-agent-tui`
  - 预期: 无编译错误
- [x] StateSnapshot 在适配器中明确返回 None
  - `grep -n "StateSnapshot" rust-agent-tui/src/relay_adapter.rs`
  - 预期: 输出包含 `None` 返回路径
- [x] 全量构建无错误
  - `cargo build`
  - 预期: 所有 crate 编译通过

---

### Task 4: relay-server 协议层解耦 Acceptance

**Prerequisites:**
- Start relay-server: `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server`
- Start TUI with relay: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token test-token`
- 需要有效的 `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`

**End-to-end verification:**

1. [x] relay-server 源码中无任何 rust-create-agent 引用
   - `grep -r "rust_create_agent" rust-relay-server/src/`
   - Expected: 无输出（零引用）
   - On failure: check Task 1（protocol.rs 废弃变体未清理）或 Task 2（client.rs 未完全替换）

2. [x] relay-server Cargo.toml 无路径依赖
   - `grep "rust-create-agent" rust-relay-server/Cargo.toml`
   - Expected: 无输出
   - On failure: check Task 2（依赖项未移除）

3. [ ] TUI 连接 relay 后工具事件正常转发
   - 启动 relay-server 和 TUI 后，通过 relay 前端（`http://localhost:8080/web/`）发送一条需要调用工具的任务
   - Expected: 前端 MessageList 中能看到 ToolStart/ToolEnd 卡片正常渲染
   - On failure: check Task 3（relay_adapter 变体映射遗漏）

4. [x] MessageAdded 事件以正确 JSON 格式转发
   - 触发一次完整对话（含 AI 回复），观察 relay 前端收到的消息
   - `grep -n "send_message" rust-agent-tui/src/app/agent.rs`
   - Expected: 输出包含 `serde_json::to_value` 调用，send_message 传递 Value 类型
   - On failure: check Task 3（send_message 调用签名不匹配）

5. [x] StateSnapshot 不出现在 relay 推送中
   - 检查适配器过滤逻辑
   - `grep -A3 "StateSnapshot" rust-agent-tui/src/relay_adapter.rs`
   - Expected: 输出显示 `return None` 或匹配臂返回 `None`
   - On failure: check Task 3（relay_adapter.rs 未处理 StateSnapshot）
