use std::sync::Arc;
use std::time::Duration;

use langfuse_client::{BackpressurePolicy, Batcher, BatcherConfig, LangfuseClient};

use super::config::LangfuseConfig;

/// Langfuse Thread 级别会话，持有跨多轮复用的共享连接状态。
///
/// 生命周期：Thread 创建/打开时构造，new_thread()/open_thread() 时重置（= None）。
/// 同一 Thread 内所有 `LangfuseTracer` 共享同一个 client + batcher + session_id。
pub struct LangfuseSession {
    pub client: Arc<LangfuseClient>,
    pub batcher: Arc<Batcher>,
    /// session_id = thread_id，Thread 内所有 Trace 共享
    pub session_id: String,
}

impl LangfuseSession {
    /// 从配置和 session_id 构造 Session，失败时返回 None（静默降级）
    pub async fn new(config: LangfuseConfig, session_id: String) -> Option<Self> {
        let client = Arc::new(LangfuseClient::new(
            &config.public_key,
            &config.secret_key,
            &config.host,
            3, // max_retries
        ));

        let batcher_config = BatcherConfig {
            max_events: 50,
            flush_interval: Duration::from_secs(10),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 3,
        };
        let batcher = Batcher::new((*client).clone(), batcher_config);

        Some(Self {
            client,
            batcher: Arc::new(batcher),
            session_id,
        })
    }
}
