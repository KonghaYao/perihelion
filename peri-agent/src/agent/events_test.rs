use super::*;

#[test]
fn test_context_warning_serde_roundtrip() {
    let ev = AgentEvent::ContextWarning {
        used_tokens: 150000,
        total_tokens: 200000,
        percentage: 75.0,
    };
    let json = serde_json::to_string(&ev).unwrap();
    let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
    if let AgentEvent::ContextWarning {
        used_tokens,
        total_tokens,
        percentage,
    } = deserialized
    {
        assert_eq!(used_tokens, 150000);
        assert_eq!(total_tokens, 200000);
        assert!((percentage - 75.0).abs() < 0.01);
    } else {
        panic!("Deserialized to wrong variant");
    }
}

#[test]
fn test_llm_retrying_serde_roundtrip() {
    let ev = AgentEvent::LlmRetrying {
        attempt: 2,
        max_attempts: 5,
        delay_ms: 2000,
        error: "API 错误 503: Service Unavailable".to_string(),
    };
    let json = serde_json::to_string(&ev).unwrap();
    let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
    if let AgentEvent::LlmRetrying {
        attempt,
        max_attempts,
        delay_ms,
        error,
    } = deserialized
    {
        assert_eq!(attempt, 2);
        assert_eq!(max_attempts, 5);
        assert_eq!(delay_ms, 2000);
        assert_eq!(error, "API 错误 503: Service Unavailable");
    } else {
        panic!("Deserialized to wrong variant");
    }
}

#[test]
fn test_subagent_started_serde_roundtrip() {
    let ev = AgentEvent::SubagentStarted {
        agent_name: "test-agent".to_string(),
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains(r#""type":"subagent_started""#));
    assert!(json.contains(r#""agent_name":"test-agent""#));
    let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
    if let AgentEvent::SubagentStarted { agent_name } = deserialized {
        assert_eq!(agent_name, "test-agent");
    } else {
        panic!("Deserialized to wrong variant");
    }
}

#[test]
fn test_subagent_stopped_serde_roundtrip() {
    let ev = AgentEvent::SubagentStopped {
        agent_name: "test-agent".to_string(),
        result: "done".to_string(),
        is_error: false,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains(r#""type":"subagent_stopped""#));
    let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
    if let AgentEvent::SubagentStopped {
        agent_name,
        result,
        is_error,
    } = deserialized
    {
        assert_eq!(agent_name, "test-agent");
        assert_eq!(result, "done");
        assert!(!is_error);
    } else {
        panic!("Deserialized to wrong variant");
    }
}

#[test]
fn test_session_ended_serde() {
    let ev = AgentEvent::SessionEnded;
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains(r#""type":"session_ended""#));
    let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, AgentEvent::SessionEnded));
}

#[test]
fn test_compact_events_serde() {
    let ev1 = AgentEvent::CompactStarted;
    let json1 = serde_json::to_string(&ev1).unwrap();
    assert!(json1.contains(r#""type":"compact_started""#));

    let ev2 = AgentEvent::CompactCompleted;
    let json2 = serde_json::to_string(&ev2).unwrap();
    assert!(json2.contains(r#""type":"compact_completed""#));

    let d1: AgentEvent = serde_json::from_str(&json1).unwrap();
    assert!(matches!(d1, AgentEvent::CompactStarted));
    let d2: AgentEvent = serde_json::from_str(&json2).unwrap();
    assert!(matches!(d2, AgentEvent::CompactCompleted));
}
