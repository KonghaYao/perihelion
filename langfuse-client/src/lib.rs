pub mod batcher;
pub mod client;
pub mod config;
pub mod error;
pub mod types;

// 重导出常用类型
pub use batcher::Batcher;
pub use client::LangfuseClient;
pub use config::{BackpressurePolicy, BatcherConfig, ClientConfig};
pub use error::LangfuseError;
pub use types::{
    ingestion_events_to_otel,
    CostDetails,
    EventBody,
    GenerationBody,
    IngestionEvent,
    IngestionUsage,
    ObservationBody,
    ObservationLevel,
    ObservationType,
    OtelAttribute,
    OtelAttributeValue,
    OtelResource,
    OtelResourceSpan,
    OtelScope,
    OtelScopeSpan,
    OtelSpan,
    OtelStatus,
    // OTLP types
    OtelTraceExportRequest,
    OtelTraceResponse,
    ScoreBody,
    ScoreDataType,
    SdkLogBody,
    SpanBody,
    TraceBody,
    Usage,
    UsageDetails,
};
