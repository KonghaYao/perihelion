# H2: ThreadStore 与 AgentState 合并 人工验收清单

**生成时间:** 2026-03-28 11:00
**关联计划:** spec/feature_20260328_H2_thread-store/spec-plan.md
**关联设计:** Plan-H2-thread-store.md（项目根目录）

> ⚠️ 所有验收项均可自动化验证，无需人类参与。

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 工作目录正确: `test -d rust-create-agent && test -d rust-agent-tui && echo "OK"`
- [ ] [AUTO] 全量编译通过（前置条件）: `cargo build 2>&1 | grep -E "^error" && echo "FAIL" || echo "OK"`

---

## 验收项目

### 场景 1：ThreadStore API 扩展

#### - [x] 1.1 append_message 方法存在且默认实现正确

- **来源:** Task 1 检查步骤 + Task 4 端到端验证
- **操作步骤:**
  1. [A] `grep -n "append_message" rust-create-agent/src/thread/store.rs` → 期望: 至少输出 2 行（方法定义 + 方法体内调用 `append_messages`）
  2. [A] `grep -A3 "async fn append_message" rust-create-agent/src/thread/store.rs` → 期望: 输出中包含 `append_messages`，即默认实现正确委托给批量方法
- **异常排查:**
  - 若输出为空: 检查 `rust-create-agent/src/thread/store.rs` 是否保存正确；重新运行 Task 1 执行步骤

---

### 场景 2：AgentState 自动持久化

#### - [x] 2.1 serde(skip) 字段与 with_persistence 方法均已添加

- **来源:** Task 2 检查步骤 + Task 4 端到端验证
- **操作步骤:**
  1. [A] `grep -n "serde(skip)\|with_persistence\|store:\|thread_id:" rust-create-agent/src/agent/state.rs` → 期望: 输出至少包含 `#[serde(skip)]`（出现在 store 和 thread_id 字段上）、`with_persistence`、`store:` 和 `thread_id:` 各 1 处以上
- **异常排查:**
  - 若缺少 `serde(skip)`: 确认 store/thread_id 字段上方有 `#[serde(skip)]` 注解
  - 若缺少 `with_persistence`: 检查 Task 2 是否执行完毕

#### - [x] 2.2 add_message 包含 tokio::spawn 自动写入逻辑

- **来源:** Task 2 检查步骤 + Task 4 端到端验证
- **操作步骤:**
  1. [A] `grep -n "tokio::spawn\|append_message" rust-create-agent/src/agent/state.rs` → 期望: 输出包含 `tokio::spawn` 1 处（fire-and-forget spawn）和 `append_message` 1 处（调用 store 方法）
- **异常排查:**
  - 若 tokio::spawn 不存在: 检查 `add_message` 实现是否包含持久化分支

#### - [x] 2.3 历史消息不经 add_message（向后兼容保证）

- **来源:** Plan-H2-thread-store.md 注意事项 §2
- **操作步骤:**
  1. [A] `grep -n "with_messages" rust-create-agent/src/agent/state.rs` → 期望: 找到 `with_messages` 构造函数，其实现直接赋值 `messages` 字段，不调用 `add_message`（即加载历史不触发持久化）
  2. [A] `grep -A8 "fn with_messages" rust-create-agent/src/agent/state.rs` → 期望: 方法体使用 `messages,` 直接赋值而非循环调用 `add_message`，确认历史消息不重复写入 DB
- **异常排查:**
  - 若 `with_messages` 内调用了 `add_message`: 这会导致历史消息被重复写入 DB，需修复为直接赋值

---

### 场景 3：TUI 手动同步清除

#### - [x] 3.1 TUI 代码中无手动 append_messages 调用

- **来源:** Task 3 检查步骤 + Task 4 端到端验证
- **操作步骤:**
  1. [A] `grep -rn "append_messages" rust-agent-tui/src/ && echo "FAIL: 发现残余调用" || echo "OK: 已全部清除"` → 期望: 输出 `OK: 已全部清除`（无任何匹配）
