pub mod agent;
pub mod agent_panel;
pub mod chat_session;
pub mod config_panel;
pub mod events;
pub mod hooks_panel;
pub mod interaction_broker;
pub mod login_panel;
pub mod memory_panel;
pub mod model_panel;
pub mod plugin_panel;
mod provider;
pub mod setup_wizard;
pub mod status_panel;
pub mod text_selection;
pub mod tool_display;

mod service_registry;
pub use service_registry::ServiceRegistry;

mod session_manager;
pub use session_manager::SessionManager;

mod ui_state;
pub use ui_state::UiState;

mod message_state;
pub use message_state::MessageState;

mod command_system;
mod session_metadata;
pub use command_system::CommandSystem;
pub use session_metadata::SessionMetadata;

mod agent_comm;
mod agent_ops;
mod ask_user_ops;
mod ask_user_prompt;
mod cron_ops;
mod cron_state;
mod hint_ops;
mod history_ops;
mod hitl_ops;
mod hitl_prompt;
mod langfuse_state;
mod mcp_panel;
pub mod message_pipeline;
mod oauth_prompt;
pub mod panel_component;
pub mod panel_manager;
mod panel_ops;
mod thread_ops;

pub use ask_user_prompt::AskUserBatchPrompt;
pub use chat_session::ChatSession;
pub use events::AgentEvent;
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
pub use interaction_broker::TuiInteractionBroker;
pub use oauth_prompt::OAuthPrompt;

/// 统一交互弹窗枚举：同一时刻只允许一种弹窗激活
pub enum InteractionPrompt {
    Approval(HitlBatchPrompt),
    Questions(AskUserBatchPrompt),
}

use crate::ui::theme;
use ratatui::style::Style;
use ratatui::text::Span;
use rust_agent_middlewares::prelude::HitlDecision;
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use tokio::sync::mpsc;
use tracing::Instrument;
use tui_textarea::TextArea;

use crate::config::PeriConfig;
use crate::thread::{SqliteThreadStore, ThreadBrowser, ThreadId, ThreadMeta, ThreadStore};

// Re-export MessageViewModel from ui::message_view
use crate::command::agents::AgentItem;
pub use crate::ui::message_view::{
    aggregate_tail_tool_groups, aggregate_tool_groups, ContentBlockView, MessageViewModel,
};
pub use agent_panel::AgentPanel;
pub use hooks_panel::HooksPanel;
pub use model_panel::ModelPanel;
pub use setup_wizard::SetupWizardPanel;
use std::sync::Arc;

use crate::ui::render_thread::RenderEvent;

// Re-export sub-structs
pub use agent_comm::AgentComm;
pub use agent_comm::RetryStatus;
pub use cron_state::{CronPanel, CronState};
pub use langfuse_state::LangfuseState;
pub use mcp_panel::{DetailAction, McpPanel, McpPanelView};
pub use panel_component::PanelComponent;
pub use panel_manager::{
    EventResult, MutexGroup, PanelContext, PanelKind, PanelManager, PanelScope, PanelState,
};

// ─── App ──────────────────────────────────────────────────────────────────────

pub struct App {
    /// 会话管理器（sessions + active + session_areas）
    pub session_mgr: SessionManager,
    /// 全局服务/状态聚合（跨 session 共享）
    pub services: ServiceRegistry,
    pub global_panels: panel_manager::PanelManager,
}

