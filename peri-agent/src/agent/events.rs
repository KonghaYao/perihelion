/// 后台任务完成通知（注入到主 agent 消息流中）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackgroundTaskResult {
    pub task_id: String,
    pub agent_name: String,
    pub prompt_summary: String,
    pub success: bool,
    pub output: String,
    pub tool_calls_count: usize,
    pub duration_ms: u64,
}

/// Agent 执行过程中的增量事件
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// AI 推理内容（reasoning/思考过程）
    AiReasoning(String),
    /// LLM 输出最终文字（非流式，整段答案），携带所属 AI 消息的 message_id
    TextChunk {
        message_id: crate::messages::MessageId,
        chunk: String,
        source_agent_id: Option<String>,
    },
    /// 工具调用开始（工具名 + 参数），携带所属 AI 消息的 message_id
    ToolStart {
        message_id: crate::messages::MessageId,
        tool_call_id: String,
        name: String,
        input: serde_json::Value,
        source_agent_id: Option<String>,
    },
    /// 工具调用结束（结果或错误），携带所属 AI 消息的 message_id
    ToolEnd {
        message_id: crate::messages::MessageId,
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
        source_agent_id: Option<String>,
    },
    /// 一轮 ReAct 步骤完成
    StepDone { step: usize },
    /// 状态快照（含完整的消息历史），用于持久化和断点续跑
    StateSnapshot(Vec<crate::messages::BaseMessage>),
    /// 增量消息（BaseMessage），持久化和遥测的最小数据单元
    MessageAdded(crate::messages::BaseMessage),
    /// LLM 调用开始（携带完整 input messages 快照 + 工具定义，用于 Langfuse Generation）
    LlmCallStart {
        step: usize,
        messages: Vec<crate::messages::BaseMessage>,
        tools: Vec<crate::tools::ToolDefinition>,
    },
    /// LLM 调用结束（携带模型名、输出文本、token 使用量）
    LlmCallEnd {
        step: usize,
        model: String,
        output: String,
        usage: Option<crate::llm::types::TokenUsage>,
    },
    /// 上下文窗口使用警告（阈值触发时发出）
    ContextWarning {
        used_tokens: u64,
        total_tokens: u64,
        percentage: f64,
    },
    /// LLM 调用重试中
    LlmRetrying {
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
        error: String,
    },
    /// 后台 agent 任务完成（TUI 使用，用于空闲时通知）
    BackgroundTaskCompleted(BackgroundTaskResult),
    /// 子 agent 开始执行
    SubagentStarted { agent_name: String },
    /// 子 agent 执行完成
    SubagentStopped {
        agent_name: String,
        result: String,
        is_error: bool,
    },
    /// Session 结束
    SessionEnded,
    /// 上下文压缩开始
    CompactStarted,
    /// 上下文压缩完成
    CompactCompleted,
    /// LSP 诊断更新
    LspDiagnostics {
        errors: usize,
        warnings: usize,
        files_with_errors: usize,
    },
}

/// 事件回调 trait（应用层实现）
///
/// 在 `ReActAgent` 执行过程中，关键节点会调用 `on_event`。
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

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;
