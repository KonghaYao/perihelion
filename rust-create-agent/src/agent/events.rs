/// Agent 执行过程中的增量事件
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// LLM 输出最终文字（非流式，整段答案）
    TextChunk(String),
    /// 工具调用开始（工具名 + 参数）
    ToolStart { name: String, input: serde_json::Value },
    /// 工具调用结束（结果或错误）
    ToolEnd { name: String, output: String, is_error: bool },
    /// 一轮 ReAct 步骤完成
    StepDone { step: usize },
}

/// 事件回调 trait（应用层实现）
///
/// 在 `AgentExecutor` 执行过程中，关键节点会调用 `on_event`。
/// 实现者通过 `mpsc::Sender` 等机制将事件转发给 UI 层。
pub trait AgentEventHandler: Send + Sync {
    fn on_event(&self, event: AgentEvent);
}

/// 函数闭包适配器 —— 方便快速实现 `AgentEventHandler`
///
/// # 示例
/// ```rust,ignore
/// let tx = tx.clone();
/// let handler = FnEventHandler(move |event| {
///     let _ = tx.try_send(event);
/// });
/// executor.with_event_handler(Arc::new(handler))
/// ```
pub struct FnEventHandler<F>(pub F)
where
    F: Fn(AgentEvent) + Send + Sync;

impl<F> AgentEventHandler for FnEventHandler<F>
where
    F: Fn(AgentEvent) + Send + Sync,
{
    fn on_event(&self, event: AgentEvent) {
        (self.0)(event)
    }
}
