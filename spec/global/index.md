# 项目全局 Spec 索引

![全局领域拓扑](./images/01-domain-topology.png)

## 项目概况
→ [overview.md](./overview.md) — 项目概述
→ [architecture.md](./architecture.md) — 架构全景
→ [features.md](./features.md) — 已有功能清单
→ [constraints.md](./constraints.md) — 架构约束

## 已归档 Feature

| Feature ID | 摘要 | 领域 | 归档日期 |
|-----------|------|------|----------|
| [20260324_F001_tui-clipboard-image-paste](../archive/feature_20260324_F001_tui-clipboard-image-paste/) | Ctrl+V 粘贴剪贴板图片作为多模态消息发送 | tui | 2026-03-24 |
| [20260324_F001_compact-context-command](../archive/feature_20260324_F001_compact-context-command/) | /compact 指令调用 LLM 将对话历史压缩为结构化摘要 | tui | 2026-03-24 |
| [20260323_F006_ws-event-sync](../archive/feature_20260323_F006_ws-event-sync/) | WebSocket 事件扁平化+seq序列号+会话 Sync 同步 | relay-server | 2026-03-24 |
| [20260323_F004_remote-control-access](../archive/feature_20260323_F004_remote-control-access/) | Relay Server + Web 前端实现远程访问控制本地 Agent | relay-server | 2026-03-24 |
| [20260323_F005_tui-bug-fixes](../archive/feature_20260323_F005_tui-bug-fixes/) | 修复弹窗滚动/粘贴换行/loading 输入锁死三个 TUI bug | tui | 2026-03-24 |
| [20260323_F001_model-alias-provider-mapping](../archive/feature_20260323_F001_model-alias-provider-mapping/) | Opus/Sonnet/Haiku 三级别名映射，支持 /model <alias> 快捷切换 | tui | 2026-03-24 |
| [20260323_F003_tui-status-panel](../archive/feature_20260323_F003_tui-status-panel/) | TODO 状态固定面板、工具调用颜色分层、路径参数缩短 | tui | 2026-03-24 |
| [20260323_F002_tui-headless-mode](../archive/feature_20260323_F002_tui-headless-mode/) | Headless 测试模式：TestBackend + 渲染线程零 sleep 同步 | tui | 2026-03-24 |
| [20260323_F001_tui-render-perf](../archive/feature_20260323_F001_tui-render-perf/) | 双线程渲染架构：独立渲染线程 + 按需重绘，消除消息多时卡顿 | tui | 2026-03-24 |
| [20260322_F002_data-pipeline-unification](../archive/feature_20260322_F002_data-pipeline-unification/) | 实时流式与历史恢复统一工具调用参数显示，含 tool_call_id 匹配 | tui | 2026-03-24 |
| [20260322_F001_message-render-refactor](../archive/feature_20260322_F001_message-render-refactor/) | MessageViewModel 中间层重构，tui-markdown 渲染，工具折叠 | tui | 2026-03-24 |
| [20260322_F001_agent-storage-refactor](../archive/feature_20260322_F001_agent-storage-refactor/) | SQLite WAL 持久化替代 JSONL，MessageAdapter 双向转换 | agent | 2026-03-24 |
| [20260321_F001_subagents-execution](../archive/feature_20260321_F001_subagents-execution/) | launch_agent 工具支持子 Agent 委派，防递归，工具过滤 | agent | 2026-03-24 |

## 领域索引

- [agent](./domains/agent.md) — Agent 核心（ReAct 执行器、消息系统、工具抽象、持久化）
- [tui](./domains/tui.md) — TUI 界面（渲染、交互、命令、面板）
- [relay-server](./domains/relay-server.md) — Relay Server（WebSocket 中继、远程控制）

---
*最后更新: 2026-03-24 — 由批量归档更新*
