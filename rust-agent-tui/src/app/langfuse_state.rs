use std::sync::Arc;

/// Langfuse 可观测性状态：Session/Tracer/Flush
pub struct LangfuseState {
    /// Thread 级别的 Langfuse Session（Thread 创建/打开时懒加载，new_thread/open_thread 时重置）
    pub langfuse_session: Option<Arc<crate::langfuse::LangfuseSession>>,
    /// 当前轮次的 Langfuse Tracer（submit_message 时创建，Done 时结束，未配置时为 None）
    pub langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
    /// on_trace_end 返回的 flush JoinHandle，进程退出前应 await 确保 batcher flush 完成
    pub langfuse_flush_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Default for LangfuseState {
    fn default() -> Self {
        Self {
            langfuse_session: None,
            langfuse_tracer: None,
            langfuse_flush_handle: None,
        }
    }
}
