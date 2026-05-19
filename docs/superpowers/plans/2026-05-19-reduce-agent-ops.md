# Agent Ops 消除式拆分

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 将 `agent_ops.rs` 中 `handle_agent_event` 从 889 行降到 ~150 行（纯分发），消除 Done/Error/Disconnected 三处重复清理代码

**Architecture:** 提取式重构，不新建子文件。每个超过 40 行的 match arm 抽为私有方法。三处重复清理逻辑（Langfuse flush + bg drain + pending state clear）统一为 `cleanup_agent_state()`。HITL/AskUser 交互处理虽然只有 90 行，但跨多个 crate import，独立为 `interaction.rs` 子模块。

**Tech Stack:** Rust 2021, tokio

---

### Task 1: 提取统一清理函数 `cleanup_agent_state`

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:538-882` (Done/Error handlers, Disconnected handler in poll_agent)

- [ ] **Step 1: Add `cleanup_agent_state` private method to `impl App`**

Insert before `handle_agent_event`:

```rust
impl App {
    /// 统一清理 Agent 完成状态：Langfuse flush + bg completion drain
    /// + pending interaction state + timing + channel close。
    /// 消除 Done / Error / Disconnected 三处重复的清理代码。
    fn cleanup_agent_state(&mut self) {
        let active = self.session_mgr.active;
        let session = &mut self.session_mgr.sessions[active];

        // 1. 关闭 agent channel
        session.agent.agent_rx = None;

        // 2. 处理暂存的后台任务完成通知
        let bg_notifications: Vec<String> = session.agent.pre_done_bg_completions.drain(..).collect();
        if !bg_notifications.is_empty() && !session.ui.loading {
            let combined = bg_notifications.join("\n");
            session.agent.pending_bg_continuation = Some(combined);
        }

        // 3. 清理弹窗状态
        session.agent.interaction_prompt = None;
        session.agent.pending_hitl_items = None;
        session.agent.pending_ask_user = None;

        // 4. Langfuse trace end
        if let Some(ref tracer) = session.langfuse.langfuse_tracer {
            let mut t = tracer.lock();
            t.on_end();
        }

        // 5. 记录执行耗时
        if let Some(start) = session.agent.task_start_time {
            session.agent.last_task_duration = Some(start.elapsed());
        }
    }
```

- [ ] **Step 2: Replace Done handler's cleanup (lines ~610-670) with call to `cleanup_agent_state()`**

In the `AgentEvent::Done` arm, find the cleanup block (lines ~610-670 handling `pre_done_bg_completions`, Langfuse flush, timing, channel close). Replace it with:

```rust
self.cleanup_agent_state();
```

- [ ] **Step 3: Replace Error handler's cleanup (lines ~810-860) with same call**

In `AgentEvent::Error` arm:

```rust
self.cleanup_agent_state();
```

- [ ] **Step 4: Replace Disconnected handler's cleanup (lines ~1240-1300) with same call**

In `poll_agent` function, `TryRecvError::Disconnected` arm:

```rust
self.cleanup_agent_state();
```

- [ ] **Step 5: Build to verify dedup compiles**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

Expected: compilation succeeds.

---

### Task 2: 提取 `handle_done` 私有方法

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:538-674` (Done match arm, 137 lines)

- [ ] **Step 1: Extract Done arm body into `handle_done`**

```rust
    fn handle_done(&mut self) -> (bool, bool, bool) {
        // Child agent Done during tool execution — ignore
        if self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .in_subagent()
        {
            return (false, false, false);
        }

        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .retry_status = None;

        // Pipeline: finalize current AI message
        let actions = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .handle_event(super::AgentEvent::Done);
        for action in actions {
            self.apply_pipeline_action(action);
        }

        // Skip reconcile if already done by Interrupted/Error
        if !self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .reconcile_already_done
        {
            self.request_rebuild();
        } else {
            // Interrupted/Error already reconciled — clear streaming flag and rebuild
            if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .pipeline
                    .streaming_vm
                    .as_mut()
            {
                *is_streaming = false;
            }
            self.request_rebuild();
        }

        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .loading = false;

        // Background task continuation logic
        if self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0 {
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_done_pending_bg = true;
        }

        self.cleanup_agent_state();

        // Flush any pending messages
        self.flush_pending_messages();

        (true, false, true)
    }
```

- [ ] **Step 2: Replace Done arm in `handle_agent_event` with delegation**

```rust
AgentEvent::Done => return self.handle_done(),
```

- [ ] **Step 3: Build**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

Expected: compilation succeeds.

---

### Task 3: 提取 `handle_interrupted` 和 `handle_error`

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:675-882` (Interrupted + Error arms, ~208 lines)

- [ ] **Step 1: Extract Interrupted arm → `handle_interrupted`**

```rust
    fn handle_interrupted(&mut self) -> (bool, bool, bool) {
        if self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .in_subagent()
        {
            return (false, false, false);
        }

        let actions = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .handle_event(super::AgentEvent::Interrupted);
        for action in actions {
            self.apply_pipeline_action(action);
        }

        let agent_replied = self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_replied;
        if !agent_replied {
            // Agent hasn't replied yet — restore user text to input
            if let Some(text) = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .last_submitted_text
                .take()
            {
                let round_start = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .round_start_vm_idx;
                self.apply_pipeline_action(PipelineAction::RebuildAll {
                    prefix_len: round_start,
                });
                self.session_mgr.sessions[self.session_mgr.active]
                    .ui
                    .textarea
                    .insert_str(&text);
            }
        } else {
            self.request_rebuild();
        }

        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .reconcile_already_done = true;
        self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .loading = false;

        // Add interrupt notification
        let note = MessageViewModel::system_note("Interrupted".to_string());
        self.apply_pipeline_action(PipelineAction::AddMessage(note));
        self.request_rebuild();

        self.flush_pending_messages();
        (true, false, true)
    }
```

- [ ] **Step 2: Extract Error arm → `handle_error`**

```rust
    fn handle_error(&mut self, e: String) -> (bool, bool, bool) {
        if self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .in_subagent()
        {
            return (false, false, false);
        }

        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .retry_status = None;

        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .done();

        let mut vm = MessageViewModel::tool_block(
            "error".to_string(),
            "Agent Error".to_string(),
            None,
            true,
        );
        if let MessageViewModel::ToolBlock {
            content, collapsed, ..
        } = &mut vm
        {
            *content = e;
            *collapsed = false;
        }
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));

        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .reconcile_already_done = true;

        self.cleanup_agent_state();
        self.flush_pending_messages();
        (true, false, true)
    }
