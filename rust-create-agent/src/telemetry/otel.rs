//! OpenTelemetry OTLP 初始化（需启用 `otel` feature）

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::trace::{BatchSpanProcessor, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct OtelGuard {
    provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        eprintln!("[otel] shutting down provider...");
        match self.provider.shutdown() {
            Ok(_) => eprintln!("[otel] shutdown ok"),
            Err(e) => eprintln!("[otel] shutdown error: {e}"),
        }
    }
}

pub fn init_otel(
    service_name: &str,
    endpoint: &str,
) -> Result<OtelGuard, Box<dyn std::error::Error + Send + Sync>> {
    // OTLP HTTP 需要完整路径 /v1/traces，若用户只传了 base URL 则自动补全
    let traces_endpoint = if endpoint.ends_with("/v1/traces") {
        endpoint.to_string()
    } else {
        format!("{}/v1/traces", endpoint.trim_end_matches('/'))
    };

    eprintln!("[otel] connecting to {traces_endpoint}");

    let exporter = SpanExporter::builder()
        .with_http()
        .with_http_client(reqwest::blocking::Client::new())
        .with_endpoint(&traces_endpoint)
        .build()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    let resource = Resource::builder()
        .with_service_name(service_name.to_string())
        .build();

    // BatchSpanProcessor：在独立 OS 线程里批量发送，不阻塞 tokio worker
    let processor = BatchSpanProcessor::builder(exporter).build();

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_span_processor(processor)
        .build();

    let tracer = provider.tracer(service_name.to_string());

    let log_file = std::env::var("RUST_LOG_FILE").ok();

    // 有 RUST_LOG_FILE 才输出文字日志，否则只走 OTLP，不碰 stdout
    match log_file {
        Some(path) => {
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("cannot open log file");
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_ansi(false).with_writer(file))
                .with(OpenTelemetryLayer::new(tracer))
                .init();
        }
        None => {
            // 只接 OTel layer，不加任何文字输出 layer
            tracing_subscriber::registry()
                .with(OpenTelemetryLayer::new(tracer))
                .init();
        }
    }

    Ok(OtelGuard { provider })
}
