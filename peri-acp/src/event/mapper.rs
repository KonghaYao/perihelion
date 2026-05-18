//! Event mapping from ExecutorEvent to ACP SessionUpdate and peri/* custom notifications.
//!
//! Translates peri-agent executor events into standard ACP session notifications
//! for consumption by TUI or other frontends, plus peri/* custom notifications
//! for SubAgent, Compact, LSP, Background tasks, and Session lifecycle events.

use agent_client_protocol::schema::{
    Content, ContentBlock, ContentChunk, SessionInfoUpdate, SessionUpdate, TextContent, ToolCall,
    ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, UsageUpdate,
};
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use serde_json::json;

/// 直接将 ExecutorEvent 映射为 ACP SessionUpdate（ACP 模式专用，无 TUI 依赖）
///
/// `context_window` 是当前模型的上下文窗口大小（tokens），用于填充 UsageUpdate.size。
pub fn map_executor_to_updates(event: &ExecutorEvent, context_window: u32) -> Vec<SessionUpdate> {
    match event {
        ExecutorEvent::TextChunk { chunk, .. } => {
            vec![SessionUpdate::AgentMessageChunk(ContentChunk::new(
                ContentBlock::Text(TextContent::new(chunk.clone())),
            ))]
        }
        ExecutorEvent::AiReasoning(text) => {
            vec![SessionUpdate::AgentThoughtChunk(ContentChunk::new(
                ContentBlock::Text(TextContent::new(text.clone())),
            ))]
        }
        ExecutorEvent::ToolStart {
            tool_call_id,
            name,
            input,
            ..
        } => {
            let args_str = input.to_string();
            vec![SessionUpdate::ToolCall(
                ToolCall::new(tool_call_id.clone(), name.clone())
                    .kind(infer_tool_kind(name))
                    .status(ToolCallStatus::InProgress)
                    .content(vec![ToolCallContent::Content(Content::new(
                        ContentBlock::Text(TextContent::new(truncate_str(&args_str, 500))),
                    ))])
                    .raw_input(Some(input.clone())),
            )]
        }
        ExecutorEvent::ToolEnd {
            tool_call_id,
            output,
            is_error,
            ..
        } => {
            let raw_output = match serde_json::from_str::<serde_json::Value>(output) {
                Ok(v) => Some(v),
                Err(_) => Some(serde_json::Value::String(output.clone())),
            };
            vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                tool_call_id.clone(),
                ToolCallUpdateFields::new()
                    .status(if *is_error {
                        ToolCallStatus::Failed
                    } else {
                        ToolCallStatus::Completed
                    })
                    .content(vec![ToolCallContent::Content(Content::new(
                        ContentBlock::Text(TextContent::new(truncate_str(output, 500))),
                    ))])
                    .raw_output(raw_output),
            ))]
        }
        ExecutorEvent::LlmCallEnd { usage: Some(u), .. } => {
            vec![SessionUpdate::UsageUpdate(UsageUpdate::new(
                u64::from(u.input_tokens) + u64::from(u.output_tokens),
                u64::from(context_window),
            ))]
        }
        ExecutorEvent::ContextWarning {
            used_tokens,
            total_tokens,
            ..
        } => {
            vec![SessionUpdate::UsageUpdate(UsageUpdate::new(
                *used_tokens,
                *total_tokens,
            ))]
        }
        ExecutorEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            ..
        } => {
            vec![SessionUpdate::SessionInfoUpdate(
                SessionInfoUpdate::new().title(format!(
                    "Retrying LLM call (attempt {}/{}, {}ms delay)",
                    attempt, max_attempts, delay_ms
                )),
            )]
        }
        // 内部事件、LLM 调用事件等不映射
        _ => vec![],
    }
}

fn infer_tool_kind(name: &str) -> ToolKind {
    match name {
        "Read" => ToolKind::Read,
        "Write" | "Edit" | "folder_operations" => ToolKind::Edit,
        "Bash" => ToolKind::Execute,
        "Grep" | "Glob" => ToolKind::Search,
        "WebFetch" | "WebSearch" => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s.floor_char_boundary(max_len);
        format!("{}...", &s[..boundary])
    }
}

// ── peri/* custom notification mapping ────────────────────────────────────────────

/// 将 ExecutorEvent 映射为 `peri/*` 自定义通知列表。
///
/// 仅包含 TUI 通过 `map_executor_event` 过滤掉（返回 None）的事件：
/// - CompactStarted → `notifications/peri/compact/start`
/// - CompactCompleted → `notifications/peri/compact/end`
/// - SessionEnded → `notifications/peri/session/ended`
///
/// 其余事件通过 `notifications/agent_event` 由 `map_executor_event` 统一处理。
pub fn map_executor_to_peri_notifications(
    event: &ExecutorEvent,
) -> Vec<(&'static str, serde_json::Value)> {
    match event {
        ExecutorEvent::CompactStarted => {
            vec![("notifications/peri/compact/start", json!({}))]
        }
        ExecutorEvent::CompactCompleted => {
            vec![("notifications/peri/compact/end", json!({}))]
        }
        ExecutorEvent::SessionEnded => {
            vec![("notifications/peri/session/ended", json!({}))]
        }
        _ => vec![],
    }
}
