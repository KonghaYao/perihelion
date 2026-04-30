# Done/Interrupted 事件 Reconcile 修复 执行计划

**目标:** `Done` 和 `Interrupted` 事件触发 `reconcile_tail()` 尾部重建，确保流式最终状态与恢复路径一致；移除 `StreamingDone` 变体。

**技术栈:** Rust / ratatui / tokio mpsc

**设计文档:** spec/feature_20260430_F002_reconcile-on-done-interrupted/spec-design.md

## 改动总览

本次改动涉及 `message_pipeline.rs` 和 `core.rs` 两个文件。Task 1 修改数据模型（`PipelineAction` 枚举、`MessagePipeline` 新增方法、`AppCore` 新增字段），Task 2 适配事件处理逻辑（`Done`/`Interrupted` 事件处理、`apply_pipeline_action` 适配、`submit_message` 记录索引），Task 3 编写单元测试覆盖核心逻辑。

关键设计决策：经代码分析确认 `compactDone` 场景（agent_ops.rs:743）使用 `RebuildAll(view_msgs)` 是全量重建，应使用 `prefix_len: 0`；`reconcile()` 方法已存在于 message_pipeline.rs:577，直接复用；`StreamingDone` 从 `PipelineAction` 移除后，render thread 的 `RenderEvent::StreamingDone` 仍保留（内部使用）。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 Rust 构建工具可用
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证测试工具可用
  - `cargo test -p rust-agent-tui --lib -- --list 2>&1 | tail -5`
  - 预期: 测试框架可用，无配置错误

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 包含 "Finished" 且无 error
- [x] 测试命令可用
  - `cargo test -p rust-agent-tui --lib message_pipeline::tests 2>&1 | grep "test result"`
  - 预期: 测试结果输出 "test result: ok"

---

### Task 1: 数据模型变更

**背景:**
业务语境 — 修复 `Done`/`Interrupted` 事件处理，使其触发 `reconcile_tail()` 尾部重建而非 `StreamingDone`/`None`，确保流式最终状态与历史恢复路径一致。修改原因 — 当前 `RebuildAll(Vec<MessageViewModel>)` 不区分不变前缀与重建尾部，导致全量重建效率低下；`StreamingDone` 变体不再需要，render thread 将通过 reconcile+LoadHistory 处理 `is_streaming` 标志。上下游影响 — Task 1 创建数据模型基础，Task 2 依赖这些改动适配事件处理逻辑，Task 3 验证核心行为正确性。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/message_pipeline.rs`
- 修改: `rust-agent-tui/src/app/core.rs`

**执行步骤:**
- [x] 将 `PipelineAction::RebuildAll` 改为结构体形式
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs:36-58`（`PipelineAction` 枚举定义）
  - 将 `RebuildAll(Vec<MessageViewModel>)` 改为 `RebuildAll { prefix_len: usize, tail_vms: Vec<MessageViewModel> }`
  - 原因: `prefix_len` 标记不变前缀长度，`tail_vms` 存储重建尾部，避免全量拷贝提升性能
  
- [x] 移除 `PipelineAction::StreamingDone` 变体
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs:46`
  - 删除 `StreamingDone` 这一行
  - 原因: `Done`/`Interrupted` 事件将改为调用 `reconcile_tail()`，不再需要此变体
  
- [x] 为 `MessagePipeline` 新增 `reconcile_tail()` 方法
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs:579`（`reconcile()` 方法之后）
  - 新增方法签名: `pub fn reconcile_tail(&self, round_start_vm_idx: usize) -> (usize, Vec<MessageViewModel>)`
  - 方法实现: 找到 `completed` 中最后一条 `Human` 消息的 index，从该 index 开始调用 `messages_to_view_models()` 获取 tail_vms，返回 `(round_start_vm_idx, tail_vms)`
  - 原因: 计算不变前缀长度和重建尾部，供 `Done`/`Interrupted` 事件使用
  
