# Agent Quality Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 5 systemic Agent defects discovered by historical data analysis, improving tool call reliability and error recovery.

**Architecture:** All changes are in the tool dispatch layer (`peri-agent`) and filesystem tools (`peri-middlewares`). No new crates or dependencies. Changes are independent and can be implemented in any order.

**Tech Stack:** Rust, tokio, async-trait

**Spec:** `side-projects/agent-defect-analyzer/docs/2026-06-01-agent-quality-improvements-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `peri-agent/src/agent/executor/tool_dispatch.rs` | Modify | Task 1 (normalization) + Task 5 (failure detection) |
| `peri-agent/src/agent/executor/tool_dispatch_test.rs` | Modify | Tests for Tasks 1 & 5 |
| `peri-middlewares/src/tools/filesystem/read.rs` | Modify | Task 2 (error message) |
| `peri-middlewares/src/tools/filesystem/write.rs` | Modify | Task 2 |
| `peri-middlewares/src/tools/filesystem/edit.rs` | Modify | Task 2 |
| `peri-middlewares/src/tools/filesystem/glob.rs` | Modify | Task 2 |
| `peri-agent/src/messages/adapters/openai.rs` | Modify | Task 3 ([ERROR] prefix) |
| `peri-middlewares/src/subagent/agent_result.rs` | Modify | Task 4 (polling guidance) |

---

### Task 1: Tool Name Normalization (Lookup Fallback)

**Files:**
- Modify: `peri-agent/src/agent/executor/tool_dispatch.rs:210-236`
- Test: `peri-agent/src/agent/executor/tool_dispatch_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `tool_dispatch_test.rs`:

```rust
/// LLM 输出小写工具名 "bash" 时能匹配到注册的 "Bash"
#[tokio::test]
async fn test_tool_name_case_insensitive_fallback() {
    struct PascalBash;
    #[async_trait::async_trait]
    impl BaseTool for PascalBash {
        fn name(&self) -> &str { "Bash" }
        fn description(&self) -> &str { "bash" }
        fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn invoke(&self, _: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("bash output".to_string())
        }
    }

    struct LLMOutputsLowercase;
    #[async_trait::async_trait]
    impl ReactLLM for LLMOutputsLowercase {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages.iter().any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call bash lowercase",
                    vec![ToolCall::new("id1", "bash", serde_json::json!({}))],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "result processed"))
            }
        }
    }

    let agent = ReActAgent::new(LLMOutputsLowercase)
        .max_iterations(5)
        .register_tool(Box::new(PascalBash));

    let mut state = AgentState::new("/tmp");
    let result = agent.execute(AgentInput::text("go"), &mut state, None).await;

    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);
    // 验证没有 ToolNotFound 错误
    let errors: Vec<_> = state.messages().iter()
        .filter_map(|m| match m {
            BaseMessage::Tool { content, .. } => {
                let s = content.to_string();
                if s.contains("不存在") { Some(s) } else { None }
            }
            _ => None,
        })
        .collect();
    assert!(errors.is_empty(), "不应有 ToolNotFound 错误: {:?}", errors);
}

/// LLM 输出别名 "Task" 时能通过别名表匹配到注册的 "Agent"
#[tokio::test]
async fn test_tool_name_alias_fallback() {
    struct AgentTool;
    #[async_trait::async_trait]
    impl BaseTool for AgentTool {
        fn name(&self) -> &str { "Agent" }
        fn description(&self) -> &str { "agent" }
        fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn invoke(&self, _: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("agent result".to_string())
        }
    }

    struct LLMOutputsTask;
    #[async_trait::async_trait]
    impl ReactLLM for LLMOutputsTask {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages.iter().any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call task alias",
                    vec![ToolCall::new("id1", "Task", serde_json::json!({}))],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "result processed"))
            }
        }
    }

    let agent = ReActAgent::new(LLMOutputsTask)
        .max_iterations(5)
        .register_tool(Box::new(AgentTool));

    let mut state = AgentState::new("/tmp");
    let result = agent.execute(AgentInput::text("go"), &mut state, None).await;

    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);
    let errors: Vec<_> = state.messages().iter()
        .filter_map(|m| match m {
            BaseMessage::Tool { content, .. } => {
                let s = content.to_string();
                if s.contains("不存在") { Some(s) } else { None }
            }
            _ => None,
        })
        .collect();
    assert!(errors.is_empty(), "不应有 ToolNotFound 错误: {:?}", errors);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests::test_tool_name_case_insensitive_fallback`