- **异常排查:**
  - 若发现残余调用: 查看 grep 输出中的文件行号，手动删除对应的 `append_messages` 调用块

#### - [x] 3.2 persisted_count 字段已从 TUI 代码中完全删除

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -rn "persisted_count" rust-agent-tui/src/ && echo "FAIL: 字段残余" || echo "OK: 已完全删除"` → 期望: 输出 `OK: 已完全删除`（无任何匹配）
- **异常排查:**
  - 若发现残余: 逐一检查 `mod.rs`、`agent_ops.rs`、`thread_ops.rs`、`panel_ops.rs` 中的引用并删除

#### - [x] 3.3 with_persistence 在 agent.rs 中正确绑定

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -n "with_persistence" rust-agent-tui/src/app/agent.rs` → 期望: 找到 1 处，对应 `AgentState::with_messages(...).with_persistence(thread_store, thread_id)` 调用链
- **异常排查:**
  - 若未找到: 检查 `run_universal_agent` 中 state 创建部分是否已链式调用 `.with_persistence`

#### - [x] 3.4 AgentRunConfig 包含 thread_store 和 thread_id 新字段

- **来源:** Task 3 执行步骤
- **操作步骤:**
  1. [A] `grep -n "thread_store\|thread_id" rust-agent-tui/src/app/agent.rs` → 期望: 在 `AgentRunConfig` 结构体定义中找到 `thread_store` 字段和 `thread_id` 字段各 1 处（定义 + 解构 + 使用共 3+ 处）
  2. [A] `grep -n "thread_store_for_agent\|thread_id_for_agent\|thread_store:\|thread_id:" rust-agent-tui/src/app/agent_ops.rs` → 期望: 找到 submit_message 中向 AgentRunConfig 传入 `thread_store` 和 `thread_id` 的赋值行
- **异常排查:**
  - 若未找到: 检查 `agent_ops.rs` 中 `AgentRunConfig { ... }` 构造块是否补充了两个新字段

---

### 场景 4：全量编译与测试

#### - [x] 4.1 全量编译无报错

- **来源:** Task 3 检查步骤 + Task 4 前置条件
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -E "^error"` → 期望: 无输出（零错误）
- **异常排查:**
  - 若有错误: 查看完整编译输出 `cargo build 2>&1 | head -50`，根据错误信息定位对应 Task

#### - [x] 4.2 全量测试无回归

- **来源:** Task 2 检查步骤 + Task 4 端到端验证
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent -p rust-agent-middlewares -p rust-agent-tui 2>&1 | grep -E "FAILED|test result"` → 期望: 全部输出行为 `test result: ok. N passed; 0 failed`，无任何 `FAILED` 字样
  2. [A] `cargo test -p rust-create-agent 2>&1 | grep "test result"` → 期望: `test result: ok`，确认核心库所有原有测试（state、messages、thread 相关）无回归
- **异常排查:**
  - 若 `test result: FAILED`: 运行 `cargo test -p [失败crate] 2>&1 | grep "FAILED"` 查看具体失败测试名，对应检查相关 Task

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | append_message 方法存在且实现正确 | 2 | 0 | ✅ | |
| 场景 2 | 2.1 | serde(skip) 字段与 with_persistence 存在 | 1 | 0 | ✅ | |
| 场景 2 | 2.2 | add_message 含 tokio::spawn 写入逻辑 | 1 | 0 | ✅ | |
| 场景 2 | 2.3 | 历史消息不经 add_message（向后兼容） | 2 | 0 | ✅ | |
| 场景 3 | 3.1 | 无手动 append_messages 调用 | 1 | 0 | ✅ | |
| 场景 3 | 3.2 | persisted_count 已删除 | 1 | 0 | ✅ | |
| 场景 3 | 3.3 | with_persistence 绑定调用存在 | 1 | 0 | ✅ | |
| 场景 3 | 3.4 | AgentRunConfig 含新字段 | 2 | 0 | ✅ | |
| 场景 4 | 4.1 | 全量编译无报错 | 1 | 0 | ✅ | |
| 场景 4 | 4.2 | 全量测试无回归 | 2 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
