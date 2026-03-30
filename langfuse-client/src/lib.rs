pub mod config;
pub mod error;
pub mod types;
pub mod client;
pub mod batcher;

// 重导出常用类型
pub use error::LangfuseError;
pub use config::{ClientConfig, BatcherConfig, BackpressurePolicy};
pub use client::LangfuseClient;
pub use batcher::Batcher;
pub use types::{
    IngestionEvent, IngestionResponse, IngestionSuccess, IngestionError,
    TraceBody, SpanBody, GenerationBody, EventBody, ObservationBody,
    ScoreBody, SdkLogBody,
    ObservationType, ObservationLevel, ScoreDataType,
    Usage, UsageDetails, CostDetails, IngestionUsage,
};
