use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent → Relay → Web (event push)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayMessage {
    AgentEvent {
        event: rust_create_agent::agent::AgentEvent,
    },
    /// HITL 审批请求（Agent → Web）
    ApprovalNeeded {
        items: Vec<ApprovalItem>,
    },
    /// AskUser 提问请求（Agent → Web）
    AskUserBatch {
        questions: Vec<AskUserQuestion>,
    },
    /// TODO 列表更新（Agent → Web）
    TodoUpdate {
        items: Vec<TodoItemInfo>,
    },
    SessionId {
        session_id: String,
    },
    Ping,
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
            _ => panic!("Expected UserInput"),
        }
    }
}
