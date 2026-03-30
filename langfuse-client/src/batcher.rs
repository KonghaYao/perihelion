use crate::config::{BackpressurePolicy, BatcherConfig};
use crate::error::LangfuseError;
use crate::types::IngestionEvent;
use crate::LangfuseClient;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

/// Batcher 内部命令（不导出）
enum BatcherCommand {
    /// 添加事件到待发送队列
    Add(IngestionEvent),
    /// 手动 flush：发送当前队列中的所有事件，完成后通过 oneshot 通知调用方
    Flush(oneshot::Sender<()>),
    /// 关闭后台 task（先 flush 剩余事件再退出）
    Shutdown,
}

/// Langfuse 事件批量聚合器
///
/// 通过后台 tokio task 异步收集事件，按 `max_events`（定量）或 `flush_interval`（定时）
/// 自动发送到 Langfuse API。支持手动 flush 和两种背压策略。
pub struct Batcher {
    tx: mpsc::Sender<BatcherCommand>,
    backpressure: BackpressurePolicy,
    /// 后台 task 的 JoinHandle，用于 Drop 时等待完成
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Batcher {
    /// 创建新的 Batcher 实例，同时启动后台事件处理 task
    pub fn new(client: LangfuseClient, config: BatcherConfig) -> Self {
        let client = Arc::new(client);
        let (tx, rx) = mpsc::channel(config.max_events);
        let backpressure = config.backpressure;

        let batch_client = Arc::clone(&client);
        let max_events = config.max_events;
        let flush_interval = config.flush_interval;

        let handle = tokio::spawn(async move {
            Self::run_loop(batch_client, rx, max_events, flush_interval).await;
        });

        Self {
            tx,
            backpressure,
            handle: Some(handle),
        }
    }

    /// 后台事件处理循环
    async fn run_loop(
        client: Arc<LangfuseClient>,
        mut rx: mpsc::Receiver<BatcherCommand>,
        max_events: usize,
        flush_interval: Duration,
    ) {
        let mut buffer: Vec<IngestionEvent> = Vec::with_capacity(max_events);
        let mut interval = interval(flush_interval);
        interval.tick().await;

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(BatcherCommand::Add(event)) => {
                            buffer.push(event);
                            if buffer.len() >= max_events {
                                Self::do_flush(&client, &mut buffer).await;
                            }
                        }
                        Some(BatcherCommand::Flush(ack)) => {
                            Self::do_flush(&client, &mut buffer).await;
                            let _ = ack.send(());
                        }
                        Some(BatcherCommand::Shutdown) | None => {
                            if !buffer.is_empty() {
                                info!(
                                    "Batcher shutting down, flushing {} remaining events",
                                    buffer.len()
                                );
                                Self::do_flush(&client, &mut buffer).await;
                            }
                            info!("Batcher background task exited");
                            return;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        debug!(
                            "Batcher periodic flush: {} events (interval: {:?})",
                            buffer.len(),
                            flush_interval
                        );
                        Self::do_flush(&client, &mut buffer).await;
                    }
                }
            }
        }
    }

    /// 执行一次 flush：将 buffer 中的事件发送到 Langfuse API
    async fn do_flush(client: &LangfuseClient, buffer: &mut Vec<IngestionEvent>) {
        if buffer.is_empty() {
            return;
        }

        let events: Vec<IngestionEvent> = buffer.drain(..).collect();
        debug!("Batcher flushing {} events", events.len());

        match client.ingest(events).await {
            Ok(response) => {
                if response.errors.is_empty() {
                    debug!(
                        "Batcher flush successful: {} events accepted",
                        response.successes.len()
                    );
                } else {
                    warn!(
                        "Batcher flush partial failure: {} succeeded, {} failed",
                        response.successes.len(),
                        response.errors.len()
                    );
                    for err in &response.errors {
                        warn!("  Failed event id={}, status={}, message={:?}",
                            err.id, err.status, err.message);
                    }
                }
            }
            Err(e) => {
                error!("Batcher flush failed: {}", e);
            }
        }
    }

    /// 添加事件到批量队列
    pub async fn add(&self, event: IngestionEvent) -> Result<(), LangfuseError> {
        let cmd = BatcherCommand::Add(event);
        match self.backpressure {
            BackpressurePolicy::DropNew => {
                self.tx.try_send(cmd).map_err(|e| match e {
                    mpsc::error::TrySendError::Full(_) => {
                        warn!("Batcher queue full, dropping event (DropNew policy)");
                        LangfuseError::ChannelClosed
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        warn!("Batcher channel closed, event dropped");
                        LangfuseError::ChannelClosed
                    }
                })
            }
            BackpressurePolicy::Block => {
                self.tx.send(cmd).await.map_err(|_| {
                    warn!("Batcher channel closed during send");
                    LangfuseError::ChannelClosed
                })
            }
        }
    }

    /// 手动触发 flush，等待所有待发送事件发送完毕
    pub async fn flush(&self) -> Result<(), LangfuseError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(BatcherCommand::Flush(tx))
            .await
            .map_err(|_| {
                warn!("Batcher channel closed, cannot flush");
                LangfuseError::ChannelClosed
            })?;
        rx.await.map_err(|_| {
            warn!("Batcher dropped flush acknowledgment");
            LangfuseError::ChannelClosed
        })
    }
}

