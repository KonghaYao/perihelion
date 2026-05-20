# Task 6: TUI 改造

## 背景
TUI 当前直接依赖 peri-agent/peri-middlewares，需要改为通过 ACP 协议（MpscTransport）驱动 Agent，实现分层解耦：TUI 成为纯 ACP client 前端。

## 执行步骤
- [ ] 创建 `AcpTuiClient` 封装层 `peri-tui/src/acp_client/client.rs`
  - 持有 MpscClientTransport
  - 高层方法: new_session, prompt, set_model, set_mode, cancel, list_sessions, load_session
  - SessionNotification 流通过 channel 转发给 TUI 事件循环
- [ ] 更新 `peri-tui/Cargo.toml`
  - 移除依赖: peri-agent, peri-middlewares, peri-lsp, langfuse-client
  - 添加依赖: peri-acp
- [ ] 重构 `peri-tui/src/main.rs` TUI 启动逻辑
  - 创建 MpscTransport pair → ACP Server task + AcpTuiClient
  - 将 AcpTuiClient 传入 App::new()
- [ ] 重构 `peri-tui/src/app/mod.rs` App 结构体
  - 移除 agent_tx/agent_rx，新增 acp_client
  - 移除 langfuse session/tracer 字段
- [ ] 重构事件处理 `peri-tui/src/app/agent_ops.rs`
  - 新增 handle_session_notification() 消费 SessionUpdate
  - 映射: AgentMessageChunk→追加文本, ToolCall→创建VM, ToolCallUpdate→更新, UsageUpdate→token
- [ ] 重构 HITL 审批 `peri-tui/src/app/hitl_ops.rs`
  - 审批通过 IncomingMessage::Request("RequestPermission") 接收
  - 弹窗后 transport.send_response(id, decision) 回复
- [ ] 重构 AskUser 询问 `peri-tui/src/app/ask_user_ops.rs`
  - 通过 IncomingMessage::Request("elicitation/create") 接收
  - 弹窗后 transport.send_response(id, answers) 回复
- [ ] 重构命令系统 `peri-tui/src/command/mod.rs`
  - /model → acp_client.set_model(), Shift+Tab → acp_client.set_mode()
  - /history → acp_client.list_sessions(), Ctrl+C → acp_client.cancel()
- [ ] 重构 Agent 提交 `peri-tui/src/app/agent_submit.rs`
  - submit_message() → acp_client.prompt(session_id, text) + poll SessionNotification
- [ ] 更新 `peri-tui/src/lib.rs`: 添加 pub mod acp_client
- [ ] 单元测试: acp_client 2 场景 (new_session, prompt_echo)
- [ ] 验证: `cargo build -p peri-tui`，确认 grep 无 peri-agent/peri-middlewares in Cargo.toml

## 涉及文件
- 新建: `peri-tui/src/acp_client/mod.rs`, `peri-tui/src/acp_client/client.rs`
- 修改: `peri-tui/Cargo.toml`, `peri-tui/src/main.rs`, `peri-tui/src/app/mod.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/agent_submit.rs`, `peri-tui/src/command/mod.rs`, `peri-tui/src/command/model.rs`, `peri-tui/src/event.rs`, `peri-tui/src/app/hitl_ops.rs`, `peri-tui/src/app/ask_user_ops.rs`, `peri-tui/src/lib.rs`