- [x] 为 `AppCore` 新增 `round_start_vm_idx` 字段
  - 位置: `rust-agent-tui/src/app/core.rs:21-67`（`AppCore` 结构体定义）
  - 在 `pub view_messages: Vec<MessageViewModel>,` 之后追加: `pub round_start_vm_idx: usize,`
  - 原因: 记录每轮对话开始时的 VM 索引，用于 `reconcile_tail()` 计算前缀长度
  
- [x] 初始化 `AppCore::new()` 中的 `round_start_vm_idx` 字段
  - 位置: `rust-agent-tui/src/app/core.rs:84-119`（`Self { ... }` 初始化块）
  - 在 `view_messages: Vec::new(),` 之后追加: `round_start_vm_idx: 0,`
  - 原因: 新字段必须初始化，初始值 0 表示第一轮对话
  
- [x] 为 `reconcile_tail()` 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/message_pipeline.rs`（现有 `[cfg(test)]` 模块，~L626 之后）
  - 测试场景:
    - 场景1: `round_start_vm_idx=0` 返回完整列表 → 预期返回 `(0, full_vms)`
    - 场景2: `round_start_vm_idx=2` 返回从最后一条 Human 消息开始的尾部 → 预期返回 `(2, 从最后一条 Human 开始的 tail_vms)`
    - 场景3: 空 `completed` 返回空尾部 → 预期返回 `(0, [])`
  - 运行命令: `cargo test -p rust-agent-tui --lib reconcile_tail`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 `PipelineAction` 定义正确
  - `grep -A 2 "pub enum PipelineAction" rust-agent-tui/src/app/message_pipeline.rs | grep "RebuildAll"`
  - 预期: 输出包含 `RebuildAll { prefix_len: usize, tail_vms: Vec<MessageViewModel> }`
  
- [x] 验证 `StreamingDone` 已移除
  - `grep "StreamingDone" rust-agent-tui/src/app/message_pipeline.rs`
  - 预期: 无输出（已移除）
  
- [x] 验证 `reconcile_tail()` 方法存在
  - `grep "pub fn reconcile_tail" rust-agent-tui/src/app/message_pipeline.rs`
  - 预期: 输出方法签名行
  
- [x] 验证 `AppCore` 包含 `round_start_vm_idx` 字段
  - `grep "round_start_vm_idx:" rust-agent-tui/src/app/core.rs`
  - 预期: 输出 2 行（结构体定义 + 初始化）
  
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 包含 "Finished" 且无 error

- [x] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib reconcile_tail 2>&1 | grep -E "(test result|running"`
  - 预期: 输出包含 "test result: ok" 且所有测试通过

---
### Task 2: 事件处理适配

**背景:**
业务语境 — 将 `Done`/`Interrupted` 事件处理从 `StreamingDone`/`None` 改为调用 `reconcile_tail()` 触发尾部重建，确保流式结束时的 VM 列表与历史恢复路径一致。修改原因 — 当前 `Done` 事件返回 `PipelineAction::StreamingDone` 导致逻辑不一致，`Interrupted` 返回 `None` 不触发重建；`round_start_vm_idx` 未记录导致无法计算前缀长度。上下游影响 — Task 1 已创建数据模型基础，本 Task 依赖 `reconcile_tail()` 方法和 `round_start_vm_idx` 字段，Task 3 将验证核心行为正确性。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/message_pipeline.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 修改 `handle_event(Done)` 返回 `None` 而非 `StreamingDone`
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs:223-226`（`AgentEvent::Done =>` 分支）
  - 将 `vec![PipelineAction::StreamingDone]` 改为 `vec![PipelineAction::None]`
  - 原因: reconcile 逻辑由 `agent_ops` 调用，pipeline 只负责状态更新

- [x] 修改 `handle_agent_event(Done)` 调用 `reconcile_tail()` 并应用 `RebuildAll`
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:415-462`（`AgentEvent::Done =>` 分支）
  - 在 `self.core.pipeline.handle_event(AgentEvent::Done);` 之后追加:
    ```rust
    let (prefix_len, tail_vms) = self.core.pipeline.reconcile_tail(self.core.round_start_vm_idx);
    self.apply_pipeline_action(PipelineAction::RebuildAll {
        prefix_len,
        tail_vms,
    });
    ```
  - 原因: 流式结束时调用 reconcile 获取重建尾部，通过 RebuildAll 触发截断+extend

