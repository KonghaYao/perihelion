//! Tracing subscriber 初始化（基础日志输出）

use tracing_subscriber::{fmt, EnvFilter, Registry, prelude::*};

pub struct TracingGuard;

impl Drop for TracingGuard {
    fn drop(&mut self) {
        // 无需特殊清理逻辑
    }
}

/// 初始化 tracing，输出到 stderr（避免干扰 TUI）
pub fn init_tracing(service_name: &str) -> TracingGuard {
    // 根据 RUST_LOG_FORMAT 环境变量决定输出格式
    let is_json = std::env::var("RUST_LOG_FORMAT").as_deref() == Ok("json");

    // 检查是否配置了日志文件
    let log_file = std::env::var("RUST_LOG_FILE").ok();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    match log_file {
        Some(path) => {
            // 输出到日志文件
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("cannot open log file");

            if is_json {
                let subscriber = Registry::default()
                    .with(filter)
                    .with(fmt::layer().json().with_writer(file));
                tracing::subscriber::set_global_default(subscriber)
                    .expect("Unable to set global subscriber");
            } else {
                let subscriber = Registry::default()
                    .with(filter)
                    .with(fmt::layer().with_writer(file).with_ansi(false));
                tracing::subscriber::set_global_default(subscriber)
                    .expect("Unable to set global subscriber");
            }
        }
        None => {
            // 输出到 stderr（避免干扰 TUI）
            if is_json {
                let subscriber = Registry::default()
                    .with(filter)
                    .with(fmt::layer().json().with_writer(std::io::stderr));
                tracing::subscriber::set_global_default(subscriber)
                    .expect("Unable to set global subscriber");
            } else {
                let subscriber = Registry::default()
                    .with(filter)
                    .with(fmt::layer().with_writer(std::io::stderr).with_ansi(false));
                tracing::subscriber::set_global_default(subscriber)
                    .expect("Unable to set global subscriber");
            }
        }
    }

    TracingGuard
}
