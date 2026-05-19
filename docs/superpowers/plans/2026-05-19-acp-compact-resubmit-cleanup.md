# ACP Compact/Resubmit 机制清理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 清理 ACP compact/resubmit 流程中的死代码和状态不一致问题，使 resubmit 路径更健壮。

**Architecture:** resubmit 循环已在 `peri-acp/src/session/executor.rs` 中正确实现。本次修复聚焦于：(1) 删除 TUI 侧 5 个死代码字段及其写入点；(2) 删除 legacy `compact_task` 函数及关联的死模块；(3) 在 `handle_compact_completed` 中重置 `round_start_vm_idx` 消除越界 error 日志。

**Tech Stack:** Rust, tokio async, ratatui TUI

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `peri-tui/src/app/agent_comm.rs` | 删除 5 个死代码字段及其 default 初始化 |
| Modify | `peri-tui/src/app/agent_submit.rs` | 删除 `last_user_input` 和 `auto_compact_resubmit_count` 写入 |
| Modify | `peri-tui/src/app/thread_ops.rs` | 删除 `needs_auto_compact` 和 `compact_should_resubmit` 重置 |
| Modify | `peri-tui/src/app/agent_compact.rs` | 在 `handle_compact_completed` 中重置 `round_start_vm_idx` |
| Modify | `peri-tui/src/app/agent.rs` | 删除 `compact_task` 函数（~180 行）；更新文件头注释 |
| Delete | `peri-tui/src/app/agent_compact.rs` | 删除 `perform_compact` / `perform_micro_compact` TODO 存根文件 |
| Modify | `peri-tui/src/app/mod.rs` | 移除 `mod agent_compact` 声明 |

---

### Task 1: 删除 AgentComm 中 5 个死代码字段

**Files:**
- Modify: `peri-tui/src/app/agent_comm.rs:53,79-86,115,127-130`

- [ ] **Step 1: 从 struct 定义中删除 5 个字段**

在 `peri-tui/src/app/agent_comm.rs` 的 `AgentComm` struct 中，删除以下字段及其 doc comment：

```rust
// 删除这 5 个字段（约 line 52-86）：
    /// 是否需要 auto-compact（在 LlmCallEnd 时标记，Done 时执行）
    pub needs_auto_compact: bool,
    ...
    /// 本轮用户原始输入（compact 后自动 re-submit 用）
    pub last_user_input: Option<String>,
    /// compact 启动时保存的用户输入副本（防止 compact 过程中 last_user_input 被覆盖）
    pub pre_compact_user_input: Option<String>,
    /// 连续 auto-compact re-submit 次数（防止无限循环，上限 3 次）
    pub auto_compact_resubmit_count: u32,
    /// compact 完成后是否应自动 resubmit（仅 agent 执行中 auto-compact 为 true，
    /// 手动 /compact 和 Done 后 auto-compact 为 false）
    pub compact_should_resubmit: bool,
```

保留的字段（不删除）：`auto_compact_failures`（在 `agent_ops.rs:650` 被读取），`pre_compact_token_snapshot`（在 `agent_compact.rs:90` 被读取）。

- [ ] **Step 2: 从 Default impl 中删除对应初始化**

在同一个文件的 `impl Default for AgentComm` 中，删除：

```rust
// 删除这 5 行（约 line 115, 127-130）：
            needs_auto_compact: false,
            ...
            last_user_input: None,
            pre_compact_user_input: None,
            auto_compact_resubmit_count: 0,
            compact_should_resubmit: false,
```

- [ ] **Step 3: 运行 cargo check 验证编译**

Run: `cargo check -p peri-tui 2>&1 | head -30`

