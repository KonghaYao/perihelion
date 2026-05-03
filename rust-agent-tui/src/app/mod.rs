pub mod agent;
pub mod agent_panel;
pub mod chat_session;
pub mod config_panel;
pub mod events;
pub mod interaction_broker;
pub mod login_panel;
pub mod memory_panel;
pub mod model_panel;
mod provider;
pub mod setup_wizard;
pub mod status_panel;
pub mod text_selection;
pub mod tool_display;

mod agent_comm;
mod agent_ops;
mod ask_user_ops;
mod ask_user_prompt;
mod core;
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
mod panel_ops;
mod thread_ops;

pub use ask_user_prompt::AskUserBatchPrompt;
pub use chat_session::ChatSession;
pub use events::AgentEvent;
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
pub use interaction_broker::TuiInteractionBroker;
pub use oauth_prompt::OAuthPrompt;

use ratatui::layout::Rect;

/// 统一交互弹窗枚举：同一时刻只允许一种弹窗激活
pub enum InteractionPrompt {
    Approval(HitlBatchPrompt),
    Questions(AskUserBatchPrompt),
}

use crate::ui::theme;
use ratatui::style::Style;
use ratatui::text::Span;
use rust_agent_middlewares::prelude::{HitlDecision, TodoItem};
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use tokio::sync::mpsc;
use tui_textarea::TextArea;
use tracing::Instrument;

use crate::config::ZenConfig;
use crate::thread::{SqliteThreadStore, ThreadBrowser, ThreadId, ThreadMeta, ThreadStore};
use std::path::PathBuf;

// Re-export MessageViewModel from ui::message_view
use crate::command::agents::AgentItem;
pub use crate::ui::message_view::{ContentBlockView, MessageViewModel};
pub use agent_panel::AgentPanel;
pub use model_panel::ModelPanel;
pub use setup_wizard::SetupWizardPanel;
use std::sync::Arc;

use crate::ui::render_thread::RenderEvent;

// Re-export sub-structs
pub use agent_comm::AgentComm;
pub use agent_comm::RetryStatus;
pub use core::AppCore;
pub use cron_state::{CronPanel, CronState};
pub use langfuse_state::LangfuseState;
pub use mcp_panel::{DetailAction, McpPanel, McpPanelView};

// ─── App ──────────────────────────────────────────────────────────────────────

pub struct App {
    /// 所有聊天会话（每个 session 独立拥有 UI 状态、Agent 通道、线程上下文）
    pub sessions: Vec<ChatSession>,
    /// 当前激活（键盘焦点）的 session 索引
    pub active: usize,
    /// 各 session 列区域（供鼠标点击判断）
    pub session_areas: Vec<Rect>,
    // ─── 共享字段（跨 session 全局）─────────────────────────────────────────
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    pub zen_config: Option<ZenConfig>,
    pub thread_store: Arc<dyn ThreadStore>,
    pub cron: CronState,
    pub setup_wizard: Option<SetupWizardPanel>,
    pub permission_mode: Arc<rust_agent_middlewares::prelude::SharedPermissionMode>,
    /// 权限模式切换后的闪烁高亮截止时间，None 表示不闪烁
    pub mode_highlight_until: Option<std::time::Instant>,
    /// 模型切换后的闪烁高亮截止时间，None 表示不闪烁
    pub model_highlight_until: Option<std::time::Instant>,
    /// 测试时覆盖配置文件路径，防止污染全局 ~/.zen-code/settings.json
    pub config_path_override: Option<PathBuf>,
    /// MCP 连接池：首次 agent 启动时惰性初始化，App 退出时 shutdown
    pub mcp_pool: Option<Arc<rust_agent_middlewares::mcp::McpClientPool>>,
    /// MCP 后台初始化状态接收端
    pub mcp_init_rx:
        Option<tokio::sync::watch::Receiver<rust_agent_middlewares::mcp::McpInitStatus>>,
    /// OAuth 授权弹窗状态（None 表示无弹窗）
    pub oauth_prompt: Option<OAuthPrompt>,
    pub mcp_panel: Option<McpPanel>,
    /// 后台事件通道：供 spawn 的 MCP OAuth 等异步任务向 TUI 主循环发送事件
    pub bg_event_tx: tokio::sync::mpsc::Sender<AgentEvent>,
    pub bg_event_rx: Option<tokio::sync::mpsc::Receiver<AgentEvent>>,
    /// MCP 就绪提示显示截止时间（首次 Ready 时设置，3 秒后消失）
    pub mcp_ready_shown_until: std::cell::Cell<Option<std::time::Instant>>,
    /// /cost & /context 状态面板
    pub status_panel: Option<status_panel::StatusPanel>,
    /// /memory 记忆文件面板状态
    pub memory_panel: Option<crate::app::memory_panel::MemoryPanel>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
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

        let initial_session =
            ChatSession::new(cwd.clone(), command_registry, skills);