impl App {
    pub async fn new() -> Self {
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // 优先从 ~/.peri/settings.json 加载配置，失败时 fallback 到环境变量
        let peri_config = crate::config::load().ok();

        let provider_from_config = peri_config
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
        let thread_store: Arc<dyn ThreadStore> = match SqliteThreadStore::default_path().await {
            Ok(store) => Arc::new(store),
            Err(_) => Arc::new(
                SqliteThreadStore::new(std::env::temp_dir().join("zen-threads.db"))
                    .await
                    .expect("无法创建临时 SQLite 数据库"),
            ),
        };

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

        let (bg_event_tx, bg_event_rx) = tokio::sync::mpsc::channel(32);

        let initial_session = ChatSession::new(cwd.clone(), command_registry, skills);

        let session_mgr = SessionManager::new(initial_session);

        let permission_mode = rust_agent_middlewares::prelude::SharedPermissionMode::new(
            rust_agent_middlewares::prelude::PermissionMode::Bypass,
        );
        let services = ServiceRegistry {
            peri_config: peri_config.clone(),
            cwd: cwd.clone(),
            provider_name: provider_name.clone(),
            model_name: model_name.clone(),
            permission_mode: permission_mode.clone(),
            thread_store: thread_store.clone(),
            mcp_pool: None,
            mcp_init_rx: None,
            cron: cron_state,
            plugin_data: None,
            bg_event_tx: bg_event_tx.clone(),
            bg_event_rx: Some(bg_event_rx),
            config_path_override: None,
            claude_settings_override: None,
            setup_wizard: None,
            oauth_prompt: None,
            mode_highlight_until: None,
            model_highlight_until: None,
            mcp_ready_shown_until: std::cell::Cell::new(None),
            quit_pending_since: None,
        };

        Self {
            session_mgr,
            services,
            global_panels: panel_manager::PanelManager::new(),
        }
    }

    // ─── Session 访问器 ─────────────────────────────────────────────────────

    /// 获取当前激活 session 的不可变引用
    pub fn active(&self) -> &ChatSession {
        self.session_mgr.current()
    }

    /// 获取当前激活 session 的可变引用
    pub fn active_mut(&mut self) -> &mut ChatSession {
        self.session_mgr.current_mut()
    }

    /// 获取指定 session 的不可变引用
    pub fn session_at(&self, idx: usize) -> Option<&ChatSession> {
        self.session_mgr.session_at(idx)
    }

    /// 获取指定 session 的可变引用
    pub fn session_at_mut(&mut self, idx: usize) -> Option<&mut ChatSession> {
        self.session_mgr.session_at_mut(idx)
    }

