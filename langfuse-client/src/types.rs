use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ─── OTLP (OpenTelemetry Protocol) Types ───────────────────────────
// These types represent the OTLP HTTP/JSON payload for trace ingestion.
// Endpoint: POST /api/public/otel/v1/traces
// Spec: https://opentelemetry.io/docs/specs/otlp/

/// OTLP trace export request body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelTraceExportRequest {
    #[serde(rename = "resourceSpans")]
    pub resource_spans: Vec<OtelResourceSpan>,
}

/// A collection of spans from a single resource
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelResourceSpan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<OtelResource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_spans: Option<Vec<OtelScopeSpan>>,
}

/// Resource attributes identifying the source of telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelResource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<OtelAttribute>>,
}

/// Collection of spans from a single instrumentation scope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelScopeSpan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<OtelScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans: Option<Vec<OtelSpan>>,
}

/// Instrumentation scope information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<OtelAttribute>>,
}

/// Individual OTLP span representing a unit of work
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelSpan {
    /// Trace ID — 16 bytes hex-encoded (32 chars), must NOT contain hyphens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Span ID — 8 bytes hex-encoded (16 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Parent span ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    /// Span name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Span kind: 1=INTERNAL, 2=SERVER, 3=CLIENT, 4=PRODUCER, 5=CONSUMER
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<i32>,
    /// Start time in nanoseconds since Unix epoch (string representation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_unix_nano: Option<String>,
    /// End time in nanoseconds since Unix epoch (string representation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_unix_nano: Option<String>,
    /// Span attributes (langfuse.* namespace for Langfuse-specific mapping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<OtelAttribute>>,
    /// Span status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OtelStatus>,
}

/// Span status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OtelStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Key-value attribute pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelAttribute {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<OtelAttributeValue>,
}

/// Attribute value wrapper supporting different value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelAttributeValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
}

impl OtelAttributeValue {
    pub fn string(v: impl Into<String>) -> Self {
        Self {
            string_value: Some(v.into()),
            int_value: None,
            double_value: None,
            bool_value: None,
        }
    }

    pub fn int(v: i64) -> Self {
        Self {
            string_value: None,
            int_value: Some(v),
            double_value: None,
            bool_value: None,
        }
    }

    pub fn bool(v: bool) -> Self {
        Self {
            string_value: None,
            int_value: None,
            double_value: None,
            bool_value: Some(v),
        }
    }
}

/// Helper to build an attribute
impl OtelAttribute {
    pub fn new(key: impl Into<String>, value: OtelAttributeValue) -> Self {
        Self {
            key: key.into(),
            value: Some(value),
        }
    }

    pub fn string(key: impl Into<String>, val: impl Into<String>) -> Self {
        Self::new(key, OtelAttributeValue::string(val))
    }
}

/// OTLP trace export response (empty object = success)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelTraceResponse {}

// ─── IngestionEvent helpers ─────────────────────────────────────────

impl IngestionEvent {
    /// Get the event envelope timestamp (all variants have a `timestamp` field)
    pub fn event_timestamp(&self) -> &str {
        match self {
            IngestionEvent::TraceCreate { timestamp, .. } => timestamp,
            IngestionEvent::SpanCreate { timestamp, .. } => timestamp,
            IngestionEvent::SpanUpdate { timestamp, .. } => timestamp,
            IngestionEvent::GenerationCreate { timestamp, .. } => timestamp,
            IngestionEvent::GenerationUpdate { timestamp, .. } => timestamp,
            IngestionEvent::EventCreate { timestamp, .. } => timestamp,
            IngestionEvent::ScoreCreate { timestamp, .. } => timestamp,
            IngestionEvent::ObservationCreate { timestamp, .. } => timestamp,
            IngestionEvent::ObservationUpdate { timestamp, .. } => timestamp,
            IngestionEvent::SdkLog { timestamp, .. } => timestamp,
        }
    }
}