        Self {
            sessions: vec![initial_session],
            active: 0,
            session_areas: Vec::new(),
            cwd,
            provider_name,
            model_name,
            zen_config,
            thread_store,
            cron: cron_state,
            setup_wizard: None,
            permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
                rust_agent_middlewares::prelude::PermissionMode::Bypass,
            ),
            mode_highlight_until: None,
            model_highlight_until: None,
            config_path_override: None,
            mcp_pool: None,
            mcp_init_rx: None,
            mcp_panel: None,
            oauth_prompt: None,
            bg_event_tx,
            bg_event_rx: Some(bg_event_rx),
            mcp_ready_shown_until: std::cell::Cell::new(None),
            status_panel: None,
            memory_panel: None,
        }
    }

    // ─── Session 访问器 ─────────────────────────────────────────────────────

    /// 获取当前激活 session 的不可变引用
    pub fn active(&self) -> &ChatSession {
        &self.sessions[self.active]
    }

    /// 获取当前激活 session 的可变引用
    pub fn active_mut(&mut self) -> &mut ChatSession {
        &mut self.sessions[self.active]
    }

    /// 获取指定 session 的不可变引用
    pub fn session_at(&self, idx: usize) -> Option<&ChatSession> {
        self.sessions.get(idx)
    }

    /// 获取指定 session 的可变引用
    pub fn session_at_mut(&mut self, idx: usize) -> Option<&mut ChatSession> {
        self.sessions.get_mut(idx)
    }

    /// 创建新 session 并切换到它
    pub fn new_session(&mut self) {
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
        let session = ChatSession::new(self.cwd.clone(), command_registry, skills);
        self.sessions.push(session);
        self.active = self.sessions.len() - 1;
    }

    /// 关闭当前 session（保留 ≥1），返回被关闭 session 的 index
    pub fn close_session(&mut self) -> Option<usize> {
        if self.sessions.len() <= 1 {
            return None;
        }
        let idx = self.active;
        // 如果有运行中的 agent，取消它
        if let Some(token) = &self.sessions[idx].agent.cancel_token {
            token.cancel();
        }
        self.sessions.remove(idx);
        // 调整 active index
        if self.active >= self.sessions.len() {
            self.active = self.sessions.len() - 1;
        }
        Some(idx)
    }

    /// 切换到下一个 session（循环）
    pub fn switch_next_session(&mut self) {
        if self.sessions.len() <= 1 {
            return;
        }
        self.active = (self.active + 1) % self.sessions.len();
    }

    /// 切换到上一个 session（循环）
    pub fn switch_prev_session(&mut self) {
        if self.sessions.len() <= 1 {
            return;
        }
        self.active = if self.active == 0 {
            self.sessions.len() - 1
        } else {
            self.active - 1
        };
    }

    /// 后台初始化 MCP 连接池（不阻塞 UI），在 run_app 中 App::new() 之后调用
    pub fn spawn_mcp_init(&mut self) {
        use rust_agent_middlewares::mcp::{McpClientPool, McpInitStatus};

        let pool = Arc::new(McpClientPool::new_pending());
        self.mcp_pool = Some(pool.clone());

        let (init_tx, init_rx) = tokio::sync::watch::channel(McpInitStatus::Pending);
        self.mcp_init_rx = Some(init_rx);

        let cwd = self.cwd.clone();
        let tx = self.bg_event_tx.clone();
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
        tokio::spawn(async move {
            McpClientPool::run_initialize(
                pool,
                std::path::Path::new(&cwd),
                init_tx,
                Some(oauth_cb),
            )
            .await;
        });
    }

    /// 保存配置：优先写入 override 路径（测试用），否则写入全局路径
    pub fn save_config(
        cfg: &ZenConfig,
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
        if let Some(token) = &self.sessions[self.active].agent.cancel_token {
            token.cancel();
        } else if self.sessions[self.active].core.loading {
            tracing::warn!("interrupt: 无 cancel_token 但 loading=true，强制清理");
            self.set_loading(false);
            self.sessions[self.active].agent.agent_rx = None;
            self.sessions[self.active].agent.interaction_prompt = None;
            self.sessions[self.active].agent.pending_hitl_items = None;
            self.sessions[self.active].agent.pending_ask_user = None;
            if let Some(start) = self.sessions[self.active].agent.task_start_time {
                self.sessions[self.active].agent.last_task_duration = Some(start.elapsed());
            }
            let vm = MessageViewModel::system("⚠ 已强制中断（后台任务可能仍在运行）".to_string());
            self.sessions[self.active].core.view_messages.push(vm.clone());
            let _ = self.sessions[self.active].core.render_tx.send(RenderEvent::AddMessage(vm));
        }
    }

    pub fn set_loading(&mut self, loading: bool) {
        let s = self.active_mut();
        s.core.loading = loading;
        if loading {
            s.core.textarea = build_textarea(true);
            s.spinner_state
                .set_mode(perihelion_widgets::SpinnerMode::Responding);
        } else {
            s.spinner_state
                .set_mode(perihelion_widgets::SpinnerMode::Idle);
            s.agent.cancel_token = None;
        }
    }

    /// 更新输入框标题以反映缓冲消息数量
    pub fn update_textarea_hint(&mut self) {
        let s = self.active_mut();
        let count = s.core.pending_messages.len();
        let hint = if count > 0 {
            format!("已缓冲 {} 条消息，完成后自动发送…", count)
        } else {
            String::new()
        };
        s.core.textarea = build_textarea_with_hint(s.core.loading, &hint);
    }

    /// 设置当前 Agent 的 ID（用于 AgentDefineMiddleware）
    pub fn set_agent_id(&mut self, id: Option<String>) {
        self.sessions[self.active].agent.agent_id = id;
    }

    /// 获取当前 Agent 的 ID
    pub fn get_agent_id(&self) -> Option<&String> {
        self.active().agent.agent_id.as_ref()
    }

    /// 获取当前任务运行时长（运行中）或上次任务时长（已完成）
    pub fn get_current_task_duration(&self) -> Option<std::time::Duration> {
        let s = self.active();
        if let Some(start) = s.agent.task_start_time {
            if s.core.loading {
                Some(start.elapsed())
            } else {
                s.agent.last_task_duration
            }
        } else {
            s.agent.last_task_duration
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

    pub fn get_compact_config(&self) -> rust_create_agent::agent::compact::CompactConfig {
        let mut config = self
            .zen_config
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