    /// 创建新 session 并切换到它
    pub fn new_session(&mut self) {
        let mut command_registry = crate::command::default_registry();
        let mut skills = {
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
        // 追加插件 skills（去重）
        if let Some(pd) = &self.services.plugin_data {
            let plugin_skills = rust_agent_middlewares::skills::list_skills(&pd.all_skill_dirs);
            let existing_names: std::collections::HashSet<String> =
                skills.iter().map(|s| s.name.clone()).collect();
            for skill in plugin_skills {
                if !existing_names.contains(&skill.name) {
                    skills.push(skill);
                }
            }
            command_registry.register_plugin_commands(pd.all_commands.clone());
        }
        let session = ChatSession::new(self.services.cwd.clone(), command_registry, skills);
        self.session_mgr.sessions.push(session);
        self.session_mgr.active = self.session_mgr.sessions.len() - 1;
    }

    /// 关闭当前 session（保留 ≥1），返回被关闭 session 的 index
    pub fn close_session(&mut self) -> Option<usize> {
        if self.session_mgr.sessions.len() <= 1 {
            return None;
        }
        let idx = self.session_mgr.active;
        // 如果有运行中的 agent，取消它
        if let Some(token) = &self.session_mgr.sessions[idx].agent.cancel_token {
            token.cancel();
        }
        self.session_mgr.sessions.remove(idx);
        // 调整 active index
        if self.session_mgr.active >= self.session_mgr.sessions.len() {
            self.session_mgr.active = self.session_mgr.sessions.len() - 1;
        }
        Some(idx)
    }

    /// 切换到下一个 session（循环）
    pub fn switch_next_session(&mut self) {
        if self.session_mgr.sessions.len() <= 1 {
            return;
        }
        self.session_mgr.active = (self.session_mgr.active + 1) % self.session_mgr.sessions.len();
    }

    /// 切换到上一个 session（循环）
    pub fn switch_prev_session(&mut self) {
        if self.session_mgr.sessions.len() <= 1 {
            return;
        }
        self.session_mgr.active = if self.session_mgr.active == 0 {
            self.session_mgr.sessions.len() - 1
        } else {
            self.session_mgr.active - 1
        };
    }

    /// 后台初始化 MCP 连接池（不阻塞 UI），在 run_app 中 App::new() 之后调用
    pub fn spawn_mcp_init(&mut self) {
        use rust_agent_middlewares::mcp::{McpClientPool, McpInitStatus};

        let pool = Arc::new(McpClientPool::new_pending());
        self.services.mcp_pool = Some(pool.clone());

        let (init_tx, init_rx) = tokio::sync::watch::channel(McpInitStatus::Pending);
        self.services.mcp_init_rx = Some(init_rx);

        let cwd = self.services.cwd.clone();
        let tx = self.services.bg_event_tx.clone();
        let oauth_cb: Box<dyn Fn(rust_agent_middlewares::mcp::OAuthFlowEvent) + Send + Sync> =
            Box::new(move |ev| {
                use rust_agent_middlewares::mcp::OAuthFlowEvent;
                if let OAuthFlowEvent::AuthorizationNeeded {
                    server_name,
                    authorization_url,
                    callback_tx,
                } = ev
                {
                    let _ = tx.try_send(events::AgentEvent::OAuthAuthorizationNeeded {
                        server_name,
                        authorization_url,
                        callback_tx,
                    });
                }
            });

        let claude_home = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");

        tokio::spawn(async move {
            McpClientPool::run_initialize(
                pool,
                std::path::Path::new(&cwd),
                &claude_home,
                init_tx,
                Some(oauth_cb),
            )
            .await;
        });
    }

    /// 保存配置：优先写入 override 路径（测试用），否则写入全局路径
    pub fn save_config(
        cfg: &PeriConfig,
        override_path: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        match override_path {
            Some(path) => crate::config::store::save_to(cfg, path),
            None => crate::config::save(cfg),
        }
    }

    // ─── 转发访问器（通过 active session 路由）──────────────────────────────

    /// 中断正在运行的 Agent（Ctrl+C during loading）
    pub fn interrupt(&mut self) {
        if let Some(token) = &self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .cancel_token
        {
            token.cancel();
        } else if self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .loading
        {
            tracing::warn!("interrupt: 无 cancel_token 但 loading=true，强制清理");
            self.set_loading(false);
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_rx = None;
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .interaction_prompt = None;
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .pending_hitl_items = None;
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .pending_ask_user = None;
            if let Some(start) = self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .task_start_time
            {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .last_task_duration = Some(start.elapsed());
            }

            // 如果 agent 尚未回复，恢复用户文本到输入框
            if !self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_replied
            {
                if let Some(text) = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .last_submitted_text
                    .take()
                {
                    let round_start = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .round_start_vm_idx;
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages
                        .truncate(round_start);
                    {
                        let remaining = self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages
                            .clone();
                        let _ = self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .render_tx
                            .send(RenderEvent::LoadHistory(remaining));
                    }
                    // 截断 agent_state_messages（回滚 StateSnapshot 扩展的内容）
                    let pre_len = self.session_mgr.sessions[self.session_mgr.active]
                        .metadata
                        .pre_submit_state_len;
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_state_messages
                        .truncate(pre_len);
                    // 清除 pipeline 状态
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .pipeline
                        .done();
                    let restored = self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .agent_state_messages
                        .clone();
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .pipeline
                        .restore_completed(restored);
                    let mut ta = build_textarea(false);
                    ta.insert_str(text.clone());
                    self.session_mgr.sessions[self.session_mgr.active]
                        .ui
                        .textarea = ta;
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .pending_messages
                        .clear();
                    self.session_mgr.sessions[self.session_mgr.active]
                        .metadata
                        .last_human_message = None;
                    let vm =
                        MessageViewModel::system("⚠ 已强制中断（输入已恢复到输入框）".to_string());
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages
                        .push(vm.clone());
                    let _ = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .render_tx
                        .send(RenderEvent::AddMessage(vm));
                } else {
                    let vm = MessageViewModel::system(
                        "⚠ 已强制中断（后台任务可能仍在运行）".to_string(),
                    );
                    self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .view_messages
                        .push(vm.clone());
                    let _ = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .render_tx
                        .send(RenderEvent::AddMessage(vm));
                }
            } else {
                let vm =
                    MessageViewModel::system("⚠ 已强制中断（后台任务可能仍在运行）".to_string());
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm.clone());
                let _ = self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .render_tx
                    .send(RenderEvent::AddMessage(vm));
            }
        }
    }