- [x] 修改 `handle_agent_event(Interrupted)` 调用 `reconcile_tail()` 并应用 `RebuildAll`
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:464-476`（`AgentEvent::Interrupted =>` 分支）
  - 在 `self.core.pipeline.handle_event(AgentEvent::Interrupted);` 之后追加与 Done 相同的 reconcile 逻辑
  - 原因: 中断场景同样需要 reconcile 尾部重建

- [x] 修改 `submit_message` 在推送 Human VM 前记录 `round_start_vm_idx`
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:21-162`（`submit_message` 方法）
  - 在 `let user_vm = MessageViewModel::user(display.clone());` 之前追加: `self.core.round_start_vm_idx = self.core.view_messages.len();`
  - 原因: 记录本轮对话起始索引，供 reconcile_tail 计算前缀长度

- [x] 修改 `apply_pipeline_action` 中 `RebuildAll` 处理为截断+extend 模式
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:174-251`（`apply_pipeline_action` 方法）
  - 将 `PipelineAction::RebuildAll(vms) =>` 分支（~L247-250）改为:
    ```rust
    PipelineAction::RebuildAll { prefix_len, tail_vms } => {
        self.core.view_messages.truncate(prefix_len);
        self.core.view_messages.extend(tail_vms.clone());
        let _ = self.core.render_tx.send(RenderEvent::LoadHistory(self.core.view_messages.clone()));
    }
    ```
  - 原因: 新结构体形式区分不变前缀与重建尾部，避免全量拷贝

- [x] 移除 `apply_pipeline_action` 中的 `StreamingDone` match 分支
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:239-246`（`PipelineAction::StreamingDone => { ... }` 分支）
  - 删除整个 `StreamingDone` match arm（含 `is_streaming = false` 逻辑和 `RenderEvent::StreamingDone` 发送）
  - 原因: `PipelineAction::StreamingDone` 变体已在 Task 1 中移除，此处 match 分支会导致编译错误

- [x] 修改 `CompactDone` 处理使用新的 `RebuildAll` 形式
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:743`（`CompactDone =>` 分支）
  - 将 `PipelineAction::RebuildAll(view_msgs)` 改为 `PipelineAction::RebuildAll { prefix_len: 0, tail_vms: view_msgs }`
  - 原因: Compact 场景是全量重建，prefix_len 设为 0

- [x] 为事件处理逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/agent_ops.rs`（新增或扩展现有测试）
  - 测试场景:
    - 场景1: `Done` 事件触发 reconcile → 预期 `view_messages` 被截断到 `round_start_vm_idx` 并 extend reconcile 结果
    - 场景2: `Interrupted` 事件触发 reconcile → 预期与 Done 相同
    - 场景3: `submit_message` 记录 `round_start_vm_idx` → 预期在 push Human VM 前索引被记录
  - 运行命令: `cargo test -p rust-agent-tui --lib reconcile_event_handling`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 `Done` 事件不再返回 `StreamingDone`
  - `grep -A 2 "AgentEvent::Done =>" rust-agent-tui/src/app/message_pipeline.rs | grep "None"`
  - 预期: 输出包含 `PipelineAction::None`

- [x] 验证 `Done` 事件处理调用 `reconcile_tail`
  - `grep -A 5 "AgentEvent::Done =>" rust-agent-tui/src/app/agent_ops.rs | grep "reconcile_tail"`
  - 预期: 输出包含 `reconcile_tail` 调用