// ─── Conversion: IngestionEvent → OTLP Spans ───────────────────────

/// Convert a batch of IngestionEvents into an OTLP trace export request.
///
/// Mapping strategy:
/// - TraceCreate → root span with `langfuse.observation.type` = omitted (root is trace)
/// - SpanCreate → span with `langfuse.observation.type` = "span"
/// - GenerationCreate → span with `langfuse.observation.type` = "generation" + model/usage attrs
/// - ObservationCreate → span with `langfuse.observation.type` from body.type
/// - EventCreate → span with `langfuse.observation.type` = "event"
/// - ScoreCreate → span with `langfuse.observation.type` = omitted (attached to trace)
/// - Others → span with basic attributes
pub fn ingestion_events_to_otel(events: &[IngestionEvent]) -> OtelTraceExportRequest {
    let mut spans: Vec<OtelSpan> = Vec::with_capacity(events.len());

    for event in events {
        match event {
            IngestionEvent::TraceCreate { body, .. } => {
                let mut attrs = Vec::new();
                if let Some(ref session_id) = body.session_id {
                    attrs.push(OtelAttribute::string("langfuse.session.id", session_id));
                }
                if let Some(ref user_id) = body.user_id {
                    attrs.push(OtelAttribute::string("langfuse.user.id", user_id));
                }
                if let Some(ref release) = body.release {
                    attrs.push(OtelAttribute::string("langfuse.release", release));
                }
                if let Some(ref version) = body.version {
                    attrs.push(OtelAttribute::string("langfuse.version", version));
                }
                if let Some(ref env) = body.environment {
                    attrs.push(OtelAttribute::string("langfuse.environment", env));
                }
                if let Some(ref tags) = body.tags {
                    // Tags as comma-separated string
                    attrs.push(OtelAttribute::string("langfuse.trace.tags", tags.join(",")));
                }
                if let Some(ref input) = body.input {
                    attrs.push(OtelAttribute::string("langfuse.trace.input", input.to_string()));
                }
                if let Some(ref output) = body.output {
                    attrs.push(OtelAttribute::string("langfuse.trace.output", output.to_string()));
                }
                if let Some(ref name) = body.name {
                    attrs.push(OtelAttribute::string("langfuse.trace.name", name));
                }
                // trace.id becomes spanId for the root span; traceId is also set
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                spans.push(OtelSpan {
                    trace_id: Some(span_id.clone()),
                    span_id: Some(span_id),
                    parent_span_id: None,
                    name: body.name.clone().or_else(|| Some("trace".into())),
                    kind: Some(1), // INTERNAL
                    start_time_unix_nano: rfc3339_to_nano(event.event_timestamp()),
                    end_time_unix_nano: body.timestamp.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: Some(OtelStatus::default()),
                });
            }
            IngestionEvent::SpanCreate { body, .. } => {
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", "span"),
                ];
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());
                if let Some(ref session_id) = body.session_id {
                    attrs.push(OtelAttribute::string("langfuse.session.id", session_id));
                }
                if let Some(ref msg) = body.status_message {
                    attrs.push(OtelAttribute::string("langfuse.observation.status_message", msg));
                }

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: body.end_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::SpanUpdate { body, .. } => {
                // For updates, we still create a span — Langfuse OTel deduplicates by spanId
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", "span"),
                ];
                if let Some(ref session_id) = body.session_id {
                    attrs.push(OtelAttribute::string("langfuse.session.id", session_id));
                }
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: body.end_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::GenerationCreate { body, .. } => {
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", "generation"),
                ];
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());
                if let Some(ref model) = body.model {
                    attrs.push(OtelAttribute::string("langfuse.observation.model.name", model));
                }
                if let Some(ref params) = body.model_parameters {
                    if let Ok(json) = serde_json::to_string(params) {
                        attrs.push(OtelAttribute::string("langfuse.observation.model.parameters", json));
                    }
                }
                if let Some(ref usage) = body.usage {
                    if let Ok(json) = serde_json::to_string(usage) {
                        attrs.push(OtelAttribute::string("langfuse.observation.usage_details", json));
                    }
                }
                if let Some(ref usage_details) = body.usage_details {
                    for (k, v) in usage_details {
                        attrs.push(OtelAttribute::new(format!("gen_ai.usage.{}", k), OtelAttributeValue::int(*v as i64)));
                    }
                }
                if let Some(ref cost_details) = body.cost_details {
                    if let Ok(json) = serde_json::to_string(cost_details) {
                        attrs.push(OtelAttribute::string("langfuse.observation.cost_details", json));
                    }
                }
                if let Some(ref prompt_name) = body.prompt_name {
                    attrs.push(OtelAttribute::string("langfuse.observation.prompt.name", prompt_name));
                }
                if let Some(ref completion_start) = body.completion_start_time {
                    attrs.push(OtelAttribute::string("langfuse.observation.completion_start_time", completion_start));
                }

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: body.end_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::GenerationUpdate { body, .. } => {
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", "generation"),
                ];
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());
                if let Some(ref model) = body.model {
                    attrs.push(OtelAttribute::string("langfuse.observation.model.name", model));
                }
                if let Some(ref usage_details) = body.usage_details {
                    for (k, v) in usage_details {
                        attrs.push(OtelAttribute::new(format!("gen_ai.usage.{}", k), OtelAttributeValue::int(*v as i64)));
                    }
                }

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: body.end_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::EventCreate { body, .. } => {
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", "event"),
                ];
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: None, // Events don't have end_time
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::ObservationCreate { body, .. } => {
                let obs_type_str = serde_json::to_value(&body.r#type)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_lowercase()))
                    .unwrap_or_else(|| "span".to_string());
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", &obs_type_str),
                ];
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());
                if let Some(ref model) = body.model {
                    attrs.push(OtelAttribute::string("langfuse.observation.model.name", model));
                }
                if let Some(ref msg) = body.status_message {
                    attrs.push(OtelAttribute::string("langfuse.observation.status_message", msg));
                }

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: body.end_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::ObservationUpdate { body, .. } => {
                let obs_type_str = serde_json::to_value(&body.r#type)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_lowercase()))
                    .unwrap_or_else(|| "span".to_string());
                let mut attrs = vec![
                    OtelAttribute::string("langfuse.observation.type", &obs_type_str),
                ];
                append_common_obs_attrs(&mut attrs, body.input.as_ref(), body.output.as_ref(), body.metadata.as_ref(), body.version.as_ref(), body.environment.as_ref());

                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");
                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let parent_span_id = body.parent_observation_id.as_deref().map(|s| s.replace('-', ""));

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id,
                    name: body.name.clone(),
                    kind: Some(1),
                    start_time_unix_nano: body.start_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    end_time_unix_nano: body.end_time.as_ref().and_then(|t| rfc3339_to_nano(t)),
                    attributes: Some(attrs),
                    status: build_status(body.level.as_ref(), body.status_message.as_deref()),
                });
            }
            IngestionEvent::ScoreCreate { body, .. } => {
                // Scores are attached via attributes on the trace
                let mut attrs = vec![];
                attrs.push(OtelAttribute::string("langfuse.score.name", &body.name));
                attrs.push(OtelAttribute::new("langfuse.score.value", match &body.value {
                    serde_json::Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            OtelAttributeValue { string_value: None, int_value: None, double_value: Some(f), bool_value: None }
                        } else if let Some(i) = n.as_i64() {
                            OtelAttributeValue::int(i)
                        } else {
                            OtelAttributeValue::string(body.value.to_string())
                        }
                    }
                    serde_json::Value::Bool(b) => OtelAttributeValue::bool(*b),
                    _ => OtelAttributeValue::string(body.value.to_string()),
                }));
                if let Some(ref trace_id) = body.trace_id {
                    attrs.push(OtelAttribute::string("langfuse.trace.id", trace_id));
                }
                if let Some(ref obs_id) = body.observation_id {
                    attrs.push(OtelAttribute::string("langfuse.observation.id", obs_id));
                }

                let span_id = body.id.as_deref().unwrap_or("").replace('-', "");
                let trace_id = body.trace_id.as_deref().unwrap_or("").replace('-', "");

                spans.push(OtelSpan {
                    trace_id: Some(trace_id),
                    span_id: Some(span_id),
                    parent_span_id: body.observation_id.as_deref().map(|s| s.replace('-', "")),
                    name: Some(format!("score:{}", body.name)),
                    kind: Some(1),
                    start_time_unix_nano: None,
                    end_time_unix_nano: None,
                    attributes: Some(attrs),
                    status: Some(OtelStatus::default()),
                });
            }
            IngestionEvent::SdkLog { body, .. } => {
                // SDK logs are metadata; we skip them in OTLP as there's no natural mapping
                let attrs = vec![
                    OtelAttribute::string("langfuse.sdk.log", body.log.to_string()),
                ];
                spans.push(OtelSpan {
                    trace_id: None,
                    span_id: None,
                    parent_span_id: None,
                    name: Some("sdk-log".into()),
                    kind: Some(1),
                    start_time_unix_nano: None,
                    end_time_unix_nano: None,
                    attributes: Some(attrs),
                    status: Some(OtelStatus::default()),
                });
            }
        }
    }

    OtelTraceExportRequest {
        resource_spans: vec![OtelResourceSpan {
            resource: Some(OtelResource {
                attributes: Some(vec![
                    OtelAttribute::string("service.name", "perihelion-agent"),
                    OtelAttribute::string("service.version", env!("CARGO_PKG_VERSION")),
                ]),
            }),
            scope_spans: Some(vec![OtelScopeSpan {
                scope: Some(OtelScope {
                    name: Some("langfuse-client".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    attributes: None,
                }),
                spans: Some(spans),
            }]),
        }],
    }
}

