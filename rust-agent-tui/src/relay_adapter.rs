use rust_create_agent::agent::events::AgentEvent as ExecutorEvent;
use rust_relay_server::protocol_types::RelayAgentEvent;

/// 将 ExecutorEvent 转换为 RelayAgentEvent；
/// 不需要转发的内部事件（StateSnapshot、MessageAdded）返回 None。
/// MessageAdded 由 agent.rs 单独通过 send_message 路径处理，以保持原有序列化格式。
pub fn to_relay_event(event: &ExecutorEvent) -> Option<RelayAgentEvent> {
    Some(match event {
        ExecutorEvent::AiReasoning(text) => RelayAgentEvent::AiReasoning {
            text: text.clone(),
        },
        ExecutorEvent::TextChunk { message_id, chunk } => RelayAgentEvent::TextChunk {
            message_id: message_id.as_uuid().to_string(),
            chunk: chunk.clone(),
        },
        ExecutorEvent::ToolStart {
            message_id,
            tool_call_id,
            name,
            input,
        } => RelayAgentEvent::ToolStart {
            message_id: message_id.as_uuid().to_string(),
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        ExecutorEvent::ToolEnd {
            message_id,
            tool_call_id,
            name,
            output,
            is_error,
        } => RelayAgentEvent::ToolEnd {
            message_id: message_id.as_uuid().to_string(),
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            output: output.clone(),
            is_error: *is_error,
        },
        ExecutorEvent::StepDone { step } => RelayAgentEvent::StepDone { step: *step },
        ExecutorEvent::LlmCallStart {
            step,
            messages,
            tools,
        } => RelayAgentEvent::LlmCallStart {
            step: *step,
            messages: messages
                .iter()
                .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                .collect(),
            tools: tools
                .iter()
                .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null))
                .collect(),
        },
        ExecutorEvent::LlmCallEnd {
            step,
            model,
            output,
            usage,
        } => RelayAgentEvent::LlmCallEnd {
            step: *step,
            model: model.clone(),
            output: output.clone(),
            usage: usage
                .as_ref()
                .and_then(|u| serde_json::to_value(u).ok()),
        },
        // MessageAdded 由调用方单独处理（保留原有序列化格式）
        ExecutorEvent::MessageAdded(_) => return None,
        // StateSnapshot 不转发到 relay（避免大量历史数据推送）
        ExecutorEvent::StateSnapshot(_) => return None,
    })
}
