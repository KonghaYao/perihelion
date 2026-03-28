pub mod agent;
pub mod agent_panel;
pub mod events;
pub mod interaction_broker;
pub mod model_panel;
pub mod relay_panel;
mod provider;
pub mod tool_display;

mod hitl_prompt;
mod ask_user_prompt;
mod hitl_ops;
mod ask_user_ops;
mod thread_ops;
mod panel_ops;
mod agent_ops;
mod relay_ops;
mod hint_ops;

pub use events::AgentEvent;
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
pub use ask_user_prompt::AskUserBatchPrompt;
pub use interaction_broker::TuiInteractionBroker;

/// 统一交互弹窗枚举：同一时刻只允许一种弹窗激活
pub enum InteractionPrompt {
    Approval(HitlBatchPrompt),
    Questions(AskUserBatchPrompt),
}

use ratatui::style::{Color, Style};
use ratatui_textarea::TextArea;
use rust_agent_middlewares::prelude::{HitlDecision, SkillMetadata, TodoItem};
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use tokio::sync::mpsc;

use crate::command::CommandRegistry;
use crate::config::ZenConfig;
use crate::thread::{SqliteThreadStore, ThreadBrowser, ThreadId, ThreadMeta, ThreadStore};

// Re-export MessageViewModel from ui::message_view
use crate::command::agents::AgentItem;
pub use crate::ui::message_view::{ContentBlockView, MessageViewModel};
pub use agent_panel::AgentPanel;
pub use model_panel::ModelPanel;
pub use relay_panel::RelayPanel;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::Instrument;

use crate::ui::render_thread::{RenderCache, RenderEvent};

// ─── App ──────────────────────────────────────────────────────────────────────

pub struct App {
    pub view_messages: Vec<MessageViewModel>,
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    agent_rx: Option<mpsc::Receiver<AgentEvent>>,
    /// 当前激活的交互弹窗（HITL 审批或 AskUser 问答，同一时刻只有一种）
    pub interaction_prompt: Option<InteractionPrompt>,
    /// 已发送待解决的 HITL 工具名称列表（用于 approval_resolved 广播）
    pending_hitl_items: Option<Vec<String>>,
    /// AskUser 是否已提交（用于广播 resolved）
    pending_ask_user: Option<bool>,
    /// 当前 TODO 列表（固定面板，不写入消息流）
    pub todo_items: Vec<TodoItem>,
    /// 内存中的配置快照（来自 ~/.zen-code/settings.json）
    pub zen_config: Option<ZenConfig>,
    /// /model 面板状态
    pub model_panel: Option<ModelPanel>,
    /// /agents 面板状态
    pub agent_panel: Option<AgentPanel>,
    /// /relay 面板状态
    pub relay_panel: Option<RelayPanel>,
    /// 命令注册表
    pub command_registry: CommandRegistry,
    /// 命令帮助文本缓存（启动时预计算，/help 直接读取，不受 std::mem::take 影响）
    pub command_help_list: Vec<(String, String)>,
    /// 可用 skills 列表（启动时加载）
    pub skills: Vec<SkillMetadata>,
    /// 提示浮层（命令/Skills）当前光标位置
    pub hint_cursor: Option<usize>,
    /// Thread 持久化存储
    pub thread_store: Arc<dyn ThreadStore>,
    /// 当前会话的 thread id（选择或新建后设置）
    pub current_thread_id: Option<ThreadId>,
    /// 启动时的历史浏览面板（选择后关闭）
    pub thread_browser: Option<ThreadBrowser>,
    /// 已持久化到 thread 的消息数量（用于增量追加）
    persisted_count: usize,
    /// 当前 Agent 任务的取消令牌（loading 时有效，Ctrl+C 触发）
    cancel_token: Option<AgentCancellationToken>,
    /// 当前 Agent 任务开始时间（用于计算运行时长）
    task_start_time: Option<std::time::Instant>,
    /// 上一次任务的总运行时长（任务结束后保留显示）
    last_task_duration: Option<std::time::Duration>,
    /// 持久化的 Agent 消息历史（多轮对话的上下文）
    agent_state_messages: Vec<rust_create_agent::messages::BaseMessage>,
    /// 当前 Agent 的 ID（用于 AgentDefineMiddleware 加载 agent 定义）
    agent_id: Option<String>,
    /// 渲染线程事件发送端（无界 channel，避免 try_send 静默丢弃导致渲染状态分叉）
    pub render_tx: mpsc::UnboundedSender<RenderEvent>,
    /// 渲染缓存（UI 线程只读）
    pub render_cache: Arc<RwLock<RenderCache>>,
    /// 渲染线程完成通知
    #[allow(dead_code)]
    pub render_notify: Arc<Notify>,
    /// UI 线程记录的最后绘制版本
    pub last_render_version: u64,
    /// 测试用事件注入队列（仅测试时使用，生产时保持为空）
    #[doc(hidden)]
    #[allow(dead_code)]
    pub agent_event_queue: Vec<AgentEvent>,
    /// Loading 期间的消息缓冲区（完成后合并发送）
    pub pending_messages: Vec<String>,
    /// 待发送的图片附件（Ctrl+V 粘贴图片后缓存，发送时消费）
    pub pending_attachments: Vec<PendingAttachment>,
    /// 是否显示工具调用消息（默认 false，完全隐藏）
    pub show_tool_messages: bool,
    /// 当前活跃 SubAgentGroup 在 view_messages 中的下标（子 agent 执行中时有值）
    subagent_group_idx: Option<usize>,
    /// Relay 客户端（远程控制，可选）
    relay_client: Option<Arc<rust_relay_server::client::RelayClient>>,
    /// Relay 事件接收端（来自 Web 端的控制消息）
    relay_event_rx: Option<rust_relay_server::client::RelayEventRx>,
    /// Relay 连接参数缓存（url, token, name），断线后自动重连使用
    relay_params: Option<(String, String, Option<String>)>,
    /// Relay 重连计划时间（达到后尝试重连，None 表示不需要重连）
    relay_reconnect_at: Option<std::time::Instant>,
    /// Thread 级别的 Langfuse Session（Thread 创建/打开时懒加载，new_thread/open_thread 时重置）
    langfuse_session: Option<Arc<crate::langfuse::LangfuseSession>>,
    /// 当前轮次的 Langfuse Tracer（submit_message 时创建，Done 时结束，未配置时为 None）
    langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
    /// on_trace_end 返回的 flush JoinHandle，进程退出前应 await 确保 batcher flush 完成
    pub langfuse_flush_handle: Option<tokio::task::JoinHandle<()>>,
}

impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let textarea = build_textarea(false, 0);

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

        // 预计算命令帮助列表（在注册表被 std::mem::take 时仍可读）
        let command_registry = crate::command::default_registry();
        let command_help_list: Vec<(String, String)> = command_registry
            .list()
            .into_iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect();

        let mut app = Self {
            view_messages: Vec::new(),
            textarea,
            loading: false,
            scroll_offset: u16::MAX,
            scroll_follow: true,
            cwd: cwd.clone(),
            provider_name,
            model_name,
            agent_rx: None,
            interaction_prompt: None,
            todo_items: Vec::new(),
            zen_config,
            model_panel: None,
            agent_panel: None,
            relay_panel: None,
            command_registry,
            command_help_list,
            hint_cursor: None,
            skills: {
                let mut dirs = Vec::new();
                // 用户级 skills（优先）
                if let Some(home) = dirs_next::home_dir() {
                    dirs.push(home.join(".claude").join("skills"));
                }
                // 全局配置的 skillsDir（~/.zen-code/settings.json）
                if let Some(global_dir) = rust_agent_middlewares::skills::load_global_skills_dir() {
                    dirs.push(global_dir);
                }
                // 项目级 skills
                if let Ok(cwd) = std::env::current_dir() {
                    dirs.push(cwd.join(".claude").join("skills"));
                }
                rust_agent_middlewares::skills::list_skills(&dirs)
            },
            thread_store,
            current_thread_id: None,
            thread_browser: None,
            persisted_count: 0,
            cancel_token: None,
            task_start_time: None,
            last_task_duration: None,
            agent_state_messages: Vec::new(),
            agent_id: None,
            render_tx,
            render_cache,
            render_notify,
            last_render_version: 0,
            agent_event_queue: Vec::new(),
            pending_messages: Vec::new(),
            pending_attachments: Vec::new(),
            show_tool_messages: false,
            subagent_group_idx: None,
            relay_client: None,
            relay_event_rx: None,
            relay_params: None,
            relay_reconnect_at: None,
            pending_hitl_items: None,
            pending_ask_user: None,
            langfuse_session: None,
            langfuse_tracer: None,
            langfuse_flush_handle: None,
        };

        let sys_msg = MessageViewModel::system(format!("CWD: {}", cwd));
        app.view_messages.push(sys_msg.clone());
        let _ = app.render_tx.send(RenderEvent::AddMessage(sys_msg));

        app
    }

    /// 尝试连接 Relay Server
    /// CLI 参数（--remote-control）优先，其次从 zen_config 读取配置
    /// 支持三层优先级：
    /// 1. CLI 参数完整指定（URL 非空）→ 使用 CLI 参数
    /// 2. CLI 参数 `--remote-control` 无 URL → 从 `remote_control` 字段读取
    /// 3. `remote_control` 字段不存在 → fallback 到 `extra.relay_*`（向后兼容）
    pub async fn try_connect_relay(&mut self, cli: Option<&crate::RelayCli>) {
        let (relay_url, relay_token, relay_name) = if let Some(c) = cli {
            if c.url.is_empty() {
                // --remote-control 无参数：从配置读取
                let config = self.zen_config
                    .as_ref()
                    .and_then(|cfg| cfg.config.remote_control.as_ref())
                    .filter(|rc| rc.is_complete());

                match config {
                    Some(rc) => (rc.url.clone(), rc.token.clone(), rc.name.clone()),
                    None => {
                        // 回退到旧 extra 字段（向后兼容）
                        let extra_config = self.zen_config
                            .as_ref()
                            .and_then(|cfg| cfg.config.extra.get("relay_url"))
                            .and_then(|v| v.as_str());
                        if extra_config.is_none() {
                            let msg = MessageViewModel::from_base_message(
                                &BaseMessage::system("未配置远程控制，请使用 /relay 命令配置".to_string()),
                                &[],
                            );
                            let _ = self.render_tx.send(RenderEvent::AddMessage(msg));
                            return;
                        }
                        let url = extra_config.unwrap().to_string();
                        let token = self.zen_config
                            .as_ref()
                            .and_then(|cfg| cfg.config.extra.get("relay_token"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = self.zen_config
                            .as_ref()
                            .and_then(|cfg| cfg.config.extra.get("relay_name"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (url, token, name)
                    }
                }
            } else {
                // --remote-control <url>：使用 CLI 参数（token 可从配置 fallback）
                let token = c.token.clone().unwrap_or_else(|| {
                    // 优先从新字段读取，fallback 到 extra 字段
                    self.zen_config
                        .as_ref()
                        .and_then(|cfg| cfg.config.remote_control.as_ref())
                        .map(|rc| rc.token.clone())
                        .unwrap_or_else(|| {
                            self.zen_config
                                .as_ref()
                                .and_then(|cfg| cfg.config.extra.get("relay_token"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string()
                        })
                });
                (c.url.clone(), token, c.name.clone())
            }
        } else {
            // 无 --remote-control 参数：不尝试连接
            return;
        };

        // 缓存参数供断线重连使用
        self.relay_params = Some((relay_url.clone(), relay_token.clone(), relay_name.clone()));

        match rust_relay_server::client::RelayClient::connect(
            &relay_url,
            &relay_token,
            relay_name.as_deref(),
        )
        .await
        {
            Ok((client, event_rx)) => {
                let sid = client.session_id.read().await.clone().unwrap_or_default();
                // 在 TUI 消息区域显示连接状态（不用 tracing，避免 raw mode 乱码）
                let status_msg = format!("Relay connected (session: {})", &sid[..8.min(sid.len())]);
                let vm = MessageViewModel::from_base_message(&BaseMessage::system(status_msg), &[]);
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                self.relay_client = Some(Arc::new(client));
                self.relay_event_rx = Some(event_rx);
                self.relay_reconnect_at = None; // 连接成功，取消重连计划
            }
            Err(e) => {
                // 不用 tracing，通过 TUI 消息显示
                let err_msg = format!("Relay connection failed: {}", e);
                let vm = MessageViewModel::from_base_message(&BaseMessage::system(err_msg), &[]);
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                // 连接失败时，3 秒后重试
                self.relay_reconnect_at = Some(
                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                );
            }
        }
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
        self.textarea = build_textarea(loading, self.pending_messages.len());
        if !loading {
            self.cancel_token = None;
        }
    }

    /// 更新输入框标题以反映缓冲消息数量
    pub fn update_textarea_hint(&mut self) {
        self.textarea = build_textarea(self.loading, self.pending_messages.len());
    }

    /// 设置当前 Agent 的 ID（用于 AgentDefineMiddleware）
    pub fn set_agent_id(&mut self, id: Option<String>) {
        self.agent_id = id;
    }

    /// 获取当前 Agent 的 ID
    pub fn get_agent_id(&self) -> Option<&String> {
        self.agent_id.as_ref()
    }

    /// 中断正在运行的 Agent（Ctrl+C during loading）
    pub fn interrupt(&mut self) {
        if let Some(token) = &self.cancel_token {
            token.cancel();
        }
    }

    /// 获取当前任务运行时长（运行中）或上次任务时长（已完成）
    pub fn get_current_task_duration(&self) -> Option<std::time::Duration> {
        if let Some(start) = self.task_start_time {
            if self.loading {
                Some(start.elapsed())
            } else {
                self.last_task_duration
            }
        } else {
            self.last_task_duration
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
                Color::Yellow,
                format!(" 处理中… (已缓存 {} 条) ", buffered_count),
                Color::Yellow,
            )
        } else {
            (Color::Yellow, " 处理中… ".to_string(), Color::Yellow)
        }
    } else {
        (Color::Cyan, " 输入 ".to_string(), Color::Cyan)
    };

    ta.set_cursor_line_style(Style::default());
    ta.set_style(Style::default().fg(Color::White));
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

