# Task 7: 清理 & 测试

## 背景
Task 1-6 完成后，peri-tui 中留有大量已迁移的旧代码。需移除 dead code，确保无编译警告，编写集成测试验证完整链路。

## 执行步骤
- [ ] 删除 `peri-tui/src/acp/` 整个目录
- [ ] 删除 `peri-tui/src/langfuse/` 整个目录
- [ ] 删除 `peri-tui/src/app/interaction_broker.rs`
- [ ] 清理 `peri-tui/src/app/agent.rs`
  - 删除 build_bare_agent() (L96-419), run_universal_agent() (L421-650), map_executor_event() (L657-815), compact_task() (L821-998)
  - 删除相关 use 语句
- [ ] 清理 `peri-tui/src/app/events.rs`——删除 AgentEvent 枚举
- [ ] 清理 TUI 中的残留引用
  - app/mod.rs: 移除 pub use events::AgentEvent, pub use interaction_broker
  - lib.rs: 移除 pub mod acp, pub mod langfuse
  - 全局搜索 use crate::acp:: 等 → 全部移除
- [ ] 迁移 main_acp.rs 到 peri-acp
  - 在 `peri-acp/Cargo.toml` 添加 [[bin]] section
  - 复制 run_acp_mode() 到 `peri-acp/src/main.rs`
  - 更新引用: crate::app::agent::LlmProvider → crate::provider::LlmProvider
- [ ] 编写集成测试 `peri-acp/tests/integration_test.rs`
  - test_full_prompt_cycle: session/new → prompt → TextChunk/ToolCall → Done
  - test_hitl_approval_flow: prompt 触发 Bash → RequestPermission → Respond → 继续
  - test_ask_user_flow: prompt 触发 AskUserQuestion → elicitation/create → Respond → 继续
  - test_model_switch: set_model → prompt → 新模型生效
  - test_cancel_request: prompt 中 → $/cancel_request → Cancelled
- [ ] 验证全局编译: `cargo build --workspace 2>&1 | grep -c "warning:"`
- [ ] 完整测试: `cargo test --workspace 2>&1 | tail -30`
- [ ] 更新 CLAUDE.md 认知

## 涉及文件
- 修改: `peri-tui/src/app/agent.rs`, `peri-tui/src/app/events.rs`, `peri-tui/src/lib.rs`, `peri-tui/src/app/mod.rs`
- 删除: `peri-tui/src/acp/`, `peri-tui/src/langfuse/`, `peri-tui/src/app/interaction_broker.rs`
- 新建: `peri-acp/tests/integration_test.rs`, `peri-acp/src/main.rs`
