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
    /// SQLite child thread ID（uuid7），用于 TUI 聚焦时 load_messages
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_thread_id: Option<String>,
}

impl BackgroundTaskResult {
    /// 格式化为注入到 LLM 消息流的通知文本
    pub fn to_notification(&self) -> String {
        let short_id = &self.task_id[..8.min(self.task_id.len())];
        if self.success {
            format!(
                "[后台任务 {} 已完成] Agent: {} | 工具调用: {} | 耗时: {}ms\n结果:\n{}",
                short_id, self.agent_name, self.tool_calls_count, self.duration_ms, self.output,
            )
        } else {
            format!(
                "[后台任务 {} 执行失败] Agent: {}\n错误:\n{}",
                short_id, self.agent_name, self.output,
            )
        }
    }
}

/// Compact 保留的文件信息摘要
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactFileInfo {
    pub path: String,
    pub lines: usize,
}

/// Todo 列表条目（用于 ExecutorEvent::TodoUpdate）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TodoEntry {
    pub content: String,
    #[serde(
        default,
        rename = "activeForm",
        skip_serializing_if = "Option::is_none"
    )]
    pub active_form: Option<String>,
    pub status: TodoStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// Agent 执行过程中的增量事件
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
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
    /// 状态快照（含完整的消息历史），用于持久化和断点续跑
    StateSnapshot(Vec<crate::messages::BaseMessage>),
    /// 增量消息（BaseMessage），持久化和遥测的最小数据单元
    MessageAdded(crate::messages::BaseMessage),
    /// LLM 调用开始（携带完整 input messages 快照 + 工具定义，用于 Langfuse Generation）
    LlmCallStart {
        step: usize,
        /// Arc 共享引用——Clone AgentEvent 时为浅拷贝（引用计数 +1），不产生独立副本
        messages: std::sync::Arc<Vec<crate::messages::BaseMessage>>,
        tools: Vec<crate::tools::ToolDefinition>,
    },
    /// LLM 调用结束（携带模型名、输出文本、token 使用量）
    LlmCallEnd {
        step: usize,
        model: String,
        output: String,
        usage: Option<crate::llm::types::TokenUsage>,
        /// LLM 响应停止原因（None 表示 LLM 调用失败/异常）
        stop_reason: Option<crate::llm::types::StopReason>,
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
    SubagentStarted {
        agent_name: String,
        /// 唯一实例标识符（用于并发同类型 SubAgent 路由）
        instance_id: String,
        /// 是否为后台模式（run_in_background）
        is_background: bool,
    },
    /// 子 agent 执行完成
    SubagentStopped {
        agent_name: String,
        result: String,
        is_error: bool,
        /// 唯一实例标识符
        instance_id: String,
    },
    /// 上下文压缩开始
    CompactStarted,
    /// 上下文压缩完成
    CompactCompleted {
        /// 摘要文本（full compact 时非空，micro compact 时为空）
        summary: String,
        /// 保留的文件摘要列表
        files: Vec<CompactFileInfo>,
        /// 保留的 Skill 名称列表
        skills: Vec<String>,
        /// micro-compact 清除的工具结果数量（>0 表示 micro-compact）
        micro_cleared: usize,
        /// 压缩后的新消息列表（full compact 时非空）
        messages: Vec<crate::messages::BaseMessage>,
    },
    /// 对话回退完成（rewind 命令，移除目标用户消息及其之后的所有消息）
    RewindCompleted {
        /// 摘要文本（如"已回滚 N 条消息"）
        summary: String,
        /// 回退后的新消息列表（目标消息之前，不含目标本身）
        messages: Vec<crate::messages::BaseMessage>,
    },
    /// 上下文压缩失败
    CompactError { message: String },
    /// Todo 列表更新
    TodoUpdate(Vec<TodoEntry>),
    /// LSP 诊断更新
    LspDiagnostics {
        errors: usize,
        warnings: usize,
        files_with_errors: usize,
    },
    /// Agent 执行失败（由 executor 在 agent.execute() 返回 Err 时发送）
    AgentExecutionFailed { message: String },
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
