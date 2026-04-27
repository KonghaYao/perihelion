# Relay Server 移除 人工验收清单

**生成时间:** 2026-04-27
**关联计划:** spec/feature_20260427_F001_relay-removal/spec-plan.md
**关联设计:** spec/feature_20260427_F001_relay-removal/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version`
- [x] [AUTO] 编译全量 workspace: `cargo build 2>&1 | tail -5`
- [x] [AUTO] 运行全量测试: `cargo test 2>&1 | tail -10`

### 测试数据准备
- 无需额外测试数据（纯删除重构）

---

## 验收项目

### 场景 1: rust-relay-server crate 完整移除

#### - [x] 1.1 crate 目录已删除
- **来源:** spec-plan.md Task 1 / spec-design.md §删除范围清单
- **目的:** 确认 relay-server 目录不存在
- **操作步骤:**
  1. [A] `test ! -d rust-relay-server && echo "OK: directory removed"` → 期望精确: `OK: directory removed`

#### - [x] 1.2 workspace members 不包含 rust-relay-server
- **来源:** spec-plan.md Task 1
- **目的:** 确认根 Cargo.toml 已移除该成员
- **操作步骤:**
  1. [A] `grep '"rust-relay-server"' Cargo.toml` → 期望包含: (无输出，grep 返回非 0)

#### - [x] 1.3 workspace 保留其余 5 个 members
- **来源:** spec-plan.md Task 1
- **目的:** 确认仅移除了 relay-server，其余 crate 正常
- **操作步骤:**
  1. [A] `grep -c '"rust-' Cargo.toml` → 期望精确: `3`

#### - [x] 1.4 根 Cargo.toml 语法有效
- **来源:** spec-plan.md Task 1
- **目的:** 确认配置文件可被 Cargo 正确解析
- **操作步骤:**
  1. [A] `cargo metadata --format-version=1 --no-deps 2>&1 | head -1` → 期望包含: `{`

---

### 场景 2: TUI Relay 专用文件清除

#### - [x] 2.1 6 个 Relay 专用文件已删除
- **来源:** spec-plan.md Task 2 / spec-design.md §删除 TUI 中的 Relay 专用文件
- **目的:** 确认所有 Relay 专用源文件已移除
- **操作步骤:**
  1. [A] `for f in rust-agent-tui/src/app/relay_panel.rs rust-agent-tui/src/app/relay_ops.rs rust-agent-tui/src/app/relay_state.rs rust-agent-tui/src/relay_adapter.rs rust-agent-tui/src/ui/main_ui/panels/relay.rs rust-agent-tui/src/command/relay.rs; do test ! -f "$f" && echo "OK" || echo "FAIL: $f"; done` → 期望包含: `OK` × 6 行

#### - [x] 2.2 app 目录无 relay_ 前缀文件
- **来源:** spec-plan.md Task 2
- **目的:** 确认 app 层无遗漏的 relay 文件
- **操作步骤:**
  1. [A] `ls rust-agent-tui/src/app/relay_*.rs 2>&1` → 期望包含: `No match found` 或 ls 报错

#### - [x] 2.3 relay_adapter.rs 和 relay 面板/命令文件不存在
- **来源:** spec-plan.md Task 2
- **目的:** 确认非 app 层的 relay 文件也已删除
- **操作步骤:**
  1. [A] `test ! -f rust-agent-tui/src/relay_adapter.rs && echo "OK"` → 期望精确: `OK`
  2. [A] `test ! -f rust-agent-tui/src/ui/main_ui/panels/relay.rs && echo "OK"` → 期望精确: `OK`
  3. [A] `test ! -f rust-agent-tui/src/command/relay.rs && echo "OK"` → 期望精确: `OK`

---

### 场景 3: TUI App 层 Relay 集成清除

#### - [x] 3.1 Cargo.toml 不再依赖 rust-relay-server
- **来源:** spec-plan.md Task 3
- **目的:** 确认 TUI 依赖中无 relay-server
- **操作步骤:**
  1. [A] `grep "rust-relay-server" rust-agent-tui/Cargo.toml` → 期望包含: (无输出)

#### - [x] 3.2 lib.rs 不再包含 RelayCli 和 relay_adapter
- **来源:** spec-plan.md Task 3
- **目的:** 确认公共 API 中无 relay 导出
- **操作步骤:**
  1. [A] `grep -c "relay_adapter\|RelayCli\|parse_relay_args" rust-agent-tui/src/lib.rs` → 期望精确: `0`

#### - [x] 3.3 app/mod.rs 不再包含 relay 相关声明和字段
- **来源:** spec-plan.md Task 3
- **目的:** 确认 App 结构体和模块声明中无 relay 残留
- **操作步骤:**
  1. [A] `grep -c "relay_panel\|relay_state\|relay_ops\|RelayPanel\|RelayState\|try_connect_relay" rust-agent-tui/src/app/mod.rs` → 期望精确: `0`

#### - [x] 3.4 events.rs 不再包含 MessageAdded 变体
- **来源:** spec-plan.md Task 3
- **目的:** 确认 AgentEvent 枚举中已移除 MessageAdded
- **操作步骤:**
  1. [A] `grep -c "MessageAdded" rust-agent-tui/src/app/events.rs` → 期望精确: `0`

#### - [x] 3.5 agent.rs 不再包含 relay_client 和 relay_adapter 引用
- **来源:** spec-plan.md Task 3
- **目的:** 确认 agent 执行逻辑中无 relay 转发
- **操作步骤:**
  1. [A] `grep -c "relay_client\|relay_adapter\|relay_for_handler" rust-agent-tui/src/app/agent.rs` → 期望精确: `0`

#### - [x] 3.6 agent_ops.rs 不再包含 relay 和 MessageAdded 引用
- **来源:** spec-plan.md Task 3
- **目的:** 确认事件处理中无 relay 转发和 MessageAdded 分支
- **操作步骤:**
  1. [A] `grep -c "relay_client\|MessageAdded" rust-agent-tui/src/app/agent_ops.rs` → 期望精确: `0`

#### - [x] 3.7 panel_ops.rs 不再包含 relay 面板操作方法
- **来源:** spec-plan.md Task 3
- **目的:** 确认面板操作中无 relay 方法
- **操作步骤:**
  1. [A] `grep -c "relay_panel\|open_relay\|close_relay\|RelayPanel\|relay:" rust-agent-tui/src/app/panel_ops.rs` → 期望精确: `0`

#### - [x] 3.8 hitl_ops.rs 不再包含 send_hitl_resolved 和 relay_client
- **来源:** spec-plan.md Task 3
- **目的:** 确认 HITL 审批流程中无 relay 通知
- **操作步骤:**
  1. [A] `grep -c "send_hitl_resolved\|relay_client" rust-agent-tui/src/app/hitl_ops.rs` → 期望精确: `0`

#### - [x] 3.9 ask_user_ops.rs 不再包含 relay_client
- **来源:** spec-plan.md Task 3
- **目的:** 确认 AskUser 流程中无 relay 转发
- **操作步骤:**
  1. [A] `grep -c "relay_client" rust-agent-tui/src/app/ask_user_ops.rs` → 期望精确: `0`

#### - [x] 3.10 thread_ops.rs 不再包含 relay_client 和 send_thread_reset
- **来源:** spec-plan.md Task 3
- **目的:** 确认线程操作中无 relay 通知
- **操作步骤:**
  1. [A] `grep -c "relay_client\|send_thread_reset" rust-agent-tui/src/app/thread_ops.rs` → 期望精确: `0`

---

### 场景 4: TUI UI/Event/Command/Config 层清除

#### - [x] 4.1 rust-agent-tui 编译成功
- **来源:** spec-plan.md Task 4
- **目的:** 确认所有 relay 残留清除后编译通过
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 4.2 event.rs 不再包含 relay_panel 和 handle_relay_panel
- **来源:** spec-plan.md Task 4
- **目的:** 确认事件处理层无 relay 按键处理
- **操作步骤:**
  1. [A] `grep -c "relay_panel\|handle_relay_panel\|RelayPanelMode" rust-agent-tui/src/event.rs` → 期望精确: `0`

#### - [x] 4.3 main_ui.rs 不再包含 relay 渲染分发
- **来源:** spec-plan.md Task 4
- **目的:** 确认 UI 渲染层无 relay 面板分发
- **操作步骤:**
  1. [A] `grep -c "relay_panel\|panels::relay" rust-agent-tui/src/ui/main_ui.rs` → 期望精确: `0`

#### - [x] 4.4 panels/mod.rs 不再包含 relay 模块声明
- **来源:** spec-plan.md Task 4
- **目的:** 确认面板模块注册中无 relay
- **操作步骤:**
  1. [A] `grep -c "relay" rust-agent-tui/src/ui/main_ui/panels/mod.rs` → 期望精确: `0`

#### - [x] 4.5 command/mod.rs 不再包含 relay 命令
- **来源:** spec-plan.md Task 4
- **目的:** 确认命令注册中无 relay
- **操作步骤:**
  1. [A] `grep -c "relay" rust-agent-tui/src/command/mod.rs` → 期望精确: `0`

#### - [x] 4.6 config/types.rs 不再包含 RemoteControlConfig
- **来源:** spec-plan.md Task 4 / spec-design.md §配置变更
- **目的:** 确认配置类型中无 RemoteControl
- **操作步骤:**
  1. [A] `grep -c "RemoteControlConfig\|remote_control" rust-agent-tui/src/config/types.rs` → 期望精确: `0`

#### - [x] 4.7 config/mod.rs 不再包含 RemoteControlConfig re-export
- **来源:** spec-plan.md Task 4
- **目的:** 确认配置模块导出中无 RemoteControl
- **操作步骤:**
  1. [A] `grep -c "RemoteControlConfig" rust-agent-tui/src/config/mod.rs` → 期望精确: `0`

#### - [x] 4.8 main.rs 不再包含任何 relay 相关引用
- **来源:** spec-plan.md Task 4
- **目的:** 确认主入口中无 CLI 参数、初始化、轮询逻辑
- **操作步骤:**
  1. [A] `grep -c "relay_cli\|parse_relay_args\|RelayCli\|poll_relay\|check_relay_reconnect\|try_connect_relay\|relay_updated" rust-agent-tui/src/main.rs` → 期望精确: `0`

#### - [x] 4.9 headless.rs 不再包含 MessageAdded
- **来源:** spec-plan.md Task 4
- **目的:** 确认 headless 测试无已删除事件引用
- **操作步骤:**
  1. [A] `grep -c "MessageAdded" rust-agent-tui/src/ui/headless.rs` → 期望精确: `0`

#### - [x] 4.10 rust-agent-tui 测试全部通过
- **来源:** spec-plan.md Task 4
- **目的:** 确认测试无回归
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 5: 全局文档同步更新

#### - [x] 5.1 architecture.md 不再包含 Relay 引用
- **来源:** spec-plan.md Task 5 / spec-design.md §全局文档更新
- **目的:** 确认架构文档与代码状态一致
- **操作步骤:**
  1. [A] `grep -ic "rust-relay-server\|relay_server\|RelayState\|relay-server\|Relay 双向\|RelayClient\|remote_control\|RemoteControl" spec/global/architecture.md` → 期望精确: `0`

#### - [x] 5.2 constraints.md 不再包含 Relay 相关技术栈
- **来源:** spec-plan.md Task 5 / spec-design.md §技术栈变更
- **目的:** 确认约束文档中无 axum/preact 等 relay 技术栈
- **操作步骤:**
  1. [A] `grep -ic "relay\|axum\|esm.sh\|preact\|useSignalValue\|tungstenite" spec/global/constraints.md` → 期望精确: `0`

#### - [x] 5.3 features.md 不再包含 Relay 功能特性
- **来源:** spec-plan.md Task 5
- **目的:** 确认功能文档中无 relay 功能描述
- **操作步骤:**
  1. [A] `grep -ic "relay\|RelayState\|ThreadReset\|CompactDone\|RelayClient" spec/global/features.md` → 期望精确: `0`

#### - [x] 5.4 CLAUDE.md workspace crate 数量为 3
- **来源:** spec-plan.md Task 5 / spec-design.md §架构约束变更
- **目的:** 确认项目概述反映正确的 crate 数量
- **操作步骤:**
  1. [A] `grep "Workspace Crate" CLAUDE.md` → 期望包含: `3 个 Workspace Crate`

#### - [x] 5.5 CLAUDE.md 不再包含 Relay 相关内容
- **来源:** spec-plan.md Task 5 / spec-design.md §全局文档更新
- **目的:** 确认开发文档中无 relay 残留
- **操作步骤:**
  1. [A] `grep -ic "rust-relay-server\|/relay\|--remote-control\|--relay-token\|--relay-name\|RELAY_TOKEN\|RelayCli\|relay-server" CLAUDE.md` → 期望精确: `0`

---

### 场景 6: 端到端构建与运行验收

#### - [x] 6.1 全量 workspace 测试通过
- **来源:** spec-plan.md Task 6 / spec-design.md §验收标准
- **目的:** 确认所有测试无回归
- **操作步骤:**
  1. [A] `cargo test --workspace 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 6.2 全量 workspace 构建成功
- **来源:** spec-plan.md Task 6 / spec-design.md §验收标准
- **目的:** 确认整体编译通过
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | tail -10` → 期望包含: `Finished`

#### - [x] 6.3 Cargo.lock 无 rust-relay-server 残留
- **来源:** spec-plan.md Task 6
- **目的:** 确认依赖锁文件干净
- **操作步骤:**
  1. [A] `grep -c "rust-relay-server" Cargo.lock` → 期望精确: `0`

#### - [x] 6.4 TUI 源码全局无 relay 残留
- **来源:** spec-plan.md Task 6 / spec-design.md §验收标准
- **目的:** 确认无遗漏的 relay 引用
- **操作步骤:**
  1. [A] `grep -rn "relay\|Relay\|relay_client\|relay_panel\|RelayState\|RelayCli\|RemoteControl\|remote_control" rust-agent-tui/src/ --include="*.rs" | grep -v "vendor\|target" | head -20` → 期望包含: (无输出)

#### - [x] 6.5 核心框架无 TUI 特有的 relay 引用
- **来源:** spec-plan.md Task 6
- **目的:** 确认核心层无 relay 业务残留
- **操作步骤:**
  1. [A] `grep -rn "relay" rust-create-agent/src/ --include="*.rs" | head -10` → 期望包含: (无输出)

#### - [x] 6.6 全局文档整体验证无 relay 残留
- **来源:** spec-plan.md Task 5/6
- **目的:** 确认 4 份文档均无 relay 引用
- **操作步骤:**
  1. [A] `grep -c "relay-server\|Relay Server\|远程控制" spec/global/architecture.md spec/global/constraints.md spec/global/features.md CLAUDE.md` → 期望包含: `:0` × 4 个文件

#### - [x] 6.7 TUI 运行冒烟测试
- **来源:** spec-plan.md Task 6 / spec-design.md §验收标准
- **目的:** 确认 TUI 正常启动且 /relay 命令已移除
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui`，观察 TUI 正常启动、无 panic → 是/否
  2. [H] 在 TUI 中输入 `/help`，确认不显示 `/relay` 命令 → 是/否

---

## 验收后清理

本验收不涉及后台服务启动，无需清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | crate 目录已删除 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | workspace members 不含 relay | 1 | 0 | ✅ |
| 场景 1 | 1.3 | workspace 保留其余 crate | 1 | 0 | ✅ |
| 场景 1 | 1.4 | Cargo.toml 语法有效 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | 6 个 Relay 文件已删除 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | app 目录无 relay_ 文件 | 1 | 0 | ✅ |
| 场景 2 | 2.3 | 非app层 relay 文件不存在 | 3 | 0 | ✅ |
| 场景 3 | 3.1 | Cargo.toml 无 relay 依赖 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | lib.rs 无 RelayCli 导出 | 1 | 0 | ✅ |
| 场景 3 | 3.3 | app/mod.rs 无 relay 声明 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | events.rs 无 MessageAdded | 1 | 0 | ✅ |
| 场景 3 | 3.5 | agent.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 3 | 3.6 | agent_ops.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 3 | 3.7 | panel_ops.rs 无 relay 方法 | 1 | 0 | ✅ |
| 场景 3 | 3.8 | hitl_ops.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 3 | 3.9 | ask_user_ops.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 3 | 3.10 | thread_ops.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | rust-agent-tui 编译成功 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | event.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | main_ui.rs 无 relay 渲染 | 1 | 0 | ✅ |
| 场景 4 | 4.4 | panels/mod.rs 无 relay 模块 | 1 | 0 | ✅ |
| 场景 4 | 4.5 | command/mod.rs 无 relay 命令 | 1 | 0 | ✅ |
| 场景 4 | 4.6 | config/types.rs 无 RemoteControl | 1 | 0 | ✅ |
| 场景 4 | 4.7 | config/mod.rs 无 RemoteControl | 1 | 0 | ✅ |
| 场景 4 | 4.8 | main.rs 无 relay 引用 | 1 | 0 | ✅ |
| 场景 4 | 4.9 | headless.rs 无 MessageAdded | 1 | 0 | ✅ |
| 场景 4 | 4.10 | TUI 测试全部通过 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | architecture.md 无 Relay | 1 | 0 | ✅ |
| 场景 5 | 5.2 | constraints.md 无 Relay 技术栈 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | features.md 无 Relay 功能 | 1 | 0 | ✅ |
| 场景 5 | 5.4 | CLAUDE.md crate 数量为 3 | 1 | 0 | ✅ |
| 场景 5 | 5.5 | CLAUDE.md 无 Relay 残留 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | 全量测试通过 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | 全量构建成功 | 1 | 0 | ✅ |
| 场景 6 | 6.3 | Cargo.lock 无 relay 残留 | 1 | 0 | ✅ |
| 场景 6 | 6.4 | TUI 源码全局无 relay 残留 | 1 | 0 | ✅ |
| 场景 6 | 6.5 | 核心框架无 relay 引用 | 1 | 0 | ✅ |
| 场景 6 | 6.6 | 全局文档整体无 relay | 1 | 0 | ✅ |
| 场景 6 | 6.7 | TUI 运行冒烟测试 | 0 | 2 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
