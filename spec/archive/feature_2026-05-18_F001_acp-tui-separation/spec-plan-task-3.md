# Task 3: Session & Agent 核心

## 背景
SessionManager 管理 ACP session 生命周期，`build_bare_agent()` 构建完整的 ReActAgent 中间件链。这两大核心组件当前在 `peri-tui` 中且依赖 TUI 类型（LlmProvider, PeriConfig, AgentEvent），需迁移到 `peri-acp` 并去 TUI 化。

## 执行步骤
- [ ] 迁移 `LlmProvider` 到 `peri-acp/src/provider/mod.rs`
  - 从 `peri-tui/src/app/provider.rs` 复制全部代码
  - 内部 `PeriConfig` 引用改为 `crate::provider::config::PeriConfig`
- [ ] 迁移 `PeriConfig` 到 `peri-acp/src/provider/config.rs`
  - 从 `peri-tui/src/config/store.rs` 和 `config/types.rs` 迁移
- [ ] 迁移系统提示词到 `peri-acp/src/prompt/`
  - 从 `peri-tui/src/prompt.rs` 迁移 build_system_prompt, PromptFeatures 等
  - 符号链接 prompt sections: `peri-acp/src/prompt/sections/ → peri-tui/prompts/sections/`
  - 更新 include_str! 路径
- [ ] 迁移 SessionManager 到 `peri-acp/src/session/mod.rs`
  - 从 `peri-tui/src/acp/session.rs` 复制
  - 更新引用: LlmProvider → crate::provider::LlmProvider
- [ ] 迁移 `build_bare_agent()` 到 `peri-acp/src/agent/builder.rs`
  - 从 `peri-tui/src/app/agent.rs:96-419` 迁移
  - 定义 AcpAgentConfig（替代 BareAgentConfig），移除 TUI 专用字段
  - 保持 middleware chain 构建逻辑不变
- [ ] 更新 `peri-acp/src/agent/mod.rs`: pub mod builder + re-export
- [ ] 单元测试: agent::builder 2 场景 (minimal config, LSP integration)
- [ ] 验证: `cargo test -p peri-acp -- agent::builder`

## 涉及文件
- 新建: `peri-acp/src/agent/builder.rs`, `peri-acp/src/provider/mod.rs`, `peri-acp/src/prompt/mod.rs`, `peri-acp/src/prompt/features.rs`
- 修改: `peri-acp/src/session/mod.rs`, `peri-acp/src/agent/mod.rs`
