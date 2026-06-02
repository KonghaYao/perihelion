//! Event mapping from ExecutorEvent to ACP SessionUpdate and peri/agent_event routing.
//!
//! Produces [`MappedEvent`] structs with three categories:
//! - **Category ①** (Enriched SessionUpdate): TextChunk, AiReasoning, ToolStart, ToolEnd, TodoUpdate,
//!   LlmCallEnd(usage) → `updates` only, `forward_to_tui: false`
//! - **Category ③** (TUI-only): StateSnapshot, Subagent*, Compact*, ContextWarning, LlmRetrying, etc.
//!   → `forward_to_tui: true` only
//! - **Filtered**: MessageAdded, LlmCallStart, LlmCallEnd(usage:None)
//!   → empty

use agent_client_protocol::schema::{
    ContentBlock, ContentChunk, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, SessionUpdate,
    TextContent, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    UsageUpdate,
};
use peri_agent::agent::events::AgentEvent as ExecutorEvent;

/// Result of mapping a single [`ExecutorEvent`].
///
/// Each ExecutorEvent produces zero or more `MappedEvent`s carrying:
/// - `updates`: standard ACP [`SessionUpdate`] list (for IDE/stdio clients)
/// - `forward_to_tui`: whether the event should also be sent via `peri/agent_event`
/// - `source_agent_id`: SubAgent routing hint
#[derive(Debug)]
pub struct MappedEvent {
    pub updates: Vec<SessionUpdate>,
    pub forward_to_tui: bool,
    pub source_agent_id: Option<String>,
}

impl MappedEvent {
    /// Category ①: full SessionUpdate, no TUI forwarding.
    pub fn standard(updates: Vec<SessionUpdate>) -> Self {
        Self {
            updates,
            forward_to_tui: false,
            source_agent_id: None,
        }
    }

    /// Category ① with source_agent_id extracted from the event.
    pub fn standard_with_src(updates: Vec<SessionUpdate>, source_agent_id: Option<String>) -> Self {
        Self {
            updates,
            forward_to_tui: false,
            source_agent_id,
        }
    }

    /// Category ③: TUI-only, no SessionUpdate.
    pub fn tui_only() -> Self {
        Self {
            updates: vec![],
            forward_to_tui: true,
            source_agent_id: None,
        }
    }

    /// Category ②: both SessionUpdate and TUI forwarding.
    pub fn both(updates: Vec<SessionUpdate>) -> Self {
        Self {
            updates,
            forward_to_tui: true,
            source_agent_id: None,
        }
    }

    /// Filtered: no output at all.
    pub fn none() -> Self {
        Self {
            updates: vec![],
            forward_to_tui: false,
            source_agent_id: None,
        }
    }
}

