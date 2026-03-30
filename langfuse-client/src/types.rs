use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 观测类型（V4 扩展，含 10 种变体）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationType {
    Span,
    Generation,
    Event,
    Agent,
    Tool,
    Chain,
    Retriever,
    Evaluator,
    Embedding,
    Guardrail,
}

/// 观测日志级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationLevel {
    Debug,
    Default,
    Warning,
    Error,
}

/// 评分数据类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScoreDataType {
    Numeric,
    Boolean,
    Categorical,
    Correction,
}

/// Langfuse Usage（legacy，API required 字段为 input/output/total）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: i32,
    pub output: i32,
    pub total: i32,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub total_cost: Option<f64>,
    pub unit: Option<String>,
}

/// UsageDetails — 灵活的 key-value map
pub type UsageDetails = HashMap<String, i32>;

/// CostDetails — 成本详情 map
pub type CostDetails = HashMap<String, f64>;

/// IngestionUsage — 兼容 Usage 和 OpenAIUsage 的灵活格式
pub type IngestionUsage = HashMap<String, serde_json::Value>;

/// Trace 创建/更新的 Body
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TraceBody {
    pub id: Option<String>,
    pub name: Option<String>,
    pub user_id: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub release: Option<String>,
    pub version: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
    pub environment: Option<String>,
    pub public: Option<bool>,
    pub timestamp: Option<String>,
}

/// V4 统一观测类型（ObservationCreate/ObservationUpdate 共用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ObservationBody {
    pub id: Option<String>,
    pub trace_id: Option<String>,
    pub r#type: ObservationType,
    pub name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub completion_start_time: Option<String>,
    pub parent_observation_id: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub model: Option<String>,
    pub model_parameters: Option<HashMap<String, serde_json::Value>>,
    pub level: Option<ObservationLevel>,
    pub status_message: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
}

/// Span Body（SpanCreate/SpanUpdate 共用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SpanBody {
    pub id: Option<String>,
    pub trace_id: Option<String>,
    pub name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub level: Option<ObservationLevel>,
    pub status_message: Option<String>,
    pub parent_observation_id: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
}

/// Generation Body（GenerationCreate/GenerationUpdate 共用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct GenerationBody {
    // From OptionalObservationBody
    pub id: Option<String>,
    pub trace_id: Option<String>,
    pub name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub level: Option<ObservationLevel>,
    pub status_message: Option<String>,
    pub parent_observation_id: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    // Generation-specific fields
    pub completion_start_time: Option<String>,
    pub model: Option<String>,
    pub model_parameters: Option<HashMap<String, serde_json::Value>>,
    pub usage: Option<IngestionUsage>,
    pub usage_details: Option<UsageDetails>,
    pub cost_details: Option<CostDetails>,
    pub prompt_name: Option<String>,
    pub prompt_version: Option<i32>,
}

/// Event Body（EventCreate 使用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct EventBody {
    pub id: Option<String>,
    pub trace_id: Option<String>,
    pub name: Option<String>,
    pub start_time: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub level: Option<ObservationLevel>,
    pub status_message: Option<String>,
    pub parent_observation_id: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
}

/// Score Body（ScoreCreate 使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ScoreBody {
    pub name: String,
    pub value: serde_json::Value,
    pub id: Option<String>,
    pub trace_id: Option<String>,
    pub observation_id: Option<String>,
    pub comment: Option<String>,
    pub data_type: Option<ScoreDataType>,
    pub config_id: Option<String>,
    pub queue_id: Option<String>,
    pub environment: Option<String>,
    pub session_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub dataset_run_id: Option<String>,
}

/// SDK Log Body（SdkLog 使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SdkLogBody {
    pub log: serde_json::Value,
}