    pub fn set_loading(&mut self, loading: bool) {
        let s = self.active_mut();
        s.ui.loading = loading;
        if loading {
            s.ui.textarea = build_textarea(true);
            s.spinner_state
                .set_mode(perihelion_widgets::SpinnerMode::Responding);
        } else {
            s.spinner_state
                .set_mode(perihelion_widgets::SpinnerMode::Idle);
            s.agent.cancel_token = None;
        }
    }

    /// 重建输入框（pending_messages 现在由 UI 层直接渲染，不再使用 textarea title）
    pub fn update_textarea_hint(&mut self) {
        // 不再需要更新 textarea title，pending_messages 在输入框上方渲染
    }

    /// 设置当前 Agent 的 ID（用于 AgentDefineMiddleware）
    pub fn set_agent_id(&mut self, id: Option<String>) {
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_id = id;
    }

    /// 获取当前 Agent 的 ID
    pub fn get_agent_id(&self) -> Option<&String> {
        self.session_mgr.current().agent.agent_id.as_ref()
    }

    /// 获取当前任务运行时长（运行中）或上次任务时长（已完成）
    pub fn get_current_task_duration(&self) -> Option<std::time::Duration> {
        let s = self.session_mgr.current();
        if let Some(start) = s.agent.task_start_time {
            if s.ui.loading {
                Some(start.elapsed())
            } else {
                s.agent.last_task_duration
            }
        } else {
            s.agent.last_task_duration
        }
    }

    /// 打开面板（统一处理跨作用域互斥）：关闭所有 manager 中的面板后，放入正确的 manager
    pub fn open_panel(&mut self, state: panel_manager::PanelState) {
        match state.kind().scope() {
            panel_manager::PanelScope::Session => {
                self.global_panels.close();
                self.session_mgr.sessions[self.session_mgr.active]
                    .session_panels
                    .close();
                self.session_mgr.sessions[self.session_mgr.active]
                    .session_panels
                    .open(state);
            }
            panel_manager::PanelScope::Global => {
                self.global_panels.close();
                for session in &mut self.session_mgr.sessions {
                    session.session_panels.close();
                }
                self.global_panels.open(state);
            }
        }
    }

    /// 关闭所有面板（跨所有作用域）
    pub fn close_all_panels(&mut self) {
        self.global_panels.close();
        for session in &mut self.session_mgr.sessions {
            session.session_panels.close();
        }
    }

    /// Setup 向导保存后刷新内存中的 Provider 状态
    pub fn refresh_after_setup(&mut self, cfg: crate::config::PeriConfig) {
        self.services.peri_config = Some(cfg);
        let cfg_ref = self.services.peri_config.as_ref().unwrap();
        if let Some(p) = agent::LlmProvider::from_config(cfg_ref) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
    }