/// Helper: append common observation-level attributes
fn append_common_obs_attrs(
    attrs: &mut Vec<OtelAttribute>,
    input: Option<&serde_json::Value>,
    output: Option<&serde_json::Value>,
    metadata: Option<&serde_json::Value>,
    version: Option<&String>,
    environment: Option<&String>,
) {
    if let Some(ref input) = input {
        attrs.push(OtelAttribute::string("langfuse.observation.input", input.to_string()));
    }
    if let Some(ref output) = output {
        attrs.push(OtelAttribute::string("langfuse.observation.output", output.to_string()));
    }
    if let Some(ref metadata) = metadata {
        if let Ok(json) = serde_json::to_string(metadata) {
            attrs.push(OtelAttribute::string("langfuse.observation.metadata", json));
        }
    }
    if let Some(v) = version {
        attrs.push(OtelAttribute::string("langfuse.version", v.as_str()));
    }
    if let Some(env) = environment {
        attrs.push(OtelAttribute::string("langfuse.environment", env.as_str()));
    }
}

/// Helper: build OTel status from Langfuse observation level + status message
fn build_status(level: Option<&ObservationLevel>, status_message: Option<&str>) -> Option<OtelStatus> {
    match level {
        Some(ObservationLevel::Error) => Some(OtelStatus {
            code: Some(2), // ERROR
            message: status_message.map(|s| s.to_string()),
        }),
        _ => Some(OtelStatus::default()),
    }
}

/// Convert RFC 3339 timestamp to Unix nanoseconds string
fn rfc3339_to_nano(rfc3339: &str) -> Option<String> {
    // Parse common RFC 3339 formats
    let ts = chrono::DateTime::parse_from_rfc3339(rfc3339).ok()?;
    Some(ts.timestamp_nanos_opt()?.to_string())
}

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
    pub session_id: Option<String>,
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
