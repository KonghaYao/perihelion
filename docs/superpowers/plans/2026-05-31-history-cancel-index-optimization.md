# history_for_cancel 索引替代全量克隆 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除 `history_for_cancel = history.clone()` 的每轮全量历史克隆，改用 16 字节的 `Option<MessageId>` 索引定位。

**Architecture:** 当前 `strip_leaked_prepends` 仅使用 `original_history` 的 `.first().id()` 来定位原始历史在 result 中的起始位置。全量 clone 整个 history（可能数 MB）完全多余。改为只传递 `first_history_id: Option<MessageId>`，在 `strip_leaked_prepends` 内部用 ID 匹配，语义完全等价。

**Tech Stack:** Rust, 无新依赖

---

## File Structure

| 文件 | 职责 | 改动类型 |
|------|------|----------|
| `peri-tui/src/acp_server/prompt.rs` | history_for_cancel 产生 + strip_leaked_prepends 定义 + 消费 | 修改 |
| `peri-tui/src/acp_server/prompt_test.rs` | strip_leaked_prepends 单元测试 | 修改 |

---

### Task 1: 修改 strip_leaked_prepends 签名和实现

**Files:**
- Modify: `peri-tui/src/acp_server/prompt.rs:231-255`

- [ ] **Step 1: 修改 strip_leaked_prepends 函数签名和实现**

将 `original_history: &[BaseMessage]` 参数替换为 `first_history_id: Option<peri_agent::messages::MessageId>`。函数逻辑保持语义等价：

```rust
fn strip_leaked_prepends(
    result_messages: &[peri_agent::messages::BaseMessage],
    first_history_id: Option<peri_agent::messages::MessageId>,
) -> Vec<peri_agent::messages::BaseMessage> {
    match first_history_id {
        Some(first_id) => {
            // Find where original history starts in result (skip leaked prepends)
            match result_messages.iter().position(|m| m.id() == first_id) {
                Some(start) => result_messages[start..].to_vec(),
                None => {
                    // Original history not found — compact may have replaced messages.
                    // Return as-is (no stripping).
                    result_messages.to_vec()
                }
            }
        }
        None => {
            // Original history was empty — strip leading system messages (all prepends)
            result_messages
                .iter()
                .skip_while(|m| m.is_system())
                .cloned()
                .collect()
        }
    }
}
```

- [ ] **Step 2: 修改调用点 — 替换 history_for_cancel.clone() 为 first_history_id**

将 `prompt.rs:131` 的 `history.clone()` 替换为只提取 first message ID：

```rust
// Before (prompt.rs:131):
let history_for_cancel = history.clone();

// After:
let first_history_id = history.first().map(|m| m.id());
```

将 `prompt.rs:188` 的调用替换：

```rust
// Before:
let cleaned = strip_leaked_prepends(&result.messages, &history_for_cancel);

// After:
let cleaned = strip_leaked_prepends(&result.messages, first_history_id);
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: 编译成功，无错误

---

### Task 2: 更新单元测试

**Files:**
- Modify: `peri-tui/src/acp_server/prompt_test.rs:1-80`

- [ ] **Step 4: 更新 4 个测试用例的调用签名**

所有 `strip_leaked_prepends` 调用的第二个参数从 `&history` 改为 `history.first().map(|m| m.id())`：

```rust
// 测试 1 (line 21): 有历史时剥离头部system消息
let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));

// 测试 2 (line 44): 空历史时剥离头部system
let history: Vec<BaseMessage> = vec![];
let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));

// 测试 3 (line 62): 历史id找不到时原样返回
let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));

// 测试 4 (line 77): 无leaked时正常返回
let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));
```

- [ ] **Step 5: 运行测试验证通过**

Run: `cargo test -p peri-tui --lib -- strip_leaked_prepends 2>&1`
Expected: 4 个测试全部 PASS

---

### Task 3: 检查 executor.rs 中的 history.clone() 是否可优化

**Files:**
- Read: `peri-acp/src/session/executor.rs:179-181`

- [ ] **Step 6: 确认 executor.rs:181 的 history.clone() 用途**

executor.rs:181 的 `history: history.clone()` 用于 `CommandContext`，Immediate 命令（如 `/compact`）需要完整历史。这是**所有权转移**（move into struct），不是仅用于 ID 匹配的冗余克隆，**不应修改**。

确认无需改动即可。

- [ ] **Step 7: 全量编译 + 测试**

Run: `cargo build && cargo test -p peri-tui 2>&1 | tail -5`
Expected: BUILD SUCCEEDED + 所有测试通过

- [ ] **Step 8: Commit**

```bash
git add peri-tui/src/acp_server/prompt.rs peri-tui/src/acp_server/prompt_test.rs
git commit -m "perf: replace history_for_cancel full clone with Option<MessageId>

Replace history.clone() (full Vec<BaseMessage> copy, potentially MBs)
with a 16-byte Option<MessageId> tracking only the first message ID.
strip_leaked_prepends only needs the first ID for position matching,
making the full history clone unnecessary.

Note: When auto-compact replaces messages mid-round, the first_history_id
may not be found in result_messages. The None branch safely returns
result_messages as-is (no stripping), preserving compacted messages.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```