```

- [ ] **Step 3: Replace both arms in `handle_agent_event`**

```rust
AgentEvent::Interrupted => return self.handle_interrupted(),
AgentEvent::Error { error: e } => return self.handle_error(e),
```

- [ ] **Step 4: Build**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

---

### Task 4: 提取 `handle_token_usage_update` 和 `handle_subagent_start`

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:383-535` (TokenUsageUpdate, SubAgentEnd, etc.)

- [ ] **Step 1: Extract TokenUsageUpdate → `handle_token_usage_update`**

```rust
    fn handle_token_usage_update(&mut self, token_info_json: Option<serde_json::Value>) -> (bool, bool, bool) {
        if self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .in_subagent()
        {
            return (false, false, false);
        }
        // ... (copy existing TokenUsageUpdate logic) ...
        (true, false, false)
    }
```

- [ ] **Step 2: Extract SubAgentStart → `handle_subagent_start`**

```rust
    fn handle_subagent_start(
        &mut self,
        agent_id: String,
        instance_id: String,
        task_preview: String,
        is_background: bool,
    ) -> (bool, bool, bool) {
        if is_background {
            self.session_mgr.sessions[self.session_mgr.active].background_task_count += 1;
        }
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .subagent_depth += 1;
        // ... (copy existing SubAgentStart Langfuse logic) ...
        (true, false, false)
    }
```

- [ ] **Step 3: Replace arms in `handle_agent_event`**

```rust
AgentEvent::SubAgentStart { agent_id, instance_id, task_preview, is_background } => {
    return self.handle_subagent_start(agent_id, instance_id, task_preview, is_background);
}
AgentEvent::TokenUsageUpdate { raw_json } => {
    return self.handle_token_usage_update(raw_json);
}
```

- [ ] **Step 4: Build**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

---

### Task 5: 提取 HITL/AskUser 交互到 `agent_ops_interaction.rs`

**Files:**
- Create: `peri-tui/src/app/agent_ops_interaction.rs`
- Modify: `peri-tui/src/app/agent_ops.rs:56-194` (handle_acp_request_permission, handle_acp_elicitation)
- Modify: `peri-tui/src/app/agent_ops.rs:883-980` (InteractionRequest arm body)

- [ ] **Step 1: Create `agent_ops_interaction.rs` with three functions**