/// Ingestion 事件统一枚举（10 种变体）
/// 通过 serde 内部标签自动序列化 `type` 判别字段
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum IngestionEvent {
    TraceCreate {
        id: String,
        timestamp: String,
        body: TraceBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    SpanCreate {
        id: String,
        timestamp: String,
        body: SpanBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    SpanUpdate {
        id: String,
        timestamp: String,
        body: SpanBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    GenerationCreate {
        id: String,
        timestamp: String,
        body: GenerationBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    GenerationUpdate {
        id: String,
        timestamp: String,
        body: GenerationBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    EventCreate {
        id: String,
        timestamp: String,
        body: EventBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    ScoreCreate {
        id: String,
        timestamp: String,
        body: ScoreBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    ObservationCreate {
        id: String,
        timestamp: String,
        body: ObservationBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    ObservationUpdate {
        id: String,
        timestamp: String,
        body: ObservationBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    SdkLog {
        id: String,
        timestamp: String,
        body: SdkLogBody,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
}

/// 207 Multi-Status 响应中的成功项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionSuccess {
    pub id: String,
    pub status: i32,
}

/// 207 Multi-Status 响应中的错误项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionError {
    pub id: String,
    pub status: i32,
    pub message: Option<String>,
    pub error: Option<serde_json::Value>,
}

/// Ingestion API 的 207 Multi-Status 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResponse {
    pub successes: Vec<IngestionSuccess>,
    pub errors: Vec<IngestionError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trace_body() -> TraceBody {
        TraceBody {
            id: Some("trace-1".into()),
            name: Some("test-trace".into()),
            ..Default::default()
        }
    }

    fn make_span_body() -> SpanBody {
        SpanBody {
            id: Some("span-1".into()),
            trace_id: Some("trace-1".into()),
            name: Some("test-span".into()),
            start_time: Some("2026-01-01T00:00:00Z".into()),
            end_time: Some("2026-01-01T00:01:00Z".into()),
            parent_observation_id: Some("parent-1".into()),
            ..Default::default()
        }
    }

    fn make_observation_body() -> ObservationBody {
        ObservationBody {
            id: Some("obs-1".into()),
            trace_id: Some("trace-1".into()),
            r#type: ObservationType::Agent,
            name: Some("Agent".into()),
            start_time: Some("2026-01-01T00:00:00Z".into()),
            input: Some(serde_json::json!("hello")),
            end_time: None,
            completion_start_time: None,
            parent_observation_id: None,
            output: None,
            metadata: None,
            model: None,
            model_parameters: None,
            level: None,
            status_message: None,
            version: None,
            environment: None,
        }
    }

    fn make_generation_body() -> GenerationBody {
        let mut usage_details = HashMap::new();
        usage_details.insert("input".to_string(), 100);
        usage_details.insert("output".to_string(), 50);

        let mut model_params = HashMap::new();
        model_params.insert("temperature".to_string(), serde_json::json!(0.7));

        let mut usage = HashMap::new();
        usage.insert("input".to_string(), serde_json::json!(100));
        usage.insert("output".to_string(), serde_json::json!(50));

        GenerationBody {
            id: Some("gen-1".into()),
            trace_id: Some("trace-1".into()),
            name: Some("ChatClaude".into()),
            model: Some("claude-3.5-sonnet".into()),
            start_time: Some("2026-01-01T00:00:00Z".into()),
            end_time: Some("2026-01-01T00:01:00Z".into()),
            input: Some(serde_json::json!("hello")),
            output: Some(serde_json::json!("world")),
            usage: Some(usage),
            usage_details: Some(usage_details),
            model_parameters: Some(model_params),
            ..Default::default()
        }
    }

    fn make_event_body() -> EventBody {
        EventBody {
            id: Some("evt-1".into()),
            trace_id: Some("trace-1".into()),
            name: Some("test-event".into()),
            input: Some(serde_json::json!("hello")),
            output: Some(serde_json::json!("world")),
            ..Default::default()
        }
    }

    // Enum serde tests
    #[test]
    fn test_observation_type_serde() {
        let json = serde_json::to_string(&ObservationType::Span).unwrap();
        assert_eq!(json, "\"SPAN\"");
        let back: ObservationType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ObservationType::Span);
    }

    #[test]
    fn test_observation_level_serde() {
        let json = serde_json::to_string(&ObservationLevel::Warning).unwrap();
        assert_eq!(json, "\"WARNING\"");
        let back: ObservationLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ObservationLevel::Warning);
    }

    #[test]
    fn test_score_data_type_serde() {
        let json = serde_json::to_string(&ScoreDataType::Categorical).unwrap();
        assert_eq!(json, "\"CATEGORICAL\"");
        let back: ScoreDataType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ScoreDataType::Categorical);
    }

    // Usage tests
    #[test]
    fn test_usage_serde() {
        let usage = Usage { input: 100, output: 50, total: 150, ..Default::default() };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"input\":100"));
        assert!(json.contains("\"output\":50"));
        assert!(json.contains("\"total\":150"));
        let back: Usage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.input, 100);
        assert_eq!(back.output, 50);
        assert_eq!(back.total, 150);
    }

    #[test]
    fn test_usage_details_serde() {
        let mut details = UsageDetails::new();
        details.insert("input".to_string(), 100);
        details.insert("cache_read_input_tokens".to_string(), 30);
        let json = serde_json::to_string(&details).unwrap();
        let back: UsageDetails = serde_json::from_str(&json).unwrap();
        assert_eq!(back["input"], 100);
        assert_eq!(back["cache_read_input_tokens"], 30);
    }

    // Body roundtrip tests
    #[test]
    fn test_trace_body_serde_minimal() {
        let body = TraceBody {
            id: Some("trace-1".into()),
            name: Some("test".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&body).unwrap();
        // null fields should not appear when skip_serializing_if is used
        // Without skip_serializing_if, serde serializes None as null
        let back: TraceBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, Some("trace-1".into()));
        assert!(back.user_id.is_none());
    }

    #[test]
    fn test_trace_body_serde_full() {
        let body = TraceBody {
            id: Some("trace-1".into()),
            name: Some("test".into()),
            user_id: Some("user-1".into()),
            input: Some(serde_json::json!("hello")),
            output: Some(serde_json::json!("world")),
            session_id: Some("sess-1".into()),
            release: Some("1.0".into()),
            version: Some("2.0".into()),
            metadata: Some(serde_json::json!({"key": "val"})),
            tags: Some(vec!["tag1".into(), "tag2".into()]),
            environment: Some("prod".into()),
            public: Some(true),
            timestamp: Some("2026-01-01T00:00:00Z".into()),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"tags\":[\"tag1\",\"tag2\"]"));
        assert!(json.contains("\"public\":true"));
        let back: TraceBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tags, Some(vec!["tag1".into(), "tag2".into()]));
    }

    #[test]
    fn test_observation_body_serde() {
        let body = make_observation_body();
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"type\":\"AGENT\""));
        let back: ObservationBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.r#type, ObservationType::Agent);
    }

    #[test]
    fn test_span_body_serde() {
        let body = make_span_body();
        let json = serde_json::to_string(&body).unwrap();
        let back: SpanBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, Some("span-1".into()));
        assert_eq!(back.parent_observation_id, Some("parent-1".into()));
    }

    #[test]
    fn test_generation_body_serde() {
        let body = make_generation_body();
        let json = serde_json::to_string(&body).unwrap();
        // Verify camelCase
        assert!(json.contains("\"model\":\"claude-3.5-sonnet\""));
        assert!(json.contains("\"usageDetails\""));
        let back: GenerationBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, Some("claude-3.5-sonnet".into()));
        assert!(back.usage_details.is_some());
    }

    #[test]
    fn test_event_body_serde() {
        let body = make_event_body();
        let json = serde_json::to_string(&body).unwrap();
        let back: EventBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, Some("evt-1".into()));
    }

    // ScoreBody tests
    #[test]
    fn test_score_body_serde_numeric() {
        let body = ScoreBody {
            name: "accuracy".into(),
            value: serde_json::json!(0.95),
            id: None,
            trace_id: None,
            observation_id: None,
            comment: None,
            data_type: None,
            config_id: None,
            queue_id: None,
            environment: None,
            session_id: None,
            metadata: None,
            dataset_run_id: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"value\":0.95"));
        let back: ScoreBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "accuracy");
    }

    #[test]
    fn test_score_body_serde_string() {
        let body = ScoreBody {
            name: "category".into(),
            value: serde_json::json!("category-a"),
            id: None,
            trace_id: None,
            observation_id: None,
            comment: None,
            data_type: None,
            config_id: None,
            queue_id: None,
            environment: None,
            session_id: None,
            metadata: None,
            dataset_run_id: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"value\":\"category-a\""));
        let back: ScoreBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "category");
    }

    #[test]
    fn test_sdk_log_body_serde() {
        let body = SdkLogBody {
            log: serde_json::json!({"message": "test"}),
        };
        let json = serde_json::to_string(&body).unwrap();
        let back: SdkLogBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.log["message"], "test");
    }

    // IngestionEvent tests
    #[test]
    fn test_ingestion_event_trace_create() {
        let event = IngestionEvent::TraceCreate {
            id: "evt-1".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_trace_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"trace-create\""));
        // metadata uses skip_serializing_if
        let back: IngestionEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, IngestionEvent::TraceCreate { .. }));
    }

    #[test]
    fn test_ingestion_event_span_create() {
        let event = IngestionEvent::SpanCreate {
            id: "evt-2".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_span_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"span-create\""));
    }

    #[test]
    fn test_ingestion_event_span_update() {
        let event = IngestionEvent::SpanUpdate {
            id: "evt-3".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_span_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"span-update\""));
    }

    #[test]
    fn test_ingestion_event_generation_create() {
        let event = IngestionEvent::GenerationCreate {
            id: "evt-4".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_generation_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"generation-create\""));
        assert!(json.contains("\"model\":\"claude-3.5-sonnet\""));
    }

    #[test]
    fn test_ingestion_event_generation_update() {
        let event = IngestionEvent::GenerationUpdate {
            id: "evt-5".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_generation_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"generation-update\""));
    }

    #[test]
    fn test_ingestion_event_event_create() {
        let event = IngestionEvent::EventCreate {
            id: "evt-6".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_event_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"event-create\""));
    }

    #[test]
    fn test_ingestion_event_score_create() {
        let body = ScoreBody {
            name: "accuracy".into(),
            value: serde_json::json!(0.95),
            id: None,
            trace_id: None,
            observation_id: None,
            comment: None,
            data_type: None,
            config_id: None,
            queue_id: None,
            environment: None,
            session_id: None,
            metadata: None,
            dataset_run_id: None,
        };
        let event = IngestionEvent::ScoreCreate {
            id: "evt-7".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body,
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"score-create\""));
    }

    #[test]
    fn test_ingestion_event_observation_create() {
        let event = IngestionEvent::ObservationCreate {
            id: "evt-8".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_observation_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"observation-create\""));
        assert!(json.contains("\"type\":\"AGENT\""));
    }

    #[test]
    fn test_ingestion_event_observation_update() {
        let event = IngestionEvent::ObservationUpdate {
            id: "evt-9".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_observation_body(),
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"observation-update\""));
    }

    #[test]
    fn test_ingestion_event_sdk_log() {
        let body = SdkLogBody {
            log: serde_json::json!({"message": "test"}),
        };
        let event = IngestionEvent::SdkLog {
            id: "evt-10".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body,
            metadata: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"sdk-log\""));
    }

    #[test]
    fn test_ingestion_event_with_metadata() {
        let event = IngestionEvent::TraceCreate {
            id: "evt-meta".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            body: make_trace_body(),
            metadata: Some(serde_json::json!({"sdk": "rust"})),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"metadata\":{\"sdk\":\"rust\"}"));
    }

    #[test]
    fn test_ingestion_response() {
        let resp = IngestionResponse {
            successes: vec![IngestionSuccess { id: "1".into(), status: 200 }],
            errors: vec![IngestionError {
                id: "2".into(),
                status: 400,
                message: Some("bad".into()),
                error: None,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: IngestionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.successes.len(), 1);
        assert_eq!(back.errors.len(), 1);
        assert_eq!(back.errors[0].message, Some("bad".into()));
    }

    #[test]
    fn test_ingestion_response_empty() {
        let resp = IngestionResponse {
            successes: vec![],
            errors: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: IngestionResponse = serde_json::from_str(&json).unwrap();
        assert!(back.successes.is_empty());
        assert!(back.errors.is_empty());
    }

    #[test]
    fn test_batch_of_events_serde() {
        let events: Vec<IngestionEvent> = vec![
            IngestionEvent::TraceCreate {
                id: "1".into(),
                timestamp: "2026-01-01T00:00:00Z".into(),
                body: make_trace_body(),
                metadata: None,
            },
            IngestionEvent::SpanCreate {
                id: "2".into(),
                timestamp: "2026-01-01T00:00:00Z".into(),
                body: make_span_body(),
                metadata: None,
            },
            IngestionEvent::ObservationCreate {
                id: "3".into(),
                timestamp: "2026-01-01T00:00:00Z".into(),
                body: make_observation_body(),
                metadata: None,
            },
        ];
        let json = serde_json::to_string(&events).unwrap();
        let back: Vec<IngestionEvent> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 3);
    }
}