Expected: FAIL — `bash` not found in HashMap (only `Bash` registered)

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests::test_tool_name_alias_fallback`
Expected: FAIL — `Task` not found in HashMap (only `Agent` registered)

- [ ] **Step 3: Add alias table and fallback lookup**

At the top of `peri-agent/src/agent/executor/tool_dispatch.rs` (after imports, before `dispatch_tools`):

```rust
/// 工具名语义别名表（小写 key → 注册名 value）。
/// 用于 LLM 输出非标准工具名时的 fallback 查找。
const TOOL_ALIASES: &[(&str, &str)] = &[
    ("task", "Agent"),
    ("shell", "Bash"),
    ("reading", "Read"),
];

/// 在 all_tools 中查找工具，支持大小写归一化和语义别名 fallback。
fn resolve_tool<'a>(
    name: &str,
    all_tools: &HashMap<String, &'a dyn BaseTool>,
) -> Option<&'a dyn BaseTool> {
    // 1. 精确匹配
    if let Some(tool) = all_tools.get(name).copied() {
        return Some(tool);
    }

    // 2. 大小写归一化：遍历所有 key 做 eq_ignore_ascii_case 匹配
    let name_lower = name.to_ascii_lowercase();
    for (key, tool) in all_tools {
        if key.eq_ignore_ascii_case(&name_lower) {
            return Some(*tool);
        }
    }

    // 3. 语义别名表
    for (alias, real_name) in TOOL_ALIASES {
        if name.eq_ignore_ascii_case(alias) {
            if let Some(tool) = all_tools.get(*real_name).copied() {
                tracing::debug!(alias = %name, resolved = %real_name, "工具名别名匹配");
                return Some(tool);
            }
        }
    }

    None
}
```

- [ ] **Step 4: Replace the direct lookup with `resolve_tool`**

In `tool_dispatch.rs:217`, change:

```rust
                let tool = all_tools.get(&call.name).copied();
```

to:

```rust
                let tool = resolve_tool(&call.name, all_tools);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests`
Expected: ALL PASS (including new tests + existing tests)

- [ ] **Step 6: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch.rs peri-agent/src/agent/executor/tool_dispatch_test.rs
git commit -m "fix: add tool name normalization with case-insensitive and alias fallback

Resolves tool-not-found errors when LLM outputs non-standard tool names
(e.g., 'bash' vs 'Bash', 'Task' vs 'Agent'). Falls back to case-insensitive
matching then alias table lookup.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: Error Message Improvements

**Files:**
- Modify: `peri-middlewares/src/tools/filesystem/read.rs:122`
- Modify: `peri-middlewares/src/tools/filesystem/write.rs:64,67`
- Modify: `peri-middlewares/src/tools/filesystem/edit.rs:75,78,81`
- Modify: `peri-middlewares/src/tools/filesystem/glob.rs:131`

- [ ] **Step 1: Update Read error message**

In `read.rs`, change:

```rust
let file_path = input["file_path"]
    .as_str()
    .ok_or("Missing file_path parameter")?;
```

to:

```rust
let file_path = input["file_path"]
    .as_str()
    .ok_or("The 'file_path' parameter is required for the Read tool. Provide the absolute path to the file.")?;
```

- [ ] **Step 2: Update Write error messages**

In `write.rs`, change both occurrences:

```rust
let file_path = input["file_path"]
    .as_str()
    .ok_or("Missing file_path parameter")?;
let content = input["content"]
    .as_str()
    .ok_or("Missing content parameter")?;
```

to:

```rust
let file_path = input["file_path"]
    .as_str()
    .ok_or("The 'file_path' parameter is required for the Write tool.")?;
let content = input["content"]
    .as_str()
    .ok_or("The 'content' parameter is required for the Write tool.")?;
```

- [ ] **Step 3: Update Edit error messages**

In `edit.rs`, change all three occurrences:

```rust
let file_path = input["file_path"]
    .as_str()
    .ok_or("Missing file_path parameter")?;
let old_string = input["old_string"]
    .as_str()
    .ok_or("Missing old_string parameter")?;
let new_string = input["new_string"]
    .as_str()
    .ok_or("Missing new_string parameter")?;
```

to:

```rust
let file_path = input["file_path"]
    .as_str()
    .ok_or("The 'file_path' parameter is required for the Edit tool.")?;
let old_string = input["old_string"]
    .as_str()
    .ok_or("The 'old_string' parameter is required for the Edit tool.")?;
let new_string = input["new_string"]
    .as_str()
    .ok_or("The 'new_string' parameter is required for the Edit tool.")?;
```

- [ ] **Step 4: Update Glob error message**

In `glob.rs`, change:

```rust
let pattern = input["pattern"]
    .as_str()
    .ok_or("Missing pattern parameter")?;
