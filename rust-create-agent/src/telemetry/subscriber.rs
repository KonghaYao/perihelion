//! 标准 tracing-subscriber 初始化（无 OTLP 导出）

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化 tracing
///
/// - `RUST_LOG_FILE=<路径>`：日志写到文件（TUI 模式必须用，否则破坏画面）
/// - 未设置 `RUST_LOG_FILE`：不添加任何文字输出 layer，完全静默
/// - `RUST_LOG`：日志级别，仅在写文件时生效，默认 `info`
/// - `RUST_LOG_FORMAT=json`：JSON 格式（默认纯文本）
pub fn init_tracing(_service_name: &str) -> TracingGuard {
    let log_file = std::env::var("RUST_LOG_FILE").ok();

    match log_file {
        Some(path) => {
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));
            let use_json = std::env::var("RUST_LOG_FORMAT")
                .map(|v| v.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("cannot open log file");

            if use_json {
                tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().json().with_writer(file))
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().with_ansi(false).with_writer(file))
                    .init();
            }
        }
        None => {
            // 没有日志文件目标，不添加任何 fmt layer，彻底静默
            tracing_subscriber::registry().init();
        }
    }

    TracingGuard
}

pub struct TracingGuard;