    pub fn get_compact_config(&self) -> rust_create_agent::agent::compact::CompactConfig {
        let mut config = self
            .services
            .peri_config
            .as_ref()
            .and_then(|zc| zc.config.compact.clone())
            .unwrap_or_default();
        config.apply_env_overrides();
        config
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

// ─── 公共单行文本编辑辅助 ────────────────────────────────────────────────────

/// 对单行 `String` + 光标位置统一处理编辑按键。
/// 返回 `true` 表示该按键已被消费（调用方应停止 match）。
///
/// 支持的按键：Char、Backspace、Delete、Left、Right、Home、End、
/// Ctrl+A(Home)、Ctrl+E(End)、Ctrl+K(kill to end)、Ctrl+U(kill to start)
pub fn handle_edit_key(buf: &mut String, cursor: &mut usize, input: tui_textarea::Input) -> bool {
    use tui_textarea::Key;
    match input {
        // ── 字符输入 ────────────────────────────────────────────────────────
        tui_textarea::Input {
            key: Key::Char(c),
            ctrl: false,
            alt: false,
            ..
        } => {
            let char_count = buf.chars().count();
            if *cursor > char_count {
                *cursor = char_count;
            }
            let byte_pos = buf
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(buf.len());
            buf.insert(byte_pos, c);
            *cursor += 1;
            true
        }
        // ── Backspace：删除光标前一个字符 ──────────────────────────────────
        tui_textarea::Input {
            key: Key::Backspace,
            ..
        } => {
            if *cursor > 0 && *cursor <= buf.len() {
                let byte_pos = buf.char_indices().nth(*cursor - 1).map(|(i, _)| i);
                let next_byte = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                if let Some(bp) = byte_pos {
                    buf.drain(bp..next_byte);
                    *cursor -= 1;
                }
            }
            true
        }
        // ── Delete：删除光标后一个字符 ─────────────────────────────────────
        tui_textarea::Input {
            key: Key::Delete, ..
        } => {
            if *cursor < buf.len() {
                let byte_pos = buf.char_indices().nth(*cursor).map(|(i, _)| i);
                let next_byte = buf
                    .char_indices()
                    .nth(*cursor + 1)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                if let Some(bp) = byte_pos {
                    buf.drain(bp..next_byte);
                }
            }
            true
        }
        // ── Left / Ctrl+A(Home) ────────────────────────────────────────────
        tui_textarea::Input {
            key: Key::Left,
            ctrl: false,
            ..
        } => {
            if *cursor > 0 {
                *cursor -= 1;
            }
            true
        }
        tui_textarea::Input { key: Key::Home, .. }
        | tui_textarea::Input {
            key: Key::Char('a'),
            ctrl: true,
            ..
        } => {
            *cursor = 0;
            true
        }
        // ── Right / Ctrl+E(End) ────────────────────────────────────────────
        tui_textarea::Input {
            key: Key::Right,
            ctrl: false,
            ..
        } => {
            if *cursor < buf.chars().count() {
                *cursor += 1;
            }
            true
        }
        tui_textarea::Input { key: Key::End, .. }
        | tui_textarea::Input {
            key: Key::Char('e'),
            ctrl: true,
            ..
        } => {
            *cursor = buf.chars().count();
            true
        }
        // ── Ctrl+K：删除光标到末尾 ──────────────────────────────────────────
        tui_textarea::Input {
            key: Key::Char('k'),
            ctrl: true,
            ..
        } => {
            if *cursor < buf.chars().count() {
                let byte_pos = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.truncate(byte_pos);
            }
            true
        }
        // ── Ctrl+U：删除开头到光标 ──────────────────────────────────────────
        tui_textarea::Input {
            key: Key::Char('u'),
            ctrl: true,
            ..
        } => {
            let char_count = buf.chars().count();
            if *cursor > 0 && *cursor <= char_count {
                let byte_pos = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.drain(..byte_pos);
                *cursor = 0;
            }
            true
        }
        _ => false,
    }
}

/// 将 `(buf, cursor)` 渲染为带光标块的字符串元组 `(before_cursor, after_cursor)`。
/// 调用方在两者之间插入 `█` 或 `▏` Span 即可。
pub fn edit_display_parts(buf: &str, cursor: usize) -> (String, String) {
    let chars: Vec<char> = buf.chars().collect();
    let clamped = cursor.min(chars.len());
    let before: String = chars[..clamped].iter().collect();
    let after: String = chars[clamped..].iter().collect();
    (before, after)
}

pub fn build_textarea(disabled: bool) -> TextArea<'static> {
    build_textarea_with_hint(disabled, "")
}

fn build_textarea_with_hint(_disabled: bool, hint: &str) -> TextArea<'static> {
    let mut ta = TextArea::default();

    // 统一灰色边框
    let border_color = theme::MUTED;

    ta.set_cursor_line_style(Style::default());
    ta.set_style(Style::default().fg(theme::TEXT));
    let mut block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
        .border_style(Style::default().fg(border_color))
        .padding(ratatui::widgets::Padding::new(2, 0, 0, 0));
    if !hint.is_empty() {
        block = block.title(Span::styled(
            hint.to_owned(),
            Style::default().fg(theme::MUTED),
        ));
    }
    ta.set_block(block);
    ta
}
