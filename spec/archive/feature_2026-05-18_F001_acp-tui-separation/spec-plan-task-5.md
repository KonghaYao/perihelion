# Task 5: 中间件下沉 (LSP, Hooks)

## 背景
LSP 中间件和 Hooks 中间件当前在 TUI 路径中构建（`peri-tui/src/app/agent.rs:586-609`），ACP 路径中缺失。LSP 和 Hooks 是后端功能，应统一在 peri-acp 的 Agent 构建中组装。

## 执行步骤
- [ ] 在 `peri-acp/src/agent/builder.rs` 的 `build_agent()` 中集成 LSP 支持
  - AcpAgentConfig 新增字段: lsp_servers: Vec<LspServerConfig>
  - 非空时 add_middleware(LspMiddleware)
- [ ] 验证 Hooks 已通过 Task 3 的 build_bare_agent 迁移正确集成
- [ ] 更新 `peri-acp/src/lsp/mod.rs` 和 `peri-acp/src/hooks/mod.rs` 作为 re-export 包装层
- [ ] 从 `peri-tui/src/app/agent.rs` 删除 LSP 组装代码（第 586-609 行）
- [ ] 单元测试: agent::builder LSP 集成场景
- [ ] 验证: `cargo build -p peri-acp && cargo build -p peri-tui`

## 涉及文件
- 新建: `peri-acp/src/lsp/mod.rs`, `peri-acp/src/hooks/mod.rs`
- 修改: `peri-acp/src/agent/builder.rs`, `peri-tui/src/app/agent.rs`