```rust
use peri_acp::transport::types::RequestId;
use peri_middlewares::hitl::BatchItem;
use tokio::sync::oneshot;

use super::App;

impl App {
    /// Handle ACP RequestPermission: create HITL approval dialog
    pub(crate) fn handle_acp_request_permission(
        &mut self,
        id: RequestId,
        params: serde_json::Value,
    ) -> (bool, bool, bool) {
        // ... (copy existing body from agent_ops.rs lines 56-102) ...
    }

    /// Handle ACP Elicitation: create AskUser question dialog
    pub(crate) fn handle_acp_elicitation(
        &mut self,
        id: RequestId,
        params: serde_json::Value,
    ) -> (bool, bool, bool) {
        // ... (copy existing body from agent_ops.rs lines 105-194) ...
    }

    /// Handle InteractionRequest AgentEvent: bridge to HITL/AskUser dialog
    pub(crate) fn handle_interaction_request(
        &mut self,
        ctx: peri_agent::interaction::InteractionContext,
        response_tx: tokio::sync::oneshot::Sender<peri_agent::interaction::InteractionResponse>,
    ) -> (bool, bool, bool) {
        // ... (copy existing body from agent_ops.rs lines 892-980) ...
    }
}
```

- [ ] **Step 2: Update `agent_ops.rs` to delegate to `agent_ops_interaction.rs` methods**

In `handle_acp_request_permission` and `handle_acp_elicitation`: replace bodies with `self.handle_acp_request_permission(id, params)` → rename as needed.

In `handle_agent_event` `InteractionRequest` arm:

```rust
AgentEvent::InteractionRequest { ctx, response_tx } => {
    return self.handle_interaction_request(ctx, response_tx);
}
```

- [ ] **Step 3: Add `mod agent_ops_interaction;` to `app/mod.rs`**

```rust
pub mod agent_ops_interaction;
```

- [ ] **Step 4: Build**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

---

### Task 6: 验证 `handle_agent_event` 降到 ~150 行

- [ ] **Step 1: Count match arms**

After extraction, `handle_agent_event` should contain only:

```rust
pub(crate) fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
    match event {
        AgentEvent::SubAgentStart { .. } => return self.handle_subagent_start(...),
        AgentEvent::SubAgentEnd { .. } | AgentEvent::SubagentLifecycle { .. } => { /* inline, ~10 lines each */ },
        AgentEvent::ContextWarning { .. } => { /* inline, ~15 lines */ },
        AgentEvent::TodoUpdate { items } => { /* inline, ~5 lines */ },
        AgentEvent::StateSnapshot { messages } => { /* inline, ~15 lines */ },
        AgentEvent::LlmRetrying { .. } => { /* inline, ~10 lines */ },
        AgentEvent::TokenUsageUpdate { raw_json } => return self.handle_token_usage_update(raw_json),
        AgentEvent::ToolStart { .. } | AgentEvent::ToolEnd { .. } | AgentEvent::AssistantChunk { .. }
            | AgentEvent::AiReasoning { .. } => { /* pipeline delegation, ~5 lines each */ },
        AgentEvent::Done => return self.handle_done(),
        AgentEvent::Interrupted => return self.handle_interrupted(),
        AgentEvent::Error { error: e } => return self.handle_error(e),
        AgentEvent::InteractionRequest { ctx, response_tx } => return self.handle_interaction_request(ctx, response_tx),
        AgentEvent::OAuthAuthorizationNeeded { .. } | AgentEvent::OAuthAuthorizationCompleted { .. }
            | AgentEvent::OAuthAuthorizationFailed { .. } | AgentEvent::McpActionCompleted { .. }
            => { /* delegate to agent_events_oauth.rs, ~5 lines each */ },
        AgentEvent::PluginActionCompleted { .. } => { /* delegate to agent_events_plugin.rs */ },
        AgentEvent::CompactCompleted { .. } | AgentEvent::CompactStarted { .. } | AgentEvent::CompactError { .. }
            => { /* delegate to agent_compact.rs */ },
        AgentEvent::BackgroundTaskCompleted { .. } => { /* delegate to agent_events_bg.rs */ },
        AgentEvent::LspDiagnostics { .. } => { /* inline, ~5 lines */ },
    }
}
```

- [ ] **Step 2: Build and count lines**

```bash
cargo build -p peri-tui 2>&1 | head -10
wc -l peri-tui/src/app/agent_ops.rs
```

Expected: `agent_ops.rs` ~600 lines (down from 1385).

---

### Task 7: 全量测试和提交

- [ ] **Step 1: Run all tests**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -10
```

Expected: All tests pass.

- [ ] **Step 2: Commit**

```bash
cargo fmt -p peri-tui
git add peri-tui/src/app/
git commit -m "refactor: eliminate agent_ops duplication with extracted handlers

- Extract cleanup_agent_state() to unify Done/Error/Disconnected cleanup
- Extract handle_done, handle_interrupted, handle_error private methods
- Extract handle_token_usage_update, handle_subagent_start
- Extract HITL/AskUser interaction to agent_ops_interaction.rs
- Reduce handle_agent_event from 889 to ~150 lines

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
