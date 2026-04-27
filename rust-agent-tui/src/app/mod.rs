pub mod agent;
pub mod agent_panel;
pub mod events;
pub mod interaction_broker;
pub mod model_panel;
mod provider;
pub mod setup_wizard;
pub mod tool_display;

mod core;
mod cron_state;
mod cron_ops;
mod agent_comm;
mod agent_ops;
mod langfuse_state;
mod ask_user_ops;
mod ask_user_prompt;
mod hint_ops;
mod hitl_ops;
mod hitl_prompt;
mod panel_ops;
mod thread_ops;

pub use ask_user_prompt::AskUserBatchPrompt;
pub use events::AgentEvent;
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
pub use interaction_broker::TuiInteractionBroker;

/// 统一交互弹窗枚举：同一时刻只允许一种弹窗激活
pub enum InteractionPrompt {
    Approval(HitlBatchPrompt),
    Questions(AskUserBatchPrompt),
}

use crate::ui::theme;
use ratatui::style::Style;
use ratatui_textarea::TextArea;
use rust_agent_middlewares::prelude::{HitlDecision, TodoItem};
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use tokio::sync::mpsc;

use crate::config::ZenConfig;
use crate::thread::{SqliteThreadStore, ThreadBrowser, ThreadId, ThreadMeta, ThreadStore};

// Re-export MessageViewModel from ui::message_view
use crate::command::agents::AgentItem;
pub use crate::ui::message_view::{ContentBlockView, MessageViewModel};
pub use agent_panel::AgentPanel;
pub use model_panel::ModelPanel;
pub use setup_wizard::SetupWizardPanel;
use std::sync::Arc;
use tracing::Instrument;

use crate::ui::render_thread::RenderEvent;

// Re-export sub-structs
pub use agent_comm::AgentComm;
pub use core::AppCore;
pub use cron_state::{CronPanel, CronState};
pub use langfuse_state::LangfuseState;

// ─── App ──────────────────────────────────────────────────────────────────────

pub struct App {
    pub core: AppCore,
    pub agent: AgentComm,
    pub langfuse: LangfuseState,
    // 不变字段（跨子结构体的"胶水"字段）
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    pub zen_config: Option<ZenConfig>,
    pub thread_store: Arc<dyn ThreadStore>,
    pub current_thread_id: Option<ThreadId>,
    pub todo_items: Vec<TodoItem>,
    pub cron: CronState,
    pub setup_wizard: Option<SetupWizardPanel>,
    pub permission_mode: Arc<rust_agent_middlewares::prelude::SharedPermissionMode>,
    /// 权限模式切换后的闪烁高亮截止时间，None 表示不闪烁
    pub mode_highlight_until: Option<std::time::Instant>,
}

impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // 优先从 ~/.zen-code/settings.json 加载配置，失败时 fallback 到环境变量
        let zen_config = crate::config::load().ok();

        let provider_from_config = zen_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config);
        let (provider_name, model_name, _status_msg) =
            match provider_from_config.or_else(agent::LlmProvider::from_env) {
                Some(p) => {
                    let name = p.display_name().to_string();
                    let model = p.model_name().to_string();
                    let msg = format!("{} ({}) 已就绪", name, model);
                    (name, model, msg)
                }
                None => (
                    "未配置".to_string(),
                    "无".to_string(),
                    "警告: 未设置任何 API Key（ANTHROPIC_API_KEY 或 OPENAI_API_KEY）".to_string(),
                ),
            };

        // 初始化 thread 存储（失败时 fallback 到临时目录）
        let thread_store: Arc<dyn ThreadStore> =
            Arc::new(SqliteThreadStore::default_path().unwrap_or_else(|_| {
                SqliteThreadStore::new(std::env::temp_dir().join("zen-threads.db"))
                    .expect("无法创建临时 SQLite 数据库")
            }));

        // 启动渲染线程（初始宽度 80，resize 事件后会更新）
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);

        // 预计算命令帮助列表
        let command_registry = crate::command::default_registry();
        let skills = {
            let mut dirs = Vec::new();
            if let Some(home) = dirs_next::home_dir() {
                dirs.push(home.join(".claude").join("skills"));
            }
            if let Some(global_dir) = rust_agent_middlewares::skills::load_global_skills_dir() {
                dirs.push(global_dir);
            }
            if let Ok(cwd) = std::env::current_dir() {
                dirs.push(cwd.join(".claude").join("skills"));
            }
            rust_agent_middlewares::skills::list_skills(&dirs)
        };

        // 初始化 cron state + spawn tick task
        let (cron_state, scheduler_arc) = CronState::new();
        CronState::spawn_tick_task(scheduler_arc);

        Self {
            core: AppCore::new(render_tx, render_cache, render_notify, command_registry, skills),
            agent: AgentComm::default(),
            langfuse: LangfuseState::default(),
            cwd,
            provider_name,
            model_name,
            zen_config,
            thread_store,
            current_thread_id: None,
            todo_items: Vec::new(),
            cron: cron_state,
            setup_wizard: None,
            permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
                rust_agent_middlewares::prelude::PermissionMode::BypassPermissions,
            ),
            mode_highlight_until: None,
        }
    }

    // ─── 转发访问器（保持 app.xxx 调用方式不变）─────────────────────────────────

    /// 中断正在运行的 Agent（Ctrl+C during loading）
    pub fn interrupt(&mut self) {
        if let Some(token) = &self.agent.cancel_token {
            token.cancel();
        }
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.core.loading = loading;
        self.core.textarea = build_textarea(loading, self.core.pending_messages.len());
        if !loading {
            self.agent.cancel_token = None;
        }
    }

    /// 更新输入框标题以反映缓冲消息数量
    pub fn update_textarea_hint(&mut self) {
        self.core.textarea = build_textarea(self.core.loading, self.core.pending_messages.len());
    }

    /// 设置当前 Agent 的 ID（用于 AgentDefineMiddleware）
    pub fn set_agent_id(&mut self, id: Option<String>) {
        self.agent.agent_id = id;
    }

    /// 获取当前 Agent 的 ID
    pub fn get_agent_id(&self) -> Option<&String> {
        self.agent.agent_id.as_ref()
    }

    /// 获取当前任务运行时长（运行中）或上次任务时长（已完成）
    pub fn get_current_task_duration(&self) -> Option<std::time::Duration> {
        if let Some(start) = self.agent.task_start_time {
            if self.core.loading {
                Some(start.elapsed())
            } else {
                self.agent.last_task_duration
            }
        } else {
            self.agent.last_task_duration
        }
    }

    /// Setup 向导保存后刷新内存中的 Provider 状态
    pub fn refresh_after_setup(&mut self, cfg: crate::config::ZenConfig) {
        self.zen_config = Some(cfg);
        let cfg_ref = self.zen_config.as_ref().unwrap();
        if let Some(p) = agent::LlmProvider::from_config(cfg_ref) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

}

/// 确保光标在滚动视口内可见，返回调整后的 scroll_offset
pub fn ensure_cursor_visible(cursor_row: u16, scroll_offset: u16, visible_height: u16) -> u16 {
    if visible_height == 0 {
        return 0;
    }
    if cursor_row < scroll_offset {
        cursor_row
    } else if cursor_row >= scroll_offset + visible_height {
        cursor_row.saturating_sub(visible_height - 1)
    } else {
        scroll_offset
    }
}

pub fn build_textarea(disabled: bool, buffered_count: usize) -> TextArea<'static> {
    let mut ta = TextArea::default();

    // Loading 状态：黄色边框 + "处理中…" 标题
    // 空闲状态：青色边框 + "输入" 标题
    let (border_color, title_text, title_color) = if disabled {
        if buffered_count > 0 {
            (
                theme::LOADING,
                format!(" 处理中… (已缓存 {} 条) ", buffered_count),
                theme::LOADING,
            )
        } else {
            (theme::LOADING, " 处理中… ".to_string(), theme::LOADING)
        }
    } else {
        (theme::ACCENT, " 输入 ".to_string(), theme::ACCENT)
    };

    ta.set_cursor_line_style(Style::default());
    ta.set_style(Style::default().fg(theme::TEXT));
    ta.set_block(
        ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(ratatui::text::Span::styled(
                title_text,
                Style::default()
                    .fg(title_color)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
    );
    ta
}
