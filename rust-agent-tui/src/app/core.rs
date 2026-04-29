use std::sync::Arc;

use parking_lot::RwLock;
use tui_textarea::TextArea;
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

use super::message_pipeline::MessagePipeline;

/// UI 核心状态：消息、输入、面板、渲染
pub struct AppCore {
    pub view_messages: Vec<MessageViewModel>,
    pub pipeline: MessagePipeline,
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub pending_messages: Vec<String>,
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
    /// 输入历史（已发送消息的文本，最新的在前面）
    pub input_history: Vec<String>,
    /// 当前浏览的历史索引，None = 不在浏览历史
    pub history_index: Option<usize>,
    /// 进入历史浏览前的草稿内容，退出浏览时恢复
    pub draft_input: Option<String>,
    pub text_selection: crate::app::text_selection::TextSelection,
    /// 消息渲染区域的 Rect，每次 render() 时更新，用于鼠标事件坐标判定
    pub messages_area: Option<ratatui::layout::Rect>,
    /// 输入框渲染区域的 Rect，每次 render() 时更新，用于鼠标选区坐标判定
    pub textarea_area: Option<ratatui::layout::Rect>,
    /// 复制成功提示截止时间，None 表示不显示
    pub copy_message_until: Option<std::time::Instant>,
    /// 复制的字符数（用于提示文案）
    pub copy_char_count: usize,
    /// 面板文字选区状态（thread_browser / agent / cron 等列表面板）
    pub panel_selection: crate::app::text_selection::PanelTextSelection,
    /// 面板 inner 区域（去掉边框后），每次面板渲染时更新
    pub panel_area: Option<ratatui::layout::Rect>,
    /// 当前面板渲染内容的纯文本行
    pub panel_plain_lines: Vec<String>,
    /// 当前面板的滚动偏移
    pub panel_scroll_offset: u16,
}

impl AppCore {
    /// 创建带渲染线程的 AppCore（生产用）
    pub fn new(cwd: String,
               render_tx: mpsc::UnboundedSender<RenderEvent>,
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
            pipeline: MessagePipeline::new(cwd),
            textarea: super::build_textarea(false),
            loading: false,
            scroll_offset: u16::MAX,
            scroll_follow: true,
            show_tool_messages: false,
            pending_messages: Vec::new(),
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
            input_history: Vec::new(),
            history_index: None,
            draft_input: None,
            text_selection: crate::app::text_selection::TextSelection::new(),
            messages_area: None,
            textarea_area: None,
            copy_message_until: None,
            copy_char_count: 0,
            panel_selection: crate::app::text_selection::PanelTextSelection::new(),
            panel_area: None,
            panel_plain_lines: Vec::new(),
            panel_scroll_offset: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_appcore_pipeline_initialized() {
        let (render_tx, _, _) = crate::ui::render_thread::spawn_render_thread(80);
        let render_cache = Arc::new(RwLock::new(RenderCache {
            lines: Vec::new(),
            message_offsets: Vec::new(),
            total_lines: 0,
            version: 0,
            wrap_map: Vec::new(),
        }));
        let render_notify = Arc::new(tokio::sync::Notify::new());
        let command_registry = crate::command::default_registry();
        let skills = Vec::new();
        let cwd = "/test/path".to_string();

        let core = AppCore::new(
            cwd.clone(),
            render_tx,
            render_cache,
            render_notify,
            command_registry,
            skills,
        );

        assert_eq!(core.pipeline.cwd(), cwd);
        assert_eq!(core.pipeline.completed_messages().len(), 0);
    }
}
