# Compact Thread Migration 人工验收清单

**生成时间:** 2026-03-29 22:00
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 检查 Node.js 可用（前端语法检查）: `node --version`

### 测试数据准备
- [ ] 无需额外测试数据（验证基于编译、测试、代码结构检查）

---

## 验收项目

### 场景 1：编译与测试

#### - [x] 1.1 Rust 全量编译通过
- **来源:** Task 5 End-to-end verification / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -10` → 期望: 输出 `Finished` 且无 error
- **异常排查:**
  - 如果编译失败: 逐个 crate 编译 `cargo build -p rust-agent-tui` 和 `cargo build -p rust-relay-server`，定位具体错误文件

#### - [x] 1.2 Rust 全量测试通过
- **来源:** Task 5 End-to-end verification / spec-design.md 验收标准（headless 测试通过）
- **操作步骤:**
  1. [A] `cargo test 2>&1 | grep "FAILED\|test result"` → 期望: 无 FAILED 行，所有 test result 行均为 ok
- **异常排查:**
  - 如果有测试失败: 运行 `cargo test 2>&1 | grep "FAILED" -B5` 查看失败用例名称，对照 Task 1-3 检查对应文件

---

### 场景 2：后端数据模型

#### - [x] 2.1 AgentEvent CompactDone 结构体变体
- **来源:** Task 1 检查步骤 / spec-design.md AgentEvent 变更
- **操作步骤:**
  1. [A] `grep -A5 "CompactDone {" rust-agent-tui/src/app/events.rs` → 期望: 输出包含 `summary: String` 和 `new_thread_id: String` 字段定义
- **异常排查:**
  - 如果字段不匹配: 检查 `rust-agent-tui/src/app/events.rs` 中 `CompactDone` 变体定义

#### - [x] 2.2 CompactDone 分支使用 block_in_place 创建新 Thread
- **来源:** Task 2 检查步骤 / spec-design.md 关键实现步骤
- **操作步骤:**
  1. [A] `grep -c "block_in_place" rust-agent-tui/src/app/agent_ops.rs` → 期望: 至少 3 处（ensure_thread_id 1 处 + CompactDone 分支 2 处）
  2. [A] `grep -A2 "create_thread" rust-agent-tui/src/app/agent_ops.rs` → 期望: 包含 `store.create_thread(meta)` 调用
  3. [A] `grep -A2 "append_messages" rust-agent-tui/src/app/agent_ops.rs` → 期望: 包含 `store.append_messages(&new_tid, &new_messages)` 调用
- **异常排查:**
  - 如果 block_in_place 数量不足: 检查 `agent_ops.rs` 中 `CompactDone` 分支是否完整实现了 Thread 创建和消息持久化

#### - [x] 2.3 RelayMessage CompactDone 变体
- **来源:** Task 3 检查步骤 / spec-design.md RelayMessage 新增
- **操作步骤:**
  1. [A] `grep -A6 "CompactDone {" rust-relay-server/src/protocol.rs` → 期望: 输出包含 `summary: String`、`new_thread_id: String`、`old_thread_id: String` 三个字段
- **异常排查:**
  - 如果字段缺失: 检查 `rust-relay-server/src/protocol.rs` 中 `RelayMessage` 枚举定义

#### - [x] 2.4 CompactDone 序列化 round-trip
- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-relay-server -- test_relay_compact_done_serialization 2>&1 | tail -5` → 期望: 包含 `ok` 且无 FAILED
- **异常排查:**
  - 如果测试失败: 检查 `rust-relay-server/src/protocol.rs` 中 `#[serde(tag = "type", rename_all = "snake_case")]` 是否正确，确认 `CompactDone` 变体名序列化为 `compact_done`

#### - [x] 2.5 无旧模式 CompactDone(String) 残留
- **来源:** Task 1 检查步骤 / spec-design.md 数据模型变更
- **操作步骤:**
  1. [A] `grep -rn "CompactDone(" rust-agent-tui/src/ rust-relay-server/src/ 2>/dev/null` → 期望: 无输出（无旧的 tuple variant 模式）
- **异常排查:**
  - 如果有残留匹配: 逐一检查并更新为结构体变体 `CompactDone { summary, new_thread_id }`

---

### 场景 3：Relay Web 前端

#### - [x] 3.1 events.js 语法正确
- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `node -c rust-relay-server/web/events.js 2>&1` → 期望: 无输出（exit code 0）
- **异常排查:**
  - 如果语法错误: 检查 `rust-relay-server/web/events.js` 中新增的 `compact_done` case 分支语法

#### - [x] 3.2 compact_done case 存在并正确处理
- **来源:** Task 4 检查步骤 / spec-design.md Web 端处理
- **操作步骤:**
  1. [A] `grep -n "compact_done" rust-relay-server/web/events.js` → 期望: 至少 1 处匹配
  2. [A] `grep -A8 "case 'compact_done'" rust-relay-server/web/events.js` → 期望: 包含 `agent.messages = []`（清空消息）和 `event.summary`（显示摘要）
- **异常排查:**
  - 如果 case 缺失: 在 `events.js` 的 `handleLegacyEvent` switch 中添加 `case 'compact_done'`

---

### 场景 4：端到端逻辑验证

#### - [x] 4.1 compact_task 发送 CompactDone 含 summary 字段
- **来源:** Task 1 执行步骤 / spec-design.md 整体流程
- **操作步骤:**
  1. [A] `grep "AgentEvent::CompactDone" rust-agent-tui/src/app/agent.rs` → 期望: 所有构造处使用 `CompactDone { summary: ..., new_thread_id: ... }` 结构体语法
- **异常排查:**
  - 如果使用旧语法: 更新为 `AgentEvent::CompactDone { summary, new_thread_id: String::new() }`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | Rust 全量编译通过 | 1 | 0 | ⬜ | |
| 场景 1 | 1.2 | Rust 全量测试通过 | 1 | 0 | ⬜ | |
| 场景 2 | 2.1 | AgentEvent CompactDone 结构体变体 | 1 | 0 | ⬜ | |
| 场景 2 | 2.2 | CompactDone 分支 block_in_place | 3 | 0 | ⬜ | |
| 场景 2 | 2.3 | RelayMessage CompactDone 变体 | 1 | 0 | ⬜ | |
| 场景 2 | 2.4 | CompactDone 序列化 round-trip | 1 | 0 | ⬜ | |
| 场景 2 | 2.5 | 无旧模式 CompactDone(String) 残留 | 1 | 0 | ⬜ | |
| 场景 3 | 3.1 | events.js 语法正确 | 1 | 0 | ⬜ | |
| 场景 3 | 3.2 | compact_done case 存在 | 2 | 0 | ⬜ | |
| 场景 4 | 4.1 | compact_task 发送含 summary | 1 | 0 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