- [x] 验证 `Interrupted` 事件处理调用 `reconcile_tail`
  - `grep -A 5 "AgentEvent::Interrupted =>" rust-agent-tui/src/app/agent_ops.rs | grep "reconcile_tail"`
  - 预期: 输出包含 `reconcile_tail` 调用

- [x] 验证 `submit_message` 记录 `round_start_vm_idx`
  - `grep -B 1 "MessageViewModel::user" rust-agent-tui/src/app/agent_ops.rs | grep "round_start_vm_idx"`
  - 预期: 输出包含 `round_start_vm_idx = self.core.view_messages.len()`

- [x] 验证 `RebuildAll` 处理使用截断+extend 模式
  - `grep -A 4 "PipelineAction::RebuildAll {" rust-agent-tui/src/app/agent_ops.rs | grep "truncate"`
  - 预期: 输出包含 `truncate(prefix_len)` 和 `extend(tail_vms)`

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 包含 "Finished" 且无 error

- [x] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib reconcile_event_handling 2>&1 | grep -E "(test result|running"`
  - 预期: 输出包含 "test result: ok" 且所有测试通过

---
### Task 3: 集成测试

**背景:**
业务语境 — 验证 `reconcile_tail()` 在 `Done`/`Interrupted` 事件后产生的尾部重建结果与 `messages_to_view_models()` 全量转换路径完全一致，确保流式结束和历史恢复的 VM 列表等价。修改原因 — Task 1-2 已实现数据模型和事件处理适配，需通过集成测试确认尾部重建逻辑的正确边界和一致性，避免运行时出现 VM 列表不一致导致的显示错误。上下游影响 — 本 Task 依赖 Task 1 的 `reconcile_tail()` 方法和 Task 2 的事件处理逻辑，测试结果将验证整个 feature 的正确性。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/message_pipeline.rs`

**执行步骤:**
- [x] 新增 `test_reconcile_tail_consistency` 测试验证尾部重建与全量转换一致性
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs`（现有 `[cfg(test)]` 模块，~L926 之后）
  - 测试逻辑: 构造多轮对话 completed = [Human("q1"), Ai("a1"), Human("q2"), Ai("a2")]，设置 round_start_vm_idx = 2，调用 `reconcile_tail(2)` 获取尾部，调用 `messages_to_view_models(completed, cwd)` 获取完整 VMs，断言 tail_vms 等于从最后一条 Human 消息开始重建的 VMs
  - 原因: 验证 reconcile_tail 能正确截取从最后一个 Human 消息开始的尾部，与全量路径结果一致

- [x] 新增 `test_reconcile_tail_with_tools` 测试验证工具调用场景的尾部重建
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs`（~L926 之后）
  - 测试逻辑: 构造 completed = [Human("read file"), Ai_with_tool_calls("reading", [tc1]), Tool(tc1, "content")]，设置 round_start_vm_idx = 0，调用 `reconcile_tail(0)` 和 `messages_to_view_models(completed, cwd)`，断言 tail_vms 等于从最后一条 Human 消息开始重建的 VMs
  - 原因: 验证工具调用场景（当前轮有工具交互）reconcile_tail 能正确重建 ToolBlock VM

