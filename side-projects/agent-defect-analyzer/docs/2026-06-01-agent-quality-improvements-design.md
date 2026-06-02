# Agent 质量改进设计

**日期**：2026-06-01
**来源**：`side-projects/agent-defect-analyzer` 缺陷分析报告

## 概述

基于 528 个会话、79K 条消息的历史数据分析，发现 6 个 Agent 系统性缺陷。本设计覆盖其中 5 项代码改动 + 1 项 bug issue。

## 改动清单

### 1. 工具名归一化查找层

**问题**：LLM 输出工具名大小写不一致（`bash` vs `Bash`）或使用别名（`Task` vs `Agent`），导致 `ToolNotFound` 错误。数据：51 次幻觉调用，影响 23 个会话。

**位置**：`peri-agent/src/agent/executor/tool_dispatch.rs:217`

**当前**：
```rust
let tool = all_tools.get(&call.name).copied();
```

**改为**：精确匹配失败后 fallback 到归一化匹配。

```
精确匹配 → 失败 → to_lowercase() 全遍历重试 → 失败 → 别名表查找 → 失败 → ToolNotFound
```

**别名表**（const，`tool_dispatch.rs` 顶部）：
```rust
const TOOL_ALIASES: &[(&str, &str)] = &[
    ("task", "Agent"),
    ("shell", "Bash"),
    ("reading", "Read"),
];
```

查找逻辑：`call.name.to_lowercase()` 遍历 `all_tools` 的 keys 做 `eq_ignore_ascii_case` 匹配，再查别名表。

**注册层不动**。所有工具按原名注册（PascalCase 或 MCP 原始名）。

**性能**：fallback 路径仅在精确匹配失败时触发（极低频），全遍历 HashMap <50 个 key，开销可忽略。

**测试**：
- LLM 输出 `bash`（小写）能找到注册的 `Bash`
- LLM 输出 `Task` 能通过别名找到 `Agent`
- 正常 PascalCase 输入不受影响

---

### 2. 错误消息改进

**问题**：`"Missing file_path parameter"` 过于简短，LLM 无法理解如何修正，导致连续重试（最长 7 次）。数据：42 次 Missing 参数错误。

**位置**：
- `peri-middlewares/src/tools/filesystem/read.rs:122`
- `peri-middlewares/src/tools/filesystem/write.rs:64,67`
- `peri-middlewares/src/tools/filesystem/edit.rs:75,78,81`
- `peri-middlewares/src/tools/filesystem/glob.rs:131`

**改为**（英文，面向 LLM）：

| 工具 | 当前 | 改后 |
|------|------|------|
| Read | `"Missing file_path parameter"` | `"The 'file_path' parameter is required for the Read tool. Provide the absolute path to the file."` |
| Write | `"Missing file_path parameter"` | `"The 'file_path' parameter is required for the Write tool."` |
| Write | `"Missing content parameter"` | `"The 'content' parameter is required for the Write tool."` |
| Edit | `"Missing file_path parameter"` | `"The 'file_path' parameter is required for the Edit tool."` |
| Edit | `"Missing old_string parameter"` | `"The 'old_string' parameter is required for the Edit tool."` |
| Edit | `"Missing new_string parameter"` | `"The 'new_string' parameter is required for the Edit tool."` |
| Glob | `"Missing pattern parameter"` | `"The 'pattern' parameter is required for the Glob tool."` |

保持 `ok_or()` 返回 `Err` 风格不变。

---

### 3. OpenAI [ERROR] 前缀

**问题**：OpenAI Chat Completions API 没有 `is_error` 字段（Anthropic 有）。OpenAI 兼容 provider 的 LLM 无法区分成功和失败的 tool_result。

**位置**：
- `peri-agent/src/messages/adapters/openai.rs:141-150`（序列化）
- `peri-agent/src/messages/adapters/openai.rs:223-228`（解析）

**序列化改为**：
```rust
BaseMessage::Tool { tool_call_id, content, is_error, .. } => {
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

**解析改为**：识别 `[ERROR] ` 前缀，还原 `is_error` 标记。

---

### 4. AgentResult 轮询引导

**问题**：Agent 连续轮询 AgentResult 最多 83 次，每轮返回相同文本，LLM 不停止。

**位置**：`peri-middlewares/src/subagent/agent_result.rs:48-52`

**当前**：
```
"No completed background agent results available. Background agents may still be running or have not been started."
```

**改为**：
```
"No completed background agent results available yet. Do not retry this query — continue with other work instead. Background tasks will notify you when they complete. If you need the result later, use ExecuteExtraTool with tool_name 'AgentResult' after completing other tasks."
```

---

### 5. 连续失败检测

**问题**：Agent 对同一错误反复重试，没有打断机制。

**位置**：`peri-agent/src/agent/executor/tool_dispatch.rs`，`dispatch_tools` 函数中。

**设计**：

在 `dispatch_tools` 中维护一个错误追踪向量，检测连续相同错误模式：

```rust
// 在 dispatch_tools 中，写入 tool_result 后
if result.is_error {
    let error_key = format!("{}:{}", result.tool_name, result.output);
    consecutive_errors.push(error_key);
    
    // 检测连续 5 次相同错误
    if consecutive_errors.len() >= 5 {
        let last_5 = &consecutive_errors[consecutive_errors.len()-5..];
        if last_5.windows(2).all(|w| w[0] == w[1]) {
            // 注入纠正系统消息
            state.add_message(BaseMessage::system(
                format!(
                    "Warning: Tool '{}' has failed 5 consecutive times with the same error. \
                     Stop retrying and analyze the root cause. Consider using a different approach \
                     or asking the user for guidance.",
                    result.tool_name
                )
            ));
            tracing::warn!(tool = %result.tool_name, "连续 5 次相同错误，已注入纠正消息");
        }
    }
} else {
    // 成功时重置该工具的计数
    consecutive_errors.retain(|e| !e.starts_with(&format!("{}:", result.tool_name)));
}
```

**关键约束**：
- 追踪向量存在 `dispatch_tools` 的调用上下文中（非 ExecutorState），每轮 Agent execute 调用间不持久化
- 注入的是 `BaseMessage::system()`，进入 state.messages 但不影响 frozen_system_prompt
- 阈值：5 次连续相同错误（同工具名 + 同错误消息）

**测试**：
- 构造连续 5 次 Read + "Missing file_path" 错误序列
- 验证第 5 次后 state 中出现 system 纠正消息
- 验证成功调用后计数器重置

---

### 6. 输出截断 bug issue

**问题**：5月16日实现截断+磁盘持久化后，仍有 17 条 >100KB 的工具输出（最新一条 6月1日）。

**立为 issue**，排查方向：
- Bash 的 `truncate_output` 函数是否有路径绕过（如 stderr 未截断）
- Grep 的 head_limit 在 `files_with_matches` 模式下是否生效
- 截断后写入的 content 是否包含了 JSON 序列化开销（content 字段含 JSON 转义后膨胀）

## 不做的事

- ~~注册层统一归一化~~：只动查找层
- ~~框架级调用去重~~：靠工具侧引导（AgentResult）+ 连续失败检测
- ~~新增工具输出截断功能~~：已有实现，立 bug 排查
