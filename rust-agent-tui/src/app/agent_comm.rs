#[allow(unused)]
use rust_create_agent::agent::AgentCancellationToken;
#[allow(unused)]
use rust_create_agent::messages::BaseMessage;
#[allow(unused)]
use tokio::sync::mpsc;

#[allow(unused)]
use super::events::AgentEvent;
#[allow(unused)]
use super::InteractionPrompt;

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
        }
    }
}