impl Drop for Batcher {
    fn drop(&mut self) {
        let shutdown_cmd = BatcherCommand::Shutdown;
        if self.tx.try_send(shutdown_cmd).is_err() {
            debug!("Batcher Drop: channel already closed, background task may have exited");
        }

        if let Some(handle) = self.handle.take() {
            handle.abort();
            debug!("Batcher Drop: aborted background task");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TraceBody;
    use std::time::Duration;

    fn create_test_client(server_url: &str) -> LangfuseClient {
        LangfuseClient::new("pk", "sk", server_url, 0)
    }

    fn create_test_event(id: &str) -> IngestionEvent {
        IngestionEvent::TraceCreate {
            id: id.to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            body: TraceBody {
                id: Some(format!("trace-{}", id)),
                name: Some("test".into()),
                ..Default::default()
            },
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_batcher_add_and_manual_flush() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[{"id":"1","status":200}],"errors":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 10,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        batcher.add(create_test_event("1")).await.unwrap();
        batcher.add(create_test_event("2")).await.unwrap();
        batcher.add(create_test_event("3")).await.unwrap();
        batcher.flush().await.unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_batcher_auto_flush_on_max_events() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[{"id":"1","status":200}],"errors":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 3,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        batcher.add(create_test_event("1")).await.unwrap();
        batcher.add(create_test_event("2")).await.unwrap();
        batcher.add(create_test_event("3")).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        mock.assert_async().await;
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_periodic_flush() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[{"id":"1","status":200}],"errors":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 100,
            flush_interval: Duration::from_millis(100),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        batcher.add(create_test_event("1")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        mock.assert_async().await;
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_flush_empty_buffer() {
        let server = mockito::Server::new_async().await;
        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 10,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);
        let result = batcher.flush().await;
        assert!(result.is_ok());
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_backpressure_block() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[{"id":"1","status":200}],"errors":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 5,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::Block,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        for i in 0..5 {
            batcher.add(create_test_event(&format!("{}", i))).await.unwrap();
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        mock.assert_async().await;
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_graceful_shutdown_on_drop() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[],"errors":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 10,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        {
            let batcher = Batcher::new(client, config);
            batcher.add(create_test_event("1")).await.unwrap();
            batcher.add(create_test_event("2")).await.unwrap();
            batcher.flush().await.unwrap();
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_batcher_multiple_flush_cycles() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[{"id":"1","status":200}],"errors":[]}"#)
            .expect(2)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 2,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        batcher.add(create_test_event("1")).await.unwrap();
        batcher.add(create_test_event("2")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        batcher.add(create_test_event("3")).await.unwrap();
        batcher.add(create_test_event("4")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        mock.assert_async().await;
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_handles_ingest_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(500)
            .with_body("error")
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 2,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        batcher.add(create_test_event("1")).await.unwrap();
        batcher.add(create_test_event("2")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        batcher.add(create_test_event("3")).await.unwrap();
        mock.assert_async().await;
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_with_large_batch() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[],"errors":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 50,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        for i in 0..50 {
            batcher.add(create_test_event(&format!("{}", i))).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(200)).await;

        mock.assert_async().await;
        drop(batcher);
    }

    #[tokio::test]
    async fn test_batcher_backpressure_drop_new() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server.mock("POST", "/api/public/ingestion")
            .with_status(207)
            .with_header("content-type", "application/json")
            .with_body(r#"{"successes":[],"errors":[]}"#)
            .create_async()
            .await;

        let client = create_test_client(&server.url());
        let config = BatcherConfig {
            max_events: 2,
            flush_interval: Duration::from_secs(60),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Batcher::new(client, config);

        batcher.add(create_test_event("1")).await.unwrap();
        batcher.add(create_test_event("2")).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        batcher.add(create_test_event("3")).await.unwrap();
        drop(batcher);
    }
}