/// 将 ExecutorEvent 映射为 [`MappedEvent`] 列表。
///
/// `context_window` 是当前模型的上下文窗口大小（tokens），用于填充 UsageUpdate.size。
pub fn map_event(event: &ExecutorEvent, context_window: u32) -> Vec<MappedEvent> {
    match event {
        // ── Category ①: Full SessionUpdate ─────────────────────────────────────────
        ExecutorEvent::TextChunk {
            chunk,
            source_agent_id,
            ..
        } => {
            vec![MappedEvent::standard_with_src(
                vec![SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    ContentBlock::Text(TextContent::new(chunk.clone())),
                ))],
                source_agent_id.clone(),
            )]
        }

        ExecutorEvent::AiReasoning(text) => {
            vec![MappedEvent::standard(vec![
                SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(text.clone()),
                ))),
            ])]
        }

        ExecutorEvent::ToolStart {
            tool_call_id,
            name,
            input,
            source_agent_id,
            ..
        } => {
            vec![MappedEvent::standard_with_src(
                vec![SessionUpdate::ToolCall(
                    ToolCall::new(tool_call_id.clone(), name.clone())
                        .kind(infer_tool_kind(name))
                        .status(ToolCallStatus::InProgress)
                        .raw_input(Some(input.clone())),
                )],
                source_agent_id.clone(),
            )]
        }

        ExecutorEvent::ToolEnd {
            tool_call_id,
            name,
            output,
            is_error,
            source_agent_id,
            ..
        } => {
            let raw_output = match serde_json::from_str::<serde_json::Value>(output) {
                Ok(v) => Some(v),
                Err(_) => Some(serde_json::Value::String(output.clone())),
            };
            vec![MappedEvent::standard_with_src(
                vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    tool_call_id.clone(),
                    ToolCallUpdateFields::new()
                        .title(name.clone())
                        .status(if *is_error {
                            ToolCallStatus::Failed
                        } else {
                            ToolCallStatus::Completed
                        })
                        .raw_output(raw_output),
                ))],
                source_agent_id.clone(),
            )]
        }

        ExecutorEvent::TodoUpdate(entries) => {
            let plan_entries: Vec<PlanEntry> = entries
                .iter()
                .map(|e| {
                    PlanEntry::new(
                        e.content.clone(),
                        PlanEntryPriority::Medium,
                        match e.status {
                            peri_agent::agent::events::TodoStatus::Pending => {
                                PlanEntryStatus::Pending
                            }
                            peri_agent::agent::events::TodoStatus::InProgress => {
                                PlanEntryStatus::InProgress
                            }
                            peri_agent::agent::events::TodoStatus::Completed => {
                                PlanEntryStatus::Completed
                            }
                        },
                    )
                })
                .collect();
            vec![MappedEvent::standard(vec![SessionUpdate::Plan(Plan::new(
                plan_entries,
            ))])]
        }

        ExecutorEvent::LlmCallEnd {
            usage: Some(u),
            model,
            stop_reason,
            ..
        } => {
            let mut meta = serde_json::Map::new();
            meta.insert("inputTokens".into(), serde_json::json!(u.input_tokens));
            meta.insert("outputTokens".into(), serde_json::json!(u.output_tokens));
            if let Some(v) = u.cache_creation_input_tokens {
                meta.insert("cacheCreationTokens".into(), serde_json::json!(v));
            }
            if let Some(v) = u.cache_read_input_tokens {
                meta.insert("cacheReadTokens".into(), serde_json::json!(v));
            }
            meta.insert("model".into(), serde_json::json!(model));
            if let Some(ref sr) = stop_reason {
                meta.insert("stopReason".into(), serde_json::json!(sr.to_string()));
            }

            vec![MappedEvent::standard(vec![SessionUpdate::UsageUpdate(
                UsageUpdate::new(
                    u64::from(u.input_tokens) + u64::from(u.output_tokens),
                    u64::from(context_window),
                )
                .meta(meta),
            )])]
        }

        // ── Category ③: TUI-only (no SessionUpdate) ──────────────────────────────
        ExecutorEvent::ContextWarning { .. }
        | ExecutorEvent::LlmRetrying { .. }
        | ExecutorEvent::StateSnapshot(_)
        | ExecutorEvent::SubagentStarted { .. }
        | ExecutorEvent::SubagentStopped { .. }
        | ExecutorEvent::CompactStarted
        | ExecutorEvent::CompactCompleted { .. }
        | ExecutorEvent::CompactError { .. }
        | ExecutorEvent::RewindCompleted { .. }
        | ExecutorEvent::BackgroundTaskCompleted(_)
        | ExecutorEvent::LspDiagnostics { .. }
        | ExecutorEvent::AgentExecutionFailed { .. } => {
            vec![MappedEvent::tui_only()]
        }

        // ── Filtered: no output ───────────────────────────────────────────────────
        ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. }
        | ExecutorEvent::LlmCallEnd { usage: None, .. } => {
            vec![MappedEvent::none()]
        }
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

#[cfg(test)]
#[path = "mapper_test.rs"]
mod tests;
