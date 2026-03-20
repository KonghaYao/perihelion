//! OpenTelemetry 集成模块
//!
//! ## 控制开关
//!
//! 通过环境变量控制，**不配置就不开启**：
//!
//! | 环境变量 | 说明 |
//! |---|---|
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | 设置后自动启用 OTLP 导出（需 `otel` feature） |
//! | `RUST_LOG` | 日志级别，默认 `info` |
//! | `RUST_LOG_FORMAT=json` | 使用 JSON 格式输出 |
//!
//! ## 使用方式
//!
//! 调用一次 [`init_tracing`]，其余自动处理：
//!
//! ```rust,no_run
//! #[tokio::main]
//! async fn main() {
//!     // 未设置 OTEL_EXPORTER_OTLP_ENDPOINT → 只输出到 stdout
//!     // 设置后（且启用 otel feature）→ 同时导出到 OTLP 后端
//!     let _guard = rust_create_agent::telemetry::init_tracing("my-agent").await;
//! }
//! ```

mod subscriber;

#[cfg(feature = "otel")]
mod otel;

#[cfg(feature = "otel")]
pub use otel::OtelGuard;

/// tracing 初始化守卫，持有期间保持 tracing 配置有效
pub enum TracingGuard {
    /// 仅 stdout 输出
    Simple(subscriber::TracingGuard),
    /// OTLP 导出（otel feature 启用且配置了 endpoint）
    #[cfg(feature = "otel")]
    Otel(otel::OtelGuard),
}

/// 初始化 tracing，**自动检测环境变量决定是否启用 OTLP**
///
/// - 未设置 `OTEL_EXPORTER_OTLP_ENDPOINT`：只输出到 stdout，无任何 OTel 开销
/// - 设置了该变量且启用 `otel` feature：同时导出 trace 到 OTLP 后端
/// - 设置了该变量但未启用 `otel` feature：打印警告，降级为 stdout 模式
///
/// 返回的 `TracingGuard` 必须保持存活直到程序退出（通常绑定到 `main` 的局部变量）。
pub fn init_tracing(service_name: &str) -> TracingGuard {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

    match endpoint {
        #[cfg(feature = "otel")]
        Some(ep) => {
            match otel::init_otel(service_name, &ep) {
                Ok(guard) => TracingGuard::Otel(guard),
                Err(e) => {
                    // OTLP 初始化失败时降级，用 eprintln 打印错误（不依赖 tracing subscriber）
                    eprintln!("[otel] init failed (endpoint={ep}): {e}");
                    TracingGuard::Simple(subscriber::init_tracing(service_name))
                }
            }
        }
        #[cfg(not(feature = "otel"))]
        Some(ep) => {
            eprintln!(
                "[otel] OTEL_EXPORTER_OTLP_ENDPOINT={ep} is set but `otel` feature is not enabled; \
                 recompile with --features otel"
            );
            TracingGuard::Simple(subscriber::init_tracing(service_name))
        }
        None => TracingGuard::Simple(subscriber::init_tracing(service_name)),
    }
}