```

to:

```rust
let pattern = input["pattern"]
    .as_str()
    .ok_or("The 'pattern' parameter is required for the Glob tool.")?;
```

- [ ] **Step 5: Run tests to verify no regressions**

Run: `cargo test -p peri-middlewares --lib`
Expected: ALL PASS — existing tests should not depend on exact error message text

- [ ] **Step 6: Commit**

```bash
git add peri-middlewares/src/tools/filesystem/read.rs peri-middlewares/src/tools/filesystem/write.rs peri-middlewares/src/tools/filesystem/edit.rs peri-middlewares/src/tools/filesystem/glob.rs
git commit -m "fix: improve tool error messages for better LLM error recovery

Replaces vague 'Missing X parameter' messages with explicit English
descriptions that tell the LLM which tool and which parameter is needed.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: OpenAI [ERROR] Prefix

**Files:**
- Modify: `peri-agent/src/messages/adapters/openai.rs:141-150` (serialization)
- Modify: `peri-agent/src/messages/adapters/openai.rs:223-228` (parsing)

- [ ] **Step 1: Write the failing test**

Add a test in `peri-agent/src/messages/adapters/` test file (or create one if needed). Check if there's an existing test file first with `Glob("**/messages/adapters/*test*")`. If not, add inline test at the bottom of `openai.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_tool_result_has_error_prefix() {
        let msg = BaseMessage::tool_error("call_123", "something went wrong");
        let json = OpenAiAdapter::messages_to_openai(&[msg.clone()]);
        let arr = json.as_array().unwrap();
        let tool_msg = &arr[0];
        assert_eq!(tool_msg["role"], "tool");
        assert!(tool_msg["content"].as_str().unwrap().starts_with("[ERROR] "));
    }

    #[test]
    fn test_parse_error_tool_result_detects_prefix() {
        let json = json!({
            "role": "tool",
            "tool_call_id": "call_123",
            "content": "[ERROR] something went wrong"
        });
        let msg = OpenAiAdapter::parse_assistant_message(&json).unwrap();
        match msg {
            BaseMessage::Tool { tool_call_id, content, is_error, .. } => {
                assert_eq!(tool_call_id, "call_123");
                assert!(is_error);
                assert_eq!(content.to_string(), "something went wrong");
            }
            _ => panic!("Expected Tool message"),
        }
    }

    #[test]
    fn test_success_tool_result_no_prefix() {
        let msg = BaseMessage::tool_result("call_456", "all good");
        let json = OpenAiAdapter::messages_to_openai(&[msg.clone()]);
        let arr = json.as_array().unwrap();
        let tool_msg = &arr[0];
        assert!(!tool_msg["content"].as_str().unwrap().starts_with("[ERROR] "));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p peri-agent --lib -- openai::tests`
Expected: FAIL — current code has no `[ERROR]` prefix

- [ ] **Step 3: Update serialization — add [ERROR] prefix**

In `openai.rs:141-151`, change:

```rust
                BaseMessage::Tool {
                    tool_call_id,
                    content,
                    ..
                } => {
                    result.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": Self::content_to_openai(content)
                    }));
                }
```

to:

```rust
                BaseMessage::Tool {
                    tool_call_id,
                    content,
                    is_error,
                    ..
                } => {
                    let content_str = Self::content_to_openai(content);
                    let final_content = if *is_error {
                        format!("[ERROR] {}", content_str)
                    } else {
                        content_str
                    };
                    result.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": final_content,
                    }));
                }