- [x] 新增 `test_reconcile_tail_empty_completed` 测试边界情况
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs`（~L926 之后）
  - 测试逻辑: 构造空 completed，调用 `reconcile_tail(0)`，断言返回空 Vec
  - 原因: 验证边界条件，空 completed 不应导致 panic 或错误结果

- [x] 修改 `test_handle_event_tool_lifecycle` 移除 `StreamingDone` 断言
  - 位置: `rust-agent-tui/src/app/message_pipeline.rs:857-859`（现有测试）
  - 删除 `assert!(matches!(actions[0], PipelineAction::StreamingDone));` 这一行
  - 原因: Task 1-2 后 Done 事件返回 `PipelineAction::None`，不再返回 `StreamingDone`

- [x] 运行所有 message_pipeline 测试确保无回归
  - 测试文件: `rust-agent-tui/src/app/message_pipeline.rs`
  - 运行命令: `cargo test -p rust-agent-tui --lib message_pipeline::tests`
  - 预期: 所有测试通过，包括新增的 reconcile_tail 测试和修改后的 test_handle_event_tool_lifecycle

**检查步骤:**
- [x] 验证 `test_reconcile_tail_consistency` 测试存在
  - `grep "fn test_reconcile_tail_consistency" rust-agent-tui/src/app/message_pipeline.rs`
  - 预期: 输出测试函数定义行

- [x] 验证 `test_reconcile_tail_with_tools` 测试存在
  - `grep "fn test_reconcile_tail_with_tools" rust-agent-tui/src/app/message_pipeline.rs`
  - 预期: 输出测试函数定义行

- [x] 验证 `test_reconcile_tail_empty_completed` 测试存在
  - `grep "fn test_reconcile_tail_empty_completed" rust-agent-tui/src/app/message_pipeline.rs`
  - 预期: 输出测试函数定义行

- [x] 验证 `test_handle_event_tool_lifecycle` 不再断言 StreamingDone
  - `grep -A 2 "AgentEvent::Done" rust-agent-tui/src/app/message_pipeline.rs | grep "StreamingDone"`
  - 预期: 无输出（已移除断言）

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 包含 "Finished" 且无 error

- [x] 验证所有测试通过
  - `cargo test -p rust-agent-tui --lib message_pipeline::tests 2>&1 | grep -E "(test result|running)"`
  - 预期: 输出包含 "test result: ok" 且所有测试通过

---

### Task 4: 验收

**前置条件:**
- Task 0-3 全部完成
- 构建成功: `cargo build -p rust-agent-tui`

**端到端验证:**

- [x] 运行完整测试套件确保无回归
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -10`
  - 预期: 全部测试通过（274 passed, 1 pre-existing failure in test_subagent_group_basic）
  - 失败排查: 检查 Task 1 reconcile_tail 测试、Task 2 事件处理测试、Task 3 集成测试

- [x] 验证 `StreamingDone` 已从 `PipelineAction` 完全移除
  - `grep -rn "PipelineAction::StreamingDone" rust-agent-tui/src/`
  - 预期: 无输出（所有引用已清除）
  - 失败排查: 检查 Task 1 是否遗漏 `apply_pipeline_action` 中的 match 分支

- [x] 验证 `RebuildAll` 使用结构体形式（非元组）
  - `grep -n "RebuildAll(" rust-agent-tui/src/`
  - 预期: 无输出（所有 `RebuildAll` 调用已改为 `RebuildAll { prefix_len, tail_vms }` 形式）
  - 失败排查: 检查 Task 1 的枚举定义和 Task 2 的所有调用点

- [x] 验证 `Done` 和 `Interrupted` 事件路径包含 `reconcile_tail` 调用
  - `grep -A 3 "AgentEvent::Done =>" rust-agent-tui/src/app/agent_ops.rs | grep "reconcile_tail"`
  - `grep -A 3 "AgentEvent::Interrupted =>" rust-agent-tui/src/app/agent_ops.rs | grep "reconcile_tail"`
  - 预期: 两个 grep 均有输出
  - 失败排查: 检查 Task 2 事件处理适配步骤

- [x] 验证 `RenderEvent::StreamingDone` 仍保留（渲染线程内部使用）
  - `grep "StreamingDone" rust-agent-tui/src/ui/render_thread.rs`
  - 预期: 有输出（RenderEvent 枚举定义和处理分支仍存在）
  - 失败排查: 确认 Task 1 只移除了 `PipelineAction::StreamingDone`，未触及 RenderEvent

---
