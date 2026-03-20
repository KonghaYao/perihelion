//! OpenTelemetry 集成模块
//!
//! ## 控制开关
//!
//! 通过环境变量控制，**不配置就不开启**：
//!
//! | 环境变量 | 说明 |
//! |---|---|
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | 设置后自动启用 OTLP 导出 |
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
//!     // 设置后 → 同时导出到 OTLP 后端
//!     let _guard = rust_create_agent::telemetry::init_tracing("my-agent");
//! }
//! ```

mod otel;
mod subscriber;

pub use otel::OtelGuard;

/// tracing 初始化守卫，持有期间保持 tracing 配置有效
pub enum TracingGuard {
    /// 仅 stdout 输出
    Simple(subscriber::TracingGuard),
    /// OTLP 导出（配置了 endpoint）
    Otel(otel::OtelGuard),
}

/// 初始化 tracing，**自动检测环境变量决定是否启用 OTLP**
///
/// - 未设置 `OTEL_EXPORTER_OTLP_ENDPOINT`：只输出到 stdout，无任何 OTel 开销
/// - 设置了该变量：同时导出 trace 到 OTLP 后端
///
/// 返回的 `TracingGuard` 必须保持存活直到程序退出（通常绑定到 `main` 的局部变量）。
pub fn init_tracing(service_name: &str) -> TracingGuard {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

    match endpoint {
        Some(ep) => match otel::init_otel(service_name, &ep) {
            Ok(guard) => TracingGuard::Otel(guard),
            Err(e) => {
                eprintln!("[otel] init failed (endpoint={ep}): {e}");
                TracingGuard::Simple(subscriber::init_tracing(service_name))
            }
        },
        None => TracingGuard::Simple(subscriber::init_tracing(service_name)),
    }
}