```

- [ ] **Step 4: Update parsing — detect [ERROR] prefix**

In `openai.rs:223-228`, change:

```rust
            "tool" => {
                let tool_call_id = value["tool_call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("tool 消息缺少 tool_call_id"))?;
                let content = parse_openai_content(&value["content"]);
                Ok(BaseMessage::tool_result(tool_call_id, content))
            }
```

to:

```rust
            "tool" => {
                let tool_call_id = value["tool_call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("tool 消息缺少 tool_call_id"))?;
                let raw_content = value["content"].as_str().unwrap_or("");
                if let Some(stripped) = raw_content.strip_prefix("[ERROR] ") {
                    Ok(BaseMessage::tool_error(tool_call_id, stripped))
                } else {
                    let content = parse_openai_content(&value["content"]);
                    Ok(BaseMessage::tool_result(tool_call_id, content))
                }
            }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p peri-agent --lib -- openai::tests`
Expected: ALL PASS

Run: `cargo test -p peri-agent --lib`
Expected: ALL PASS — existing tests should not break

- [ ] **Step 6: Commit**

```bash
git add peri-agent/src/messages/adapters/openai.rs
git commit -m "fix: add [ERROR] prefix to OpenAI tool error results

OpenAI Chat Completions API has no is_error field. Error tool results
are now prefixed with '[ERROR] ' so OpenAI-compatible LLMs can
distinguish failures from successes. Parsing side detects the prefix
and reconstructs is_error=true.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: AgentResult Polling Guidance

**Files:**
- Modify: `peri-middlewares/src/subagent/agent_result.rs:52`

- [ ] **Step 1: Update the return message**

In `agent_result.rs:48-53`, change:

```rust
    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("No completed background agent results available. Background agents may still be running or have not been started.".to_string())
    }
```

to:

```rust
    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("No completed background agent results available yet. \
            Do not retry this query immediately — continue with other work instead. \
            Background tasks will notify you when they complete. \
            If you need the result later, use ExecuteExtraTool with tool_name 'AgentResult' \
            after completing other tasks."
            .to_string())
    }
```

- [ ] **Step 2: Run tests to verify no regressions**

Run: `cargo test -p peri-middlewares --lib`
Expected: ALL PASS

- [ ] **Step 3: Commit**

```bash
git add peri-middlewares/src/subagent/agent_result.rs
git commit -m "fix: improve AgentResult response to prevent polling loops

The previous message did not instruct the LLM to stop retrying. The new
message explicitly tells the agent to continue with other work instead of
repeatedly polling for results.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 5: Consecutive Failure Detection

**Files:**
- Modify: `peri-agent/src/agent/executor/tool_dispatch.rs:72-85` (dispatch_tools, state-write phase)
- Modify: `peri-agent/src/agent/executor/tool_dispatch.rs:19-24` (dispatch_tools signature)
- Test: `peri-agent/src/agent/executor/tool_dispatch_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `tool_dispatch_test.rs`:

```rust
/// 连续 5 次同工具+同错误后注入系统纠正消息
#[tokio::test]
async fn test_consecutive_failure_injects_correction() {
    struct AlwaysFailRead;
    #[async_trait::async_trait]
    impl BaseTool for AlwaysFailRead {
        fn name(&self) -> &str { "Read" }
        fn description(&self) -> &str { "read" }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({ "properties": { "file_path": { "type": "string" } } })
        }
        async fn invoke(&self, _: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Err("The 'file_path' parameter is required for the Read tool.".into())
        }
    }

    /// LLM 反复调用 Read 工具（不传参数），直到收到纠正消息后回答
    struct StubbornLLM;
    #[async_trait::async_trait]
    impl ReactLLM for StubbornLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            // 检查是否已收到纠正消息
            let has_correction = messages.iter().any(|m| {
                matches!(m, BaseMessage::System { content, .. }
                    if content.to_string().contains("5 consecutive times"))
            });
            if has_correction {
                return Ok(Reasoning::with_answer("done", "I'll stop retrying"));
            }
            // 持续调用 Read
            Ok(Reasoning::with_tools(
                "retrying",
                vec![ToolCall::new(format!("id_{}", messages.len()), "Read", serde_json::json!({}))],
            ))
        }
    }

    let agent = ReActAgent::new(StubbornLLM)
        .max_iterations(20)
        .register_tool(Box::new(AlwaysFailRead));

    let mut state = AgentState::new("/tmp");
    let result = agent.execute(AgentInput::text("go"), &mut state, None).await;

    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);
    // 验证 state 中包含纠正消息
    let has_correction = state.messages().iter().any(|m| {
        matches!(m, BaseMessage::System { content, .. }
            if content.to_string().contains("5 consecutive times"))
    });
    assert!(has_correction, "应注入连续失败纠正消息");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests::test_consecutive_failure_injects_correction`
Expected: FAIL — no correction message is injected

- [ ] **Step 3: Add consecutive failure tracking in dispatch_tools**

In `tool_dispatch.rs`, modify `dispatch_tools` function. Add a static-ish tracking structure and detection logic in the state-write phase (after line 85):

After the imports at the top of the file, add:

```rust
/// 连续失败检测阈值
const CONSECUTIVE_FAILURE_THRESHOLD: usize = 5;
```

In `dispatch_tools`, after the state-write loop (after line 85), add:

```rust
    // 阶段 C：连续失败检测
    // 如果同一工具连续 N 次返回相同错误，注入系统纠正消息
    {
        let error_results: Vec<(&str, &str)> = results
            .iter()
            .filter(|(_, r)| r.is_error)
            .map(|(_, r)| (r.tool_name.as_str(), r.output.as_str()))
            .collect();

        for (tool_name, error_msg) in &error_results {
            let key = format!("{}:{}", tool_name, error_msg);
            let count = state.consecutive_failure_count(&key);
            if count >= CONSECUTIVE_FAILURE_THRESHOLD {
                tracing::warn!(
                    tool = %tool_name,
                    count = count,
                    "连续 {} 次相同错误，注入纠正消息",
                    count
                );
                state.add_message(BaseMessage::system(format!(
                    "Warning: Tool '{}' has failed {} consecutive times with the same error. \
                     Stop retrying and analyze the root cause. Consider using a different approach \
                     or asking the user for guidance.",
                    tool_name, count
                )));
            }
        }
    }
```

**Note:** The `state.consecutive_failure_count()` method needs to be added to the `State` trait. See Step 4.

- [ ] **Step 4: Add consecutive failure tracking to AgentState**

In `peri-agent/src/agent/state.rs`, add a field to track consecutive failures:

```rust
// In AgentState struct, add:
/// 连续失败追踪：key = "tool_name:error_msg" → count
consecutive_failures: std::collections::HashMap<String, usize>,
```

Add methods to `State` trait or `AgentState` impl:

```rust
/// 记录一次工具失败并返回当前连续失败次数
pub fn record_tool_failure(&mut self, key: &str) -> usize {
    let count = self.consecutive_failures.entry(key.to_string()).or_insert(0);
    *count += 1;
    *count
}

/// 记录一次工具成功（重置该工具的失败计数）
pub fn record_tool_success(&mut self, tool_name: &str) {
    self.consecutive_failures.retain(|k, _| !k.starts_with(&format!("{}:", tool_name)));
}

/// 获取指定 key 的连续失败次数
pub fn consecutive_failure_count(&self, key: &str) -> usize {
    *self.consecutive_failures.get(key).unwrap_or(&0)
}
```

Update `dispatch_tools` to call these methods in the state-write loop. Modify lines 76-85:

```rust
    for (_, result) in &results {
        // 记录成功/失败
        if result.is_error {
            let key = format!("{}:{}", result.tool_name, result.output);
            state.record_tool_failure(&key);
        } else {
            state.record_tool_success(&result.tool_name);
        }

        let tool_msg = if result.is_error {
            BaseMessage::tool_error(&result.tool_call_id, result.output.as_str())
        } else {
            BaseMessage::tool_result(&result.tool_call_id, result.output.as_str())
        };
        let tool_msg_clone = tool_msg.clone();
        state.add_message(tool_msg);
        agent.emit(AgentEvent::MessageAdded(tool_msg_clone));
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch.rs peri-agent/src/agent/executor/tool_dispatch_test.rs peri-agent/src/agent/state.rs
git commit -m "feat: add consecutive failure detection with correction messages

Tracks consecutive tool failures (same tool + same error). After 5
consecutive failures, injects a system message instructing the agent
to stop retrying and analyze the root cause.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Task 6: File Output Truncation Bug Issue

This is an investigation task, not a code change. Create an issue document.

- [ ] **Step 1: Create issue document**

Create `spec/issues/2026-06-01-tool-output-truncation-bypass.md`:

```markdown
---
id: 2026-06-01-tool-output-truncation-bypass
title: 工具输出截断机制被绕过（5月16日后仍有 17 条 >100KB 输出）
status: open
priority: high
created: 2026-06-01
---

## 问题

`output_persist.rs` 的截断机制在 2026-05-16 修复后，仍有 17 条超过 100KB 的工具输出（最新一条 2026-06-01）。

## 数据

| 工具 | 最大输出 | 日期 |
|------|---------|------|
| Bash | 203.3KB | 2026-05-25 |
| Grep | 155.6KB | 2026-05-27 |
| Bash | 114.4KB | 2026-05-27 |
| Glob | 109.4KB | 2026-05-29 |
| Bash | 109.6KB | 2026-06-01 |

## 排查方向

1. Bash: `truncate_output` 是否覆盖 stderr？某些命令大量输出到 stderr
2. Grep: `head_limit` 在 `output_mode=files_with_matches` 模式下是否生效？
3. JSON 序列化：content 字段含 JSON 转义后可能膨胀（`\n` → `\\n`）
4. `output_persist.rs` 的阈值是否被某些工具调用路径绕过

## 相关

- 已修复 issue: `spec/archive-issues/2026-05-15-tool-output-truncation-with-disk-persist.md`
- 分析报告: `side-projects/agent-defect-analyzer/docs/2026-06-01-defect-analysis.md` SIZE-001
```

- [ ] **Step 2: Commit**

```bash
git add spec/issues/2026-06-01-tool-output-truncation-bypass.md
git commit -m "issue: tool output truncation bypass (17 cases >100KB after fix)

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```