预期：看到 4 个编译错误（agent_submit.rs 和 thread_ops.rs 中引用了被删除的字段），这是预期的——Task 2 修复。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/agent_comm.rs
git commit -m "refactor(acp): remove 5 dead resubmit fields from AgentComm"
```

---

### Task 2: 删除死代码字段的写入点

**Files:**
- Modify: `peri-tui/src/app/agent_submit.rs:177-183`
- Modify: `peri-tui/src/app/thread_ops.rs:115,121`

- [ ] **Step 1: 删除 agent_submit.rs 中的写入**

在 `peri-tui/src/app/agent_submit.rs` 的 `submit_message()` 中，删除以下 3 个写入块（约 line 177-183）：

```rust
// 删除注释 + 两行写入
        // 保存原始用户输入（compact 后自动 re-submit 用）并重置 re-submit 计数器
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .last_user_input = Some(input.clone());
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .auto_compact_resubmit_count = 0;
```

- [ ] **Step 2: 删除 thread_ops.rs 中的写入**

在 `peri-tui/src/app/thread_ops.rs` 的 `reset_agent_session()` 中，删除以下 2 个重置块（约 line 113-121）：

```rust
// 删除这两个重置块：
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .needs_auto_compact = false;
        ...
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .compact_should_resubmit = false;
```

注意：保留 `auto_compact_failures = 0` 重置（它不是死代码，在 `agent_ops.rs:650` 被读取）。

- [ ] **Step 3: 运行 cargo check 验证编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: `Finished` (无错误)

- [ ] **Step 4: 运行 cargo test 验证测试通过**

Run: `cargo test -p peri-tui --lib 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/app/agent_submit.rs peri-tui/src/app/thread_ops.rs
git commit -m "refactor(acp): remove dead resubmit field writes from submit/thread_ops"
```

---

### Task 3: 删除 legacy compact_task 函数及关联模块

**Files:**
- Modify: `peri-tui/src/app/agent.rs:1-3,178-362`
- Delete: `peri-tui/src/app/agent_compact.rs`
- Modify: `peri-tui/src/app/mod.rs:38`

- [ ] **Step 1: 删除 agent.rs 中的 compact_task 函数**

在 `peri-tui/src/app/agent.rs` 中：

1. 删除文件头注释中关于 `compact_task` 的行：
```rust
// 删除这一行（line 3）：
// - compact_task: used by start_compact / auto-compact flow (thread_ops.rs, command/compact.rs)
```

2. 删除 `compact_task` 函数及 `mpsc` import（约 line 178-362）。具体来说：
   - 保留 `map_executor_event` 函数（line 18-176）
   - 保留 `LlmProvider` pub use（line 8）
   - 删除 `use tokio::sync::mpsc;`（line 5）— 如果 `map_executor_event` 不使用 mpsc 的话。先检查：`map_executor_event` 不使用 mpsc，只有 `compact_task` 用。安全删除。
   - 删除 `use tracing::warn;`（line 6）— 检查 `map_executor_event` 不使用 warn。安全删除。
   - 删除从 `// ─── 上下文压缩任务` 开始到文件末尾的所有内容（约 line 178-362）

- [ ] **Step 2: 删除 agent_compact.rs 存根文件**

删除整个文件 `peri-tui/src/app/agent_compact.rs`。这个文件包含 `perform_compact` 和 `perform_micro_compact` 两个 TODO 存根，只被已删除的 `compact_task` 引用。

- [ ] **Step 3: 从 mod.rs 中移除模块声明**

在 `peri-tui/src/app/mod.rs` 中，删除：
```rust
// 删除这一行（约 line 38）：
mod agent_compact;
```

- [ ] **Step 4: 运行 cargo check 验证编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: `Finished` (无错误)

- [ ] **Step 5: 运行 cargo test 验证测试通过**

Run: `cargo test -p peri-tui --lib 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/agent.rs peri-tui/src/app/mod.rs
git rm peri-tui/src/app/agent_compact.rs
git commit -m "refactor(acp): remove legacy compact_task and agent_compact stubs"
```

---

### Task 4: 修复 round_start_vm_idx resubmit 后越界

**Files:**
- Modify: `peri-tui/src/app/agent_compact.rs` (full compact 分支)

- [ ] **Step 1: 在 handle_compact_completed 中重置 round_start_vm_idx**

在 `peri-tui/src/app/agent_compact.rs` 的 `handle_compact_completed` 方法中，在 `RebuildAll` 之前，添加 `round_start_vm_idx` 重置。

找到这段代码（full compact 分支，约 line 66-69）：
```rust
        // 清空 view_messages，只显示 compact 通知
        let view_msgs = vec![MessageViewModel::system(compact_label)];
        self.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: view_msgs,
        });
```

在 `RebuildAll` 调用之前插入重置：
```rust
        // 清空 view_messages，只显示 compact 通知
        let view_msgs = vec![MessageViewModel::system(compact_label)];
        // resubmit 后 view_messages 被清空重建，round_start_vm_idx 必须重置
        // 否则第二轮 agent 事件的 request_rebuild() 使用旧值会越界
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .round_start_vm_idx = 0;
        self.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: view_msgs,
        });
```

- [ ] **Step 2: 运行 cargo check 验证编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: `Finished` (无错误)

- [ ] **Step 3: 运行 cargo test 验证测试通过**

Run: `cargo test -p peri-tui --lib 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/agent_compact.rs
git commit -m "fix(acp): reset round_start_vm_idx after compact to prevent resubmit OOB"
```

---

### Task 5: 全量验证 + lefthook

**Files:**
- 无修改

- [ ] **Step 1: 运行 lefthook pre-commit 检查**

Run: `lefthook run pre-commit 2>&1 | tail -20`
Expected: 全部通过 (fmt + check + clippy)

- [ ] **Step 2: 运行全量测试**

Run: `cargo test -p peri-tui 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 3: 运行 cargo clippy 确认无警告**

Run: `cargo clippy -p peri-tui -- -D warnings 2>&1 | tail -10`
Expected: 无 warning

- [ ] **Step 4: 最终检查 — 确认所有死代码已清除**

Run:
```bash
grep -rn 'compact_should_resubmit\|auto_compact_resubmit_count\|last_user_input\|pre_compact_user_input\|needs_auto_compact\|compact_task' peri-tui/src/ --include='*.rs' | grep -v '_test.rs'
```
Expected: 无输出（所有引用已清除）

- [ ] **Step 5: 确认 agent_compact.rs 已删除**

Run: `ls peri-tui/src/app/agent_compact.rs 2>&1`
Expected: `No such file or directory`
