use rust_agent_middlewares::ask_user::AskUserBatchRequest;
use rust_agent_middlewares::prelude::TodoItem;

use super::BatchApprovalRequest;

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
    /// 新消息添加到状态（包括最终 AI 回答）
    MessageAdded(rust_create_agent::messages::BaseMessage),
    Done,
    Error(String),
    /// 用户中断（Ctrl+C），工具已以 error 结尾，消息已持久化
    Interrupted,
    /// HITL 批量审批请求
    ApprovalNeeded(BatchApprovalRequest),
    /// AskUser 批量提问请求
    AskUserBatch(AskUserBatchRequest),
    /// Todo 列表更新
    TodoUpdate(Vec<TodoItem>),
    /// Agent 执行结束后的消息快照（用于多轮对话续接）
    StateSnapshot(Vec<rust_create_agent::messages::BaseMessage>),
    /// 上下文压缩成功，携带摘要文本
    CompactDone(String),
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
}
