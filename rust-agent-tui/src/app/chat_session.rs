use rust_agent_middlewares::prelude::SkillMetadata;
use rust_agent_middlewares::prelude::TodoItem;

use super::AgentComm;
use super::AppCore;
use crate::command::CommandRegistry;
use super::langfuse_state::LangfuseState;
use crate::thread::ThreadId;

/// 独立聊天会话：封装一个对话的完整 UI 状态、Agent 通信状态和持久化上下文。
pub struct ChatSession {
    pub core: AppCore,
    pub agent: AgentComm,
    pub current_thread_id: Option<ThreadId>,
    pub langfuse: LangfuseState,
    pub todo_items: Vec<TodoItem>,
    /// 当前运行中的后台任务数量（状态栏指示器使用）
    pub background_task_count: usize,
    pub spinner_state: perihelion_widgets::SpinnerState,
}

impl ChatSession {
    pub fn new(
        cwd: String,
        command_registry: CommandRegistry,
        skills: Vec<SkillMetadata>,
    ) -> Self {
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);
        Self {
            core: AppCore::new(
                cwd,
                render_tx,
                render_cache,
                render_notify,
                command_registry,
                skills,
            ),
            agent: AgentComm::default(),
            current_thread_id: None,
            langfuse: LangfuseState::default(),
            todo_items: Vec::new(),
            background_task_count: 0,
            spinner_state: perihelion_widgets::SpinnerState::new(
                perihelion_widgets::SpinnerMode::Idle,
            ),
        }
    }
}
