# Task 4: 事件 & 交互 & Langfuse

## 背景
事件映射（ExecutorEvent → SessionUpdate）、权限桥接（RequestPermission RPC）、Langfuse 追踪是 ACP 服务层的三个关键横切关注点。当前 event_mapper、AcpInteractionBroker、LangfuseTracer 都在 peri-tui 中，需迁移到 peri-acp 并适配新 Transport 抽象。

## 执行步骤
- [ ] 迁移事件映射到 `peri-acp/src/event/mapper.rs`
  - 从 `peri-tui/src/acp/event_mapper.rs:112-200` 迁移 map_executor_to_updates()
  - 辅助函数 infer_tool_kind, truncate_str 一并迁移
- [ ] 实现 `AcpTransportBroker` 在 `peri-acp/src/broker/transport_broker.rs`
  - 实现 UserInteractionBroker trait
  - Approval → transport.send_request("RequestPermission", ...)
  - Questions → transport.send_request("elicitation/create", ...)
  - 需启用 unstable_elicitation feature
- [ ] 更新 `peri-acp/src/broker/mod.rs`: pub mod transport_broker + re-export
- [ ] 迁移 Langfuse 到 `peri-acp/src/langfuse/`
  - config.rs, session.rs, tracer.rs 从 peri-tui/src/langfuse/ 直接复制
- [ ] 更新 `peri-acp/src/event/mod.rs`: pub mod mapper + re-export
- [ ] 单元测试: broker::transport_broker 5 场景 + event::mapper 4 场景
- [ ] 验证: `cargo test -p peri-acp -- broker event`

## 涉及文件
- 新建: `peri-acp/src/event/mapper.rs`, `peri-acp/src/broker/transport_broker.rs`, `peri-acp/src/langfuse/tracer.rs`, `peri-acp/src/langfuse/session.rs`, `peri-acp/src/langfuse/config.rs`
- 修改: `peri-acp/src/event/mod.rs`, `peri-acp/src/broker/mod.rs`, `peri-acp/src/langfuse/mod.rs`, `peri-acp/src/lib.rs`
