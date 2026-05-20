# Task 2: Transport & Protocol 层

## 背景
ACP 协议的核心是 JSON-RPC 2.0 双向通信，需要抽象 transport 层以支持内存通道（TUI 对接）和 stdio（IDE 对接）。当前 peri-tui 中 ACP 直接使用 agent-client-protocol 的 Stdio transport，无内存通道抽象。

## 执行步骤
- [ ] 定义核心类型 `peri-acp/src/transport/types.rs`: IncomingMessage(Request/Notification/Response), AcpError, RequestId
- [ ] 定义 `AcpTransport` trait `peri-acp/src/transport/mod.rs`: send_request(), send_notification(), recv()
- [ ] 实现 `MpscTransport` `peri-acp/src/transport/mpsc.rs`:
  - MpscClientTransport (TUI 端) + MpscServerTransport (ACP 端)
  - 两对 unbounded_channel + oneshot response
  - mpsc_transport_pair() 工厂
- [ ] 实现 `StdioTransport` `peri-acp/src/transport/stdio.rs`——复用 agent_client_protocol_tokio::Stdio
- [ ] 更新 `peri-acp/src/transport/mod.rs`: pub mod types/mpsc/stdio + re-export
- [ ] 单元测试: transport::mpsc 3 场景 (request-response, notification, bidirectional)
- [ ] 验证: `cargo test -p peri-acp -- transport`

## 涉及文件
- 新建: `peri-acp/src/transport/types.rs`, `peri-acp/src/transport/mpsc.rs`, `peri-acp/src/transport/stdio.rs`
- 修改: `peri-acp/src/transport/mod.rs`
