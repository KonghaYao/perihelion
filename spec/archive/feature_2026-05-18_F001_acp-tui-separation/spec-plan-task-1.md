# Task 1: peri-acp 脚手架

## 背景
当前 ACP 代码散落在 `peri-tui/src/acp/`，需要独立为 workspace crate 才能实现分层解耦。本 Task 新建 `peri-acp` crate 作为所有后续迁移步骤的容器。

## 执行步骤
- [ ] 创建 `peri-acp/Cargo.toml`
  - package: peri-acp, 0.1.0, edition 2021, lib
  - 依赖: peri-agent, peri-middlewares, peri-lsp, langfuse-client, agent-client-protocol (features: unstable, unstable_elicitation), agent-client-protocol-tokio, tokio, async-trait, serde, serde_json, parking_lot, tracing, anyhow, uuid, chrono, dashmap, dirs-next, thiserror
- [ ] 创建 `peri-acp/src/lib.rs`，声明 11 个模块: transport, session, agent, event, broker, langfuse, prompt, provider, hooks, lsp, dispatch
- [ ] 创建各模块占位 mod.rs，内容 `// TODO: migrate from peri-tui`
- [ ] 修改根 `Cargo.toml` workspace.members，添加 `"peri-acp"`
- [ ] 验证: `cargo build -p peri-acp`

## 涉及文件
- 新建: `peri-acp/Cargo.toml`, `peri-acp/src/lib.rs`, `peri-acp/src/transport/mod.rs`, `peri-acp/src/session/mod.rs`, `peri-acp/src/agent/mod.rs`, `peri-acp/src/event/mod.rs`, `peri-acp/src/broker/mod.rs`, `peri-acp/src/langfuse/mod.rs`, `peri-acp/src/prompt/mod.rs`, `peri-acp/src/provider/mod.rs`, `peri-acp/src/hooks/mod.rs`, `peri-acp/src/lsp/mod.rs`, `peri-acp/src/dispatch/mod.rs`
- 修改: 根 `Cargo.toml`
