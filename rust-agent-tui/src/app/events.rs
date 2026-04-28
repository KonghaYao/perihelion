use rust_agent_middlewares::prelude::TodoItem;
use rust_create_agent::interaction::{InteractionContext, InteractionResponse};
use tokio::sync::oneshot;

/// TUI 与后台 Agent 任务之间的通信事件（通过 mpsc channel 传递）
pub enum AgentEvent {
    ToolCall {
        tool_call_id: String,
        name: String,
        display: String,
        args: Option<String>,
        is_error: bool,
    },
    AssistantChunk(String),
    Done,
    Error(String),
    /// 用户中断（Ctrl+C），工具已以 error 结尾，消息已持久化
    Interrupted,
    /// 统一人机交互请求（HITL 审批 / AskUser 问答）
    InteractionRequest {
        ctx: InteractionContext,
        response_tx: oneshot::Sender<InteractionResponse>,
    },
    /// Todo 列表更新
    TodoUpdate(Vec<TodoItem>),
    /// Agent 执行结束后的消息快照（用于多轮对话续接）
    StateSnapshot(Vec<rust_create_agent::messages::BaseMessage>),
    /// 上下文压缩成功，携带摘要文本和新 Thread ID
    CompactDone {
        summary: String,
        new_thread_id: String,
    },
    /// 上下文压缩失败，携带错误信息
    CompactError(String),
    /// SubAgent 开始执行（由 launch_agent ToolStart 映射而来）
    SubAgentStart {
        agent_id: String,
        task_preview: String,
    },
    /// SubAgent 执行结束（由 launch_agent ToolEnd 映射而来）
    SubAgentEnd {
        result: String,
        is_error: bool,
    },
    /// Token 使用量更新（从核心层 LlmCallEnd 映射而来）
    TokenUsageUpdate {
        usage: rust_create_agent::llm::types::TokenUsage,
        model: String,
    },
    /// LLM 调用重试中（从核心层 LlmRetrying 映射而来）
    LlmRetrying {
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
        error: String,
    },
}
