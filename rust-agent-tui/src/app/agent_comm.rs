use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::BaseMessage;
use tokio::sync::mpsc;

use super::events::AgentEvent;
use super::InteractionPrompt;

type SharedToolRegistry = std::sync::Arc<
    parking_lot::RwLock<
        std::collections::HashMap<String, std::sync::Arc<dyn rust_create_agent::tools::BaseTool>>,
    >,
>;

/// LLM 重试状态（由 AgentEvent::LlmRetrying 更新）
pub struct RetryStatus {
    pub attempt: usize,
    pub max_attempts: usize,
    pub delay_ms: u64,
    /// 最近一次重试的错误描述（供状态栏展示）
    pub error: String,
}

/// Agent 通信状态：事件接收、交互弹窗、取消/计时
pub struct AgentComm {
    pub agent_rx: Option<mpsc::Receiver<AgentEvent>>,
    /// 当前激活的交互弹窗（HITL 审批或 AskUser 问答，同一时刻只有一种）
    pub interaction_prompt: Option<InteractionPrompt>,
    /// 已发送待解决的 HITL 工具名称列表（用于 approval_resolved 广播）
    pub pending_hitl_items: Option<Vec<String>>,
    /// AskUser 是否已提交（用于广播 resolved）
    pub pending_ask_user: Option<bool>,
    /// 持久化的 Agent 消息历史（多轮对话的上下文）
    pub agent_state_messages: Vec<BaseMessage>,
    /// 当前 Agent 的 ID（用于 AgentDefineMiddleware 加载 agent 定义）
    pub agent_id: Option<String>,
    /// 当前 Agent 任务的取消令牌（loading 时有效，Ctrl+C 触发）
    pub cancel_token: Option<AgentCancellationToken>,
    /// 当前 Agent 任务开始时间（用于计算运行时长）
    pub task_start_time: Option<std::time::Instant>,
    /// 上一次任务的总运行时长（任务结束后保留显示）
    pub last_task_duration: Option<std::time::Duration>,
    /// 测试用事件注入队列（仅测试时使用，生产时保持为空）
    pub agent_event_queue: Vec<AgentEvent>,
    /// 会话级 token 累积追踪（从 AgentEvent::TokenUsageUpdate 聚合）
    pub session_token_tracker: rust_create_agent::agent::token::TokenTracker,
    /// 当前模型的上下文窗口大小（从最近一次 TokenUsageUpdate 中的 model 推断）
    pub context_window: u32,
    /// 是否需要 auto-compact（在 LlmCallEnd 时标记，Done 时执行）
    pub needs_auto_compact: bool,
    /// 连续 auto-compact 失败次数（circuit breaker，达到 3 次后停止自动触发）
    pub auto_compact_failures: u32,
    /// compact 前的 token tracker 快照（compact 失败时恢复，防止 tracker 失去对上下文大小的感知）
    pub pre_compact_token_snapshot: Option<rust_create_agent::agent::token::TokenTracker>,
    /// LLM 重试状态（重试中时为 Some，收到下一个正常事件时清除）
    pub retry_status: Option<RetryStatus>,
    /// SubAgent 执行深度计数器（>0 表示当前在 SubAgent 内，忽略其 TokenUsageUpdate）
    pub subagent_depth: u32,
    /// 会话开始时间（首次 submit_message 时记录）
    pub session_start_time: Option<std::time::Instant>,
    /// 会话级工具调用次数（统计 ToolStart 事件数）
    pub tool_call_count: u32,
    /// 后台任务全部完成后的待提交 continuation 消息
    ///（延迟到下一帧提交，避免在 handle_agent_event 内部修改 agent_rx）
    pub pending_bg_continuation: Option<String>,
    /// Agent 已完成（Done/Error）但仍有后台任务在运行，
    /// 此时 agent_rx 保持存活以接收 BackgroundTaskCompleted 事件
    pub agent_done_pending_bg: bool,
    /// Agent 尚未 Done 但后台任务已完成的通知缓存。
    /// 修复 BackgroundTaskCompleted 与 Done 事件的竞态条件：
    /// 当 BackgroundTaskCompleted 在 Done 之前被消费时，将显示通知暂存于此，
    /// 待 Done 处理时检查此字段并设置 pending_bg_continuation。
    pub pre_done_bg_completions: Vec<String>,
    /// 本轮 agent 是否已产生回复（收到 TextChunk/ToolStart/AssistantChunk），
    /// 用于 Ctrl+C 中断时判断是否恢复用户文本
    pub agent_replied: bool,
    /// 标记 Interrupted/Error 处理器已完成 reconcile，Done 到达时应跳过重复 reconcile
    /// （防止 Done 的 RebuildAll 覆盖 Interrupted/Error 添加的通知消息）
    pub reconcile_already_done: bool,
    /// 本轮用户原始输入（compact 后自动 re-submit 用）
    pub last_user_input: Option<String>,
    /// compact 启动时保存的用户输入副本（防止 compact 过程中 last_user_input 被覆盖）
    pub pre_compact_user_input: Option<String>,
    /// 连续 auto-compact re-submit 次数（防止无限循环，上限 3 次）
    pub auto_compact_resubmit_count: u32,
    /// compact 完成后是否应自动 resubmit（仅 agent 执行中 auto-compact 为 true，
    /// 手动 /compact 和 Done 后 auto-compact 为 false）
    pub compact_should_resubmit: bool,
    /// LSP 诊断计数（由 LspDiagnostics 事件更新）
    pub lsp_errors: usize,
    pub lsp_warnings: usize,
    pub lsp_files_with_errors: usize,
    /// 会话级 ToolSearch 索引（跨 submit 复用，缓存 deferred tools 提示词）
    pub tool_search_index:
        Option<std::sync::Arc<rust_agent_middlewares::tool_search::ToolSearchIndex>>,
    /// 会话级共享工具注册表（跨 submit 复用）
    pub shared_tools: Option<SharedToolRegistry>,
}

impl Default for AgentComm {
    fn default() -> Self {
        Self {
            agent_rx: None,
            interaction_prompt: None,
            pending_hitl_items: None,
            pending_ask_user: None,
            agent_state_messages: Vec::new(),
            agent_id: None,
            cancel_token: None,
            task_start_time: None,
            last_task_duration: None,
            agent_event_queue: Vec::new(),
            session_token_tracker: rust_create_agent::agent::token::TokenTracker::default(),
            context_window: 200_000,
            needs_auto_compact: false,
            auto_compact_failures: 0,
            pre_compact_token_snapshot: None,
            retry_status: None,
            subagent_depth: 0,
            session_start_time: None,
            tool_call_count: 0,
            pending_bg_continuation: None,
            agent_done_pending_bg: false,
            pre_done_bg_completions: Vec::new(),
            agent_replied: false,
            reconcile_already_done: false,
            last_user_input: None,
            pre_compact_user_input: None,
            auto_compact_resubmit_count: 0,
            compact_should_resubmit: false,
            lsp_errors: 0,
            lsp_warnings: 0,
            lsp_files_with_errors: 0,
            tool_search_index: None,
            shared_tools: None,
        }
    }
}
