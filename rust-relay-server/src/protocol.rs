use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent → Relay → Web (event push)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayMessage {
    /// deprecated: 不再主动发送，实时事件改为扁平化 JSON + seq 直接推送
    AgentEvent {
        event: rust_create_agent::agent::AgentEvent,
    },
    /// Sync 响应：历史事件批量推送（Agent → Web）
    SyncResponse {
        events: Vec<serde_json::Value>,
    },
    /// HITL 审批请求（Agent → Web）
    ApprovalNeeded {
        items: Vec<ApprovalItem>,
    },
    /// AskUser 提问请求（Agent → Web）
    AskUserBatch {
        questions: Vec<AskUserQuestion>,
    },
    /// HITL 审批已解决（任意一端确认后广播给所有端）
    ApprovalResolved {
        items: Vec<String>,
    },
    /// AskUser 已解决（任意一端确认后广播给所有端）
    AskUserResolved,
    /// TODO 列表更新（Agent → Web）
    TodoUpdate {
        items: Vec<TodoItemInfo>,
    },
    SessionId {
        session_id: String,
    },
    Ping,
    /// 增量消息批量推送（BaseMessage JSON + seq），替代扁平化事件
    MessageBatch {
        messages: Vec<serde_json::Value>,
    },
}

/// HITL 审批项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalItem {
    pub tool_name: String,
    pub input: serde_json::Value,
}

/// AskUser 问题项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserQuestion {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
}

/// TODO 项信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItemInfo {
    pub content: String,
    pub status: String,
}

/// Web → Relay → Agent (user actions)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebMessage {
    UserInput {
        text: String,
    },
    HitlDecision {
        decisions: Vec<HitlDecisionItem>,
    },
    AskUserResponse {
        answers: HashMap<String, String>,
    },
    ClearThread,
    Pong,
    SyncRequest {
        since_seq: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlDecisionItem {
    pub tool_call_id: String,
    pub decision: String,
    #[serde(default)]
    pub input: Option<String>,
}

/// Relay → all Web clients (broadcast)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BroadcastMessage {
    AgentOnline {
        session_id: String,
        name: Option<String>,
        connected_at: String,
    },
    AgentOffline {
        session_id: String,
    },
    AgentsList {
        agents: Vec<AgentInfo>,
    },
}

/// Relay error messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayError {
    Error { code: String, message: String },
}

/// Agent info for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub session_id: String,
    pub name: Option<String>,
    pub connected_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_message_serialization() {
        let msg = RelayMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ping\""));

        let msg = RelayMessage::SessionId {
            session_id: "test-123".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"session_id\""));
        assert!(json.contains("test-123"));
    }

    #[test]
    fn test_web_message_serialization() {
        let msg = WebMessage::UserInput {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"user_input\""));

        let msg = WebMessage::ClearThread;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"clear_thread\""));
    }

    #[test]
    fn test_broadcast_message_serialization() {
        let msg = BroadcastMessage::AgentOnline {
            session_id: "abc".into(),
            name: Some("Agent-A".into()),
            connected_at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"agent_online\""));
        assert!(json.contains("Agent-A"));
    }

    #[test]
    fn test_web_message_deserialization() {
        let json = r#"{"type":"user_input","text":"hello world"}"#;
        let msg: WebMessage = serde_json::from_str(json).unwrap();
        match msg {
            WebMessage::UserInput { text } => assert_eq!(text, "hello world"),
            _ => unreachable!("Expected UserInput"),
        }
    }

    #[test]
    fn test_sync_request_serialization() {
        let msg = WebMessage::SyncRequest { since_seq: 42 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"sync_request\""), "json: {}", json);
        assert!(json.contains("\"since_seq\":42"), "json: {}", json);
    }

    #[test]
    fn test_sync_request_deserialization() {
        let json = r#"{"type":"sync_request","since_seq":100}"#;
        let msg: WebMessage = serde_json::from_str(json).unwrap();
        match msg {
            WebMessage::SyncRequest { since_seq } => assert_eq!(since_seq, 100),
            _ => unreachable!("Expected SyncRequest"),
        }
    }

    #[test]
    fn test_sync_response_serialization() {
        let msg = RelayMessage::SyncResponse { events: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"sync_response\""), "json: {}", json);
    }

    #[test]
    fn test_message_batch_serialization() {
        let msg = RelayMessage::MessageBatch { messages: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"message_batch\""), "json: {}", json);
    }

    #[test]
    fn test_approval_resolved_serialization() {
        let msg = RelayMessage::ApprovalResolved {
            items: vec!["bash".into(), "write_file".into()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"approval_resolved\""), "json: {}", json);
        assert!(json.contains("bash"));
    }

    #[test]
    fn test_ask_user_resolved_serialization() {
        let msg = RelayMessage::AskUserResolved;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ask_user_resolved\""), "json: {}", json);
    }
}
