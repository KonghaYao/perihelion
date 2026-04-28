use std::sync::Arc;

use parking_lot::RwLock;
use ratatui_textarea::TextArea;
use rust_agent_middlewares::prelude::SkillMetadata;
use tokio::sync::{mpsc, Notify};

use crate::command::CommandRegistry;
use crate::ui::message_view::MessageViewModel;
use crate::ui::render_thread::{RenderCache, RenderEvent};

use super::agent_panel::AgentPanel;
use super::hitl_prompt::PendingAttachment;
use super::login_panel::LoginPanel;
use super::model_panel::ModelPanel;
use crate::thread::ThreadBrowser;

/// UI 核心状态：消息、输入、面板、渲染
pub struct AppCore {
    pub view_messages: Vec<MessageViewModel>,
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub pending_messages: Vec<String>,
    pub subagent_group_idx: Option<usize>,
    pub render_tx: mpsc::UnboundedSender<RenderEvent>,
    pub render_cache: Arc<RwLock<RenderCache>>,
    pub render_notify: Arc<Notify>,
    pub last_render_version: u64,
    pub command_registry: CommandRegistry,
    pub command_help_list: Vec<(String, String)>,
    pub skills: Vec<SkillMetadata>,
    pub hint_cursor: Option<usize>,
    pub pending_attachments: Vec<PendingAttachment>,
    pub last_human_message: Option<String>,
    pub model_panel: Option<ModelPanel>,
    pub login_panel: Option<LoginPanel>,
    pub agent_panel: Option<AgentPanel>,
    pub thread_browser: Option<ThreadBrowser>,
}

impl AppCore {
    /// 创建带渲染线程的 AppCore（生产用）
    pub fn new(render_tx: mpsc::UnboundedSender<RenderEvent>,
               render_cache: Arc<RwLock<RenderCache>>,
               render_notify: Arc<Notify>,
               command_registry: CommandRegistry,
               skills: Vec<SkillMetadata>) -> Self {
        let command_help_list: Vec<(String, String)> = command_registry
            .list()
            .into_iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect();
        Self {
            view_messages: Vec::new(),
            textarea: super::build_textarea(false),
            loading: false,
            scroll_offset: u16::MAX,
            scroll_follow: true,
            show_tool_messages: false,
            pending_messages: Vec::new(),
            subagent_group_idx: None,
            render_tx,
            render_cache,
            render_notify,
            last_render_version: 0,
            command_registry,
            command_help_list,
            skills,
            hint_cursor: None,
            pending_attachments: Vec::new(),
            last_human_message: None,
            model_panel: None,
            login_panel: None,
            agent_panel: None,
            thread_browser: None,
        }
    }
}
