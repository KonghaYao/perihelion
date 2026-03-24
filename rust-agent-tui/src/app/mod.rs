pub mod agent;
pub mod agent_panel;
pub mod hitl;
pub mod model_panel;
mod provider;
pub mod tool_display;

use ratatui::style::{Color, Style};
use ratatui_textarea::TextArea;
use rust_agent_middlewares::ask_user::{AskUserBatchRequest, AskUserQuestionData};
use rust_agent_middlewares::prelude::{BatchItem, HitlDecision, SkillMetadata, TodoItem};
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use tokio::sync::mpsc;

use crate::command::CommandRegistry;
use crate::config::ZenConfig;
use crate::thread::{SqliteThreadStore, ThreadBrowser, ThreadId, ThreadMeta, ThreadStore};

// Re-export MessageViewModel from ui::message_view
use crate::command::agents::AgentItem;
use crate::ui::markdown::parse_markdown;
pub use crate::ui::message_view::{ContentBlockView, MessageViewModel};
pub use agent_panel::AgentPanel;
pub use hitl::{ApprovalEvent, BatchApprovalRequest};
pub use model_panel::ModelPanel;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::Instrument;

use crate::ui::render_thread::{RenderCache, RenderEvent};

// ─── AgentEvent ───────────────────────────────────────────────────────────────

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
}

// ─── PendingAttachment ────────────────────────────────────────────────────────

/// 待发送的图片附件（Ctrl+V 从剪贴板粘贴）
pub struct PendingAttachment {
    /// 显示名称，如 "clipboard_1.png"
    pub label: String,
    /// MIME 类型，固定为 "image/png"
    pub media_type: String,
    /// base64 编码的 PNG 数据
    pub base64_data: String,
    /// PNG 文件大小（字节，用于显示）
    pub size_bytes: usize,
}

// ─── HitlBatchPrompt ──────────────────────────────────────────────────────────

/// 批量 HITL 弹窗状态：每项独立的批准/拒绝选择
pub struct HitlBatchPrompt {
    /// 待审批的工具调用列表
    pub items: Vec<BatchItem>,
    /// 每项的当前决策（true=批准，false=拒绝）
    pub approved: Vec<bool>,
    /// 当前光标所在的行（工具索引）
    pub cursor: usize,
    /// 回复 channel
    pub response_tx: tokio::sync::oneshot::Sender<Vec<HitlDecision>>,
}

impl HitlBatchPrompt {
    pub fn new(
        items: Vec<BatchItem>,
        response_tx: tokio::sync::oneshot::Sender<Vec<HitlDecision>>,
    ) -> Self {
        let len = items.len();
        Self {
            items,
            approved: vec![true; len], // 默认全部批准
            cursor: 0,
            response_tx,
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        self.cursor = ((self.cursor as isize + delta).rem_euclid(len as isize)) as usize;
    }

    /// 切换当前项的批准/拒绝状态
    pub fn toggle_current(&mut self) {
        if let Some(v) = self.approved.get_mut(self.cursor) {
            *v = !*v;
        }
    }

    /// 全部批准
    pub fn approve_all(&mut self) {
        self.approved.iter_mut().for_each(|v| *v = true);
    }

    /// 全部拒绝
    pub fn reject_all(&mut self) {
        self.approved.iter_mut().for_each(|v| *v = false);
    }

    /// 确认并发送决策
    pub fn confirm(self) {
        let decisions: Vec<HitlDecision> = self
            .approved
            .iter()
            .map(|&ok| {
                if ok {
                    HitlDecision::Approve
                } else {
                    HitlDecision::Reject
                }
            })
            .collect();
        let _ = self.response_tx.send(decisions);
    }
}

// ─── AskUserBatchPrompt ───────────────────────────────────────────────────────

/// 单个问题的交互状态
pub struct QuestionState {
    pub data: AskUserQuestionData,
    pub option_cursor: isize, // 当前光标在第几个选项（最后一项 = 自定义输入行）
    pub selected: Vec<bool>,
    pub custom_input: String,
    pub in_custom_input: bool,
}

impl QuestionState {
    fn new(data: AskUserQuestionData) -> Self {
        let len = data.options.len();
        Self {
            data,
            option_cursor: 0,
            selected: vec![false; len],
            custom_input: String::new(),
            in_custom_input: false,
        }
    }

    fn total_rows(&self) -> isize {
        self.data.options.len() as isize + if self.data.allow_custom_input { 1 } else { 0 }
    }

    pub fn move_option_cursor(&mut self, delta: isize) {
        let total = self.total_rows();
        if total == 0 {
            return;
        }
        self.option_cursor = (self.option_cursor + delta).rem_euclid(total);
        self.in_custom_input =
            self.data.allow_custom_input && self.option_cursor == self.data.options.len() as isize;
    }

    pub fn toggle_current(&mut self) {
        if self.in_custom_input {
            return;
        }
        let i = self.option_cursor as usize;
        if i < self.selected.len() {
            if self.data.multi_select {
                self.selected[i] = !self.selected[i];
            } else {
                self.selected.iter_mut().for_each(|v| *v = false);
                self.selected[i] = true;
            }
        }
    }

    pub fn push_char(&mut self, c: char) {
        if self.in_custom_input {
            self.custom_input.push(c);
        }
    }

    pub fn pop_char(&mut self) {
        if self.in_custom_input {
            self.custom_input.pop();
        }
    }

    /// 收集当前问题的答案文本
    pub fn answer(&self) -> String {
        let mut parts: Vec<String> = self
            .selected
            .iter()
            .enumerate()
            .filter(|(_, &v)| v)
            .map(|(i, _)| self.data.options[i].label.clone())
            .collect();
        let custom = self.custom_input.trim().to_string();
        if !custom.is_empty() {
            parts.push(custom);
        }
        if parts.is_empty() {
            self.custom_input.trim().to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// 批量 AskUser 弹窗：多个问题用 Tab 切换，Enter 逐题确认，全部确认后提交
pub struct AskUserBatchPrompt {
    pub questions: Vec<QuestionState>,
    /// 当前激活的问题 tab 索引
    pub active_tab: usize,
    /// 每个问题是否已按 Enter 确认
    pub confirmed: Vec<bool>,
    pub response_tx: tokio::sync::oneshot::Sender<Vec<String>>,
    /// 内容滚动偏移
    pub scroll_offset: u16,
}

impl AskUserBatchPrompt {
    pub fn from_request(req: AskUserBatchRequest) -> Self {
        let len = req.questions.len();
        let questions = req.questions.into_iter().map(QuestionState::new).collect();
        Self {
            questions,
            active_tab: 0,
            confirmed: vec![false; len],
            response_tx: req.response_tx,
            scroll_offset: 0,
        }
    }

    pub fn next_tab(&mut self) {
        if !self.questions.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.questions.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.questions.is_empty() {
            self.active_tab = self
                .active_tab
                .checked_sub(1)
                .unwrap_or(self.questions.len() - 1);
        }
    }

    pub fn current(&mut self) -> &mut QuestionState {
        &mut self.questions[self.active_tab]
    }

    /// Enter 确认当前问题：标记已确认，跳到下一未确认的问题。
    /// 若所有问题都已确认，返回 true（调用方负责调用 confirm()）。
    pub fn confirm_current(&mut self) -> bool {
        self.confirmed[self.active_tab] = true;

        if self.confirmed.iter().all(|&c| c) {
            return true;
        }

        // 跳到下一个未确认的问题
        let n = self.questions.len();
        for offset in 1..=n {
            let next = (self.active_tab + offset) % n;
            if !self.confirmed[next] {
                self.active_tab = next;
                break;
            }
        }
        false
    }

    pub fn confirm(self) {
        let answers: Vec<String> = self.questions.iter().map(|q| q.answer()).collect();
        let _ = self.response_tx.send(answers);
    }
}

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
    /// 当前等待用户确认的批量 HITL 弹窗
    pub hitl_prompt: Option<HitlBatchPrompt>,
    /// 已发送待解决的 HITL 工具名称列表（用于 approval_resolved 广播）
    pending_hitl_items: Option<Vec<String>>,
    /// 当前等待用户输入的 AskUser 批量弹窗
    pub ask_user_prompt: Option<AskUserBatchPrompt>,
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
    /// Relay 客户端（远程控制，可选）
    relay_client: Option<Arc<rust_relay_server::client::RelayClient>>,
    /// Relay 事件接收端（来自 Web 端的控制消息）
    relay_event_rx: Option<rust_relay_server::client::RelayEventRx>,
    /// 当前轮次的 Langfuse Tracer（submit_message 时创建，Done 时结束，未配置时为 None）
    langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
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
            hitl_prompt: None,
            ask_user_prompt: None,
            todo_items: Vec::new(),
            zen_config,
            model_panel: None,
            agent_panel: None,
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
            relay_client: None,
            relay_event_rx: None,
            pending_hitl_items: None,
            pending_ask_user: None,
            langfuse_tracer: None,
        };

        let sys_msg = MessageViewModel::system(format!("CWD: {}", cwd));
        app.view_messages.push(sys_msg.clone());
        let _ = app.render_tx.send(RenderEvent::AddMessage(sys_msg));

        app
    }

    /// 尝试连接 Relay Server
    /// CLI 参数（--remote-control）优先，其次从 zen_config extra 中读取配置
    pub async fn try_connect_relay(&mut self, cli: Option<&crate::RelayCli>) {
        // CLI 参数优先
        let (relay_url, relay_token, relay_name) = if let Some(c) = cli {
            let token = c.token.clone().unwrap_or_else(|| {
                // token 回退到 settings.json
                self.zen_config
                    .as_ref()
                    .and_then(|cfg| cfg.config.extra.get("relay_token"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            });
            (c.url.clone(), token, c.name.clone())
        } else {
            // 无 CLI 参数时从 settings.json 读取
            let config = match &self.zen_config {
                Some(c) => &c.config.extra,
                None => return,
            };
            let url = match config.get("relay_url").and_then(|v| v.as_str()) {
                Some(u) => u.to_string(),
                None => return,
            };
            let token = config
                .get("relay_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = config
                .get("relay_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            (url, token, name)
        };

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
            }
            Err(e) => {
                // 不用 tracing，通过 TUI 消息显示
                let err_msg = format!("Relay connection failed: {}", e);
                let vm = MessageViewModel::from_base_message(&BaseMessage::system(err_msg), &[]);
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
            }
        }
    }

    /// 每帧调用：消费 Relay 事件（Web 端发来的控制消息）
    pub fn poll_relay(&mut self) -> bool {
        use rust_relay_server::protocol::WebMessage;

        // 先收集所有待处理事件（避免借用冲突）
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = self.relay_event_rx.as_mut() {
            loop {
                match rx.try_recv() {
                    Ok(msg) => events.push(msg),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        } else {
            return false;
        }

        if disconnected {
            // 不用 tracing，通过 TUI 消息显示
            self.relay_event_rx = None;
            self.relay_client = None;
            let vm = MessageViewModel::from_base_message(
                &BaseMessage::system("Relay disconnected"),
                &[],
            );
            let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
        }

        if events.is_empty() {
            return false;
        }

        for web_msg in events {
            match web_msg {
                WebMessage::UserInput { text } => {
                    // 不在此处发送 user 消息到 relay，由 executor 中的 MessageAdded 事件统一发送
                    if self.loading {
                        self.pending_messages.push(text);
                    } else {
                        self.submit_message(text);
                    }
                }
                WebMessage::HitlDecision { decisions } => {
                    if let Some(prompt) = self.hitl_prompt.take() {
                        // 远程控制支持全部 4 种 HITL 决策：Approve / Edit / Reject / Respond
                        let hitl_decisions: Vec<HitlDecision> = decisions
                            .iter()
                            .map(|d| match d.decision.as_str() {
                                "Approve" => HitlDecision::Approve,
                                "Edit" => {
                                    let new_input = d
                                        .input
                                        .as_deref()
                                        .and_then(|s| serde_json::from_str(s).ok())
                                        .unwrap_or(serde_json::json!({}));
                                    HitlDecision::Edit(new_input)
                                }
                                "Respond" => {
                                    HitlDecision::Respond(d.input.clone().unwrap_or_default())
                                }
                                _ => HitlDecision::Reject,
                            })
                            .collect();
                        let _ = prompt.response_tx.send(hitl_decisions);
                    }
                }
                WebMessage::AskUserResponse { answers } => {
                    if let Some(prompt) = self.ask_user_prompt.as_mut() {
                        for (q_text, answer) in &answers {
                            if let Some(q) = prompt
                                .questions
                                .iter_mut()
                                .find(|q| q.data.description == *q_text)
                            {
                                q.custom_input = answer.clone();
                                q.in_custom_input = true;
                            }
                        }
                        for c in prompt.confirmed.iter_mut() {
                            *c = true;
                        }
                    }
                    self.ask_user_confirm();
                }
                WebMessage::ClearThread => {
                    self.new_thread();
                }
                WebMessage::Pong => {}
                WebMessage::SyncRequest { since_seq } => {
                    if let Some(ref relay) = self.relay_client {
                        let events = relay.get_history_since(since_seq);
                        let response = serde_json::json!({
                            "type": "sync_response",
                            "events": events.iter()
                                .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap_or_default())
                                .collect::<Vec<_>>()
                        });
                        if let Ok(json) = serde_json::to_string(&response) {
                            relay.send_raw(&json);
                        }
                    }
                }
            }
        }
        true
    }

    /// 获取或新建当前 thread id（同步，block_in_place）
    fn ensure_thread_id(&mut self) -> ThreadId {
        if let Some(id) = &self.current_thread_id {
            return id.clone();
        }
        let meta = ThreadMeta::new(&self.cwd);
        let store = self.thread_store.clone();
        let id = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.create_thread(meta))
                .unwrap_or_else(|_| uuid::Uuid::now_v7().to_string())
        });
        self.current_thread_id = Some(id.clone());
        id
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
        self.scroll_follow = false;
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(3);
        self.scroll_follow = false;
    }

    /// 展开/折叠所有工具调用消息
    pub fn toggle_collapsed_messages(&mut self) {
        self.show_tool_messages = !self.show_tool_messages;
        let _ = self
            .render_tx
            .send(RenderEvent::ToggleToolMessages(self.show_tool_messages));
    }

    /// 获取当前提示浮层的候选数量和类型
    /// 返回 (候选总数, 选中的文本) — 用于 Tab 补全
    pub fn hint_candidates_count(&self) -> usize {
        let first_line = self
            .textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");
        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            self.command_registry.match_prefix(prefix).len()
        } else if first_line.starts_with('#') {
            let prefix = first_line.trim_start_matches('#');
            self.skills
                .iter()
                .filter(|s| prefix.is_empty() || s.name.contains(prefix))
                .take(8)
                .count()
        } else {
            0
        }
    }

    /// Tab 补全：选中当前光标处的候选项，替换输入框内容
    pub fn hint_complete(&mut self) {
        let first_line = self
            .textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let cursor = self.hint_cursor.unwrap_or(0);

        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            let candidates = self.command_registry.match_prefix(prefix);
            if let Some((name, _)) = candidates.get(cursor) {
                self.textarea = build_textarea(false, 0);
                self.textarea.insert_str(&format!("/{} ", name));
                self.hint_cursor = None;
            }
        } else if first_line.starts_with('#') {
            let prefix = first_line.trim_start_matches('#').to_string();
            let candidates: Vec<_> = self
                .skills
                .iter()
                .filter(|s| prefix.is_empty() || s.name.contains(&prefix))
                .take(8)
                .collect();
            if let Some(skill) = candidates.get(cursor) {
                self.textarea = build_textarea(false, 0);
                self.textarea.insert_str(&format!("#{} ", skill.name));
                self.hint_cursor = None;
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

    pub fn submit_message(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        // 消费待发送附件
        let attachments = std::mem::take(&mut self.pending_attachments);

        // 构建用于显示的文字（附件摘要追加在末尾）
        let display = if attachments.is_empty() {
            input.clone()
        } else {
            format!("{} [🖼 {} 张图片]", input, attachments.len())
        };
        let user_vm = MessageViewModel::user(display);
        self.view_messages.push(user_vm.clone());
        let _ = self.render_tx.send(RenderEvent::AddMessage(user_vm));
        self.set_loading(true);
        self.scroll_offset = u16::MAX;
        self.scroll_follow = true;
        self.todo_items.clear();

        // 开始计时新任务
        self.task_start_time = Some(std::time::Instant::now());
        self.last_task_duration = None;

        let provider = match self
            .zen_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config)
            .or_else(agent::LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                self.view_messages.push(MessageViewModel::tool_block(
                    "error".to_string(),
                    "config-error".to_string(),
                    None,
                    true,
                ));
                self.set_loading(false);
                return;
            }
        };

        let (tx, rx) = mpsc::channel(32);
        self.agent_rx = Some(rx);

        // 创建取消令牌（Ctrl+C 触发中断）
        let cancel = AgentCancellationToken::new();
        self.cancel_token = Some(cancel.clone());

        // YOLO_MODE 时跳过 HITL channel，直接给 agent 一个永远不会被消费的 sender
        let yolo = rust_agent_middlewares::is_yolo_mode();

        let (approval_tx, approval_rx) = mpsc::channel::<ApprovalEvent>(4);
        {
            let tx_hitl = tx.clone();
            tokio::spawn(async move {
                let mut approval_rx = approval_rx;
                while let Some(ev) = approval_rx.recv().await {
                    match ev {
                        ApprovalEvent::Batch(req) => {
                            if yolo {
                                // YOLO 模式：跳过弹窗，直接全部批准
                                let decisions = vec![HitlDecision::Approve; req.items.len()];
                                let _ = req.response_tx.send(decisions);
                            } else {
                                let _ = tx_hitl.send(AgentEvent::ApprovalNeeded(req)).await;
                            }
                        }
                        ApprovalEvent::AskUserBatch(req) => {
                            let _ = tx_hitl.send(AgentEvent::AskUserBatch(req)).await;
                        }
                    }
                }
            });
        }

        let cwd = self.cwd.clone();
        let system_prompt = crate::prompt::default_system_prompt(&cwd);

        // 构建多模态 AgentInput（有附件时包含图片 blocks）
        let agent_input = if attachments.is_empty() {
            AgentInput::text(input.clone())
        } else {
            let mut blocks = vec![ContentBlock::text(input.clone())];
            for att in &attachments {
                blocks.push(ContentBlock::image_base64(
                    &att.media_type,
                    &att.base64_data,
                ));
            }
            AgentInput::blocks(MessageContent::blocks(blocks))
        };

        // 确保当前 thread 存在
        let thread_id = self.ensure_thread_id();
        // 用户消息将由 agent 执行结束时的 StateSnapshot 统一持久化，
        // 避免与 StateSnapshot 竞争写 DB seq 导致序号错乱/重复写入
        // 用户消息已追加到 self.view_messages，更新已持久化计数
        self.persisted_count = self.view_messages.len();

        // 构造 Langfuse Tracer（未配置环境变量时静默跳过）
        let langfuse_tracer = crate::langfuse::LangfuseConfig::from_env()
            .and_then(|cfg| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(crate::langfuse::LangfuseTracer::new(cfg))
                })
            })
            .map(|mut t| {
                t.on_trace_start(input.trim(), self.current_thread_id.as_deref());
                Arc::new(parking_lot::Mutex::new(t))
            });
        self.langfuse_tracer = langfuse_tracer.clone();

        let span = tracing::info_span!(
            "thread.run",
            thread.id = %thread_id,
            thread.cwd = %cwd,
        );
        let history = self.agent_state_messages.clone();
        let agent_id = self.agent_id.clone();
        let relay_client = self.relay_client.clone();
        tokio::spawn(
            async move {
                agent::run_universal_agent(
                    provider,
                    agent_input,
                    cwd,
                    system_prompt,
                    thread_id,
                    history,
                    approval_tx,
                    tx,
                    cancel,
                    agent_id,
                    relay_client,
                    langfuse_tracer,
                )
                .await;
            }
            .instrument(span),
        );
    }

    /// 处理单个 AgentEvent，返回 `(updated, should_break, should_return)`
    fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
        match event {
            AgentEvent::ToolCall {
                tool_call_id: _tool_call_id,
                name,
                display,
                args,
                is_error,
            } => {
                let vm = MessageViewModel::tool_block(name, display, args, is_error);
                self.view_messages.push(vm.clone());
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                (true, false, false)
            }
            AgentEvent::MessageAdded(msg) => {
                // 只处理工具调用消息的渲染；纯文本 AI 消息由 AssistantChunk 处理
                match msg {
                    rust_create_agent::messages::BaseMessage::Ai {
                        content,
                        tool_calls,
                        ..
                    } => {
                        // 工具调用消息需要同步到 UI（折叠状态、工具调用列表）
                        if !tool_calls.is_empty() {
                            let text = match &content {
                                rust_create_agent::messages::MessageContent::Text(t) => t.clone(),
                                rust_create_agent::messages::MessageContent::Blocks(blocks) => blocks
                                    .iter()
                                    .filter_map(|b| match b {
                                        rust_create_agent::messages::ContentBlock::Text { text } => {
                                            Some(text.clone())
                                        }
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join(""),
                                _ => String::new(),
                            };

                            match self.view_messages.last_mut() {
                                Some(m) if m.is_assistant() => {
                                    // 追加文本到现有的 assistant 消息
                                    if !text.is_empty() {
                                        if let MessageViewModel::AssistantBubble {
                                            blocks, ..
                                        } = m
                                        {
                                            blocks.push(ContentBlockView::Text {
                                                raw: text.clone(),
                                                rendered: parse_markdown(&text),
                                                dirty: false,
                                            });
                                        }
                                    }
                                }
                                _ => {
                                    // 创建新的 assistant 消息（折叠状态）
                                    let mut vm = MessageViewModel::assistant();
                                    if let MessageViewModel::AssistantBubble {
                                        collapsed,
                                        blocks,
                                        ..
                                    } = &mut vm
                                    {
                                        *collapsed = true;
                                        if !text.is_empty() {
                                            blocks.push(ContentBlockView::Text {
                                                raw: text.clone(),
                                                rendered: parse_markdown(&text),
                                                dirty: false,
                                            });
                                        }
                                    }
                                    self.view_messages.push(vm.clone());
                                    let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                                }
                            }
                        }
                        // 纯文本 AI 消息由 AssistantChunk 事件处理，此处不重复渲染
                    }
                    _ => {}
                }
                (true, false, false)
            }
            AgentEvent::AssistantChunk(chunk) => {
                match self.view_messages.last_mut() {
                    Some(m) if m.is_assistant() => m.append_chunk(&chunk),
                    _ => {
                        let vm = MessageViewModel::assistant();
                        self.view_messages.push(vm.clone());
                        let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                    }
                }
                let _ = self.render_tx.send(RenderEvent::AppendChunk(chunk));
                (true, false, false)
            }
            AgentEvent::Done => {
                // 将最后一个 AssistantBubble 的 is_streaming 设为 false
                if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                    self.view_messages.last_mut()
                {
                    *is_streaming = false;
                }
                // 通知渲染线程清除流式指示器
                let _ = self.render_tx.send(RenderEvent::StreamingDone);
                // Langfuse：结束 Trace，上报最终答案
                if let Some(ref tracer) = self.langfuse_tracer {
                    let final_answer = self.view_messages.iter().rev()
                        .find_map(|m| {
                            if let MessageViewModel::AssistantBubble { blocks, .. } = m {
                                blocks.iter().find_map(|b| {
                                    if let ContentBlockView::Text { raw, .. } = b {
                                        Some(raw.clone())
                                    } else {
                                        None
                                    }
                                })
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    tracer.lock().on_trace_end(&final_answer);
                }
                self.langfuse_tracer = None;
                self.set_loading(false);
                self.agent_rx = None;
                // Agent 异常退出时清理残留弹窗状态，避免 UI 卡在弹窗
                self.hitl_prompt = None;
                self.ask_user_prompt = None;
                self.pending_hitl_items = None;
                self.pending_ask_user = None;
                if let Some(start) = self.task_start_time {
                    self.last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }
                (true, false, true)
            }
            AgentEvent::Interrupted => {
                // 中断：工具已以 error 结尾，持久化中间状态，下次发消息可断点续跑
                let vm = MessageViewModel::system(
                    "⚠ 已中断（工具调用已以 error 结尾，消息已保存，可继续发送恢复）".to_string(),
                );
                self.view_messages.push(vm.clone());
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                // Done 事件会紧随而来，由 Done 分支完成 set_loading + persist
                (true, false, false)
            }
            AgentEvent::Error(_e) => {
                let vm = MessageViewModel::tool_block(
                    "error".to_string(),
                    "agent-error".to_string(),
                    None,
                    true,
                );
                self.view_messages.push(vm.clone());
                let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                self.set_loading(false);
                self.agent_rx = None;
                // Agent 出错时清理残留弹窗状态，避免 UI 卡在弹窗
                self.hitl_prompt = None;
                self.ask_user_prompt = None;
                self.pending_hitl_items = None;
                self.pending_ask_user = None;
                if let Some(start) = self.task_start_time {
                    self.last_task_duration = Some(start.elapsed());
                }
                // 检查缓冲消息，合并发送
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }
                (true, false, true)
            }
            AgentEvent::ApprovalNeeded(req) => {
                // 转发 HITL 审批请求到 Relay
                if let Some(ref relay) = self.relay_client {
                    let items: Vec<serde_json::Value> = req
                        .items
                        .iter()
                        .map(|item| {
                            serde_json::json!({
                                "tool_name": item.tool_name,
                                "input": item.input,
                            })
                        })
                        .collect();
                    relay.send_value(serde_json::json!({
                        "type": "approval_needed",
                        "items": items,
                    }));
                }
                self.hitl_prompt = Some(HitlBatchPrompt::new(req.items, req.response_tx));
                (true, true, false) // 暂停消费，等待用户确认
            }
            AgentEvent::AskUserBatch(req) => {
                // 转发 AskUser 请求到 Relay
                self.pending_ask_user = Some(false);
                if let Some(ref relay) = self.relay_client {
                    let questions: Vec<serde_json::Value> = req.questions.iter().map(|q| {
                        serde_json::json!({
                            "question": q.description,
                            "options": q.options.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
                            "multi_select": q.multi_select,
                        })
                    }).collect();
                    relay.send_value(serde_json::json!({
                        "type": "ask_user_batch",
                        "questions": questions,
                    }));
                }
                self.ask_user_prompt = Some(AskUserBatchPrompt::from_request(req));
                (true, true, false) // 暂停消费，等待用户输入
            }
            AgentEvent::TodoUpdate(todos) => {
                // 转发 TODO 更新到 Relay
                if let Some(ref relay) = self.relay_client {
                    let items: Vec<serde_json::Value> = todos
                        .iter()
                        .map(|t| {
                            serde_json::json!({
                                "content": t.content,
                                "status": format!("{:?}", t.status),
                            })
                        })
                        .collect();
                    relay.send_value(serde_json::json!({
                        "type": "todo_update",
                        "items": items,
                    }));
                }
                self.todo_items = todos;
                (true, false, false)
            }
            AgentEvent::StateSnapshot(msgs) => {
                tracing::debug!(count = msgs.len(), "received StateSnapshot in poll_agent");
                for msg in &msgs {
                    match msg {
                        BaseMessage::Ai {
                            content: _,
                            tool_calls,
                        } => {
                            tracing::debug!(
                                has_tc = !tool_calls.is_empty(),
                                tc_len = tool_calls.len(),
                                "ai msg in snapshot"
                            );
                        }
                        BaseMessage::Tool { tool_call_id, .. } => {
                            tracing::debug!(tc_id = %tool_call_id, "tool msg in snapshot");
                        }
                        _ => {}
                    }
                }
                // 增量追加到 agent_state_messages（去重，避免重复消息）
                let start = self.agent_state_messages.len();
                self.agent_state_messages.extend(msgs);

                // 增量持久化到 thread（从上次持久化位置之后的所有消息）
                if let Some(id) = self.current_thread_id.clone() {
                    let new_msgs: Vec<_> = self.agent_state_messages[start..]
                        .iter()
                        .filter(|m| !matches!(m, BaseMessage::System { .. }))
                        .cloned()
                        .collect();
                    if !new_msgs.is_empty() {
                        let store = self.thread_store.clone();
                        let tid = id.clone();
                        tokio::spawn(async move {
                            let _ = store.append_messages(&tid, &new_msgs).await;
                        });
                    }
                }
                (true, false, false)
            }
            AgentEvent::CompactDone(summary) => {
                // 替换 LLM 历史为摘要（以 AI Message 形式写入，保留 system prompt 由 agent 注入）
                self.agent_state_messages = vec![BaseMessage::ai(summary.clone())];

                // 保留最近 10 条显示消息
                let keep_count = 10usize;
                if self.view_messages.len() > keep_count {
                    let tail = self
                        .view_messages
                        .split_off(self.view_messages.len() - keep_count);
                    self.view_messages = tail;
                }

                // 头部插入压缩提示
                let compact_vm = MessageViewModel::system(format!(
                    "📦 上下文已压缩（保留最近 {} 条显示消息，LLM 历史已替换为摘要）",
                    keep_count
                ));
                self.view_messages.insert(0, compact_vm);

                // 尾部追加摘要内容（可见）
                let summary_vm = MessageViewModel::system(format!("📋 压缩摘要：\n{}", summary));
                self.view_messages.push(summary_vm);

                // 通知渲染线程重建显示
                let _ = self
                    .render_tx
                    .send(crate::ui::render_thread::RenderEvent::Clear);
                for vm in &self.view_messages {
                    let _ = self
                        .render_tx
                        .send(crate::ui::render_thread::RenderEvent::AddMessage(
                            vm.clone(),
                        ));
                }

                // 重置持久化计数（view_messages 已重建）
                self.persisted_count = 0;

                self.set_loading(false);
                self.agent_rx = None;

                // 刷新 compact 期间缓冲的消息（与 Done 分支行为一致）
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
            AgentEvent::CompactError(msg) => {
                let vm = MessageViewModel::system(format!("❌ 压缩失败: {}", msg));
                self.view_messages.push(vm.clone());
                let _ = self
                    .render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                self.set_loading(false);
                self.agent_rx = None;

                // 刷新 compact 期间缓冲的消息
                if !self.pending_messages.is_empty() {
                    let combined = self.pending_messages.join("\n\n");
                    self.pending_messages.clear();
                    self.submit_message(combined);
                }

                (true, false, true)
            }
        }
    }

    /// 每帧调用：消费 channel 事件，返回是否有 UI 更新
    pub fn poll_agent(&mut self) -> bool {
        if self.agent_rx.is_none() {
            return false;
        }

        let mut updated = false;

        loop {
            // 先 try_recv 拿到事件（短暂借用 rx），立即释放借用
            let result = self.agent_rx.as_mut().map(|rx| rx.try_recv());
            match result {
                Some(Ok(event)) => {
                    let (ev_updated, should_break, should_return) = self.handle_agent_event(event);
                    if ev_updated {
                        updated = true;
                    }
                    if should_return {
                        return true;
                    }
                    if should_break {
                        break;
                    }
                }
                Some(Err(mpsc::error::TryRecvError::Empty)) | None => break,
                Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                    let vm = MessageViewModel::tool_block(
                        "error".to_string(),
                        "agent-error".to_string(),
                        None,
                        true,
                    );
                    self.view_messages.push(vm.clone());
                    let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
                    self.set_loading(false);
                    self.agent_rx = None;
                    return true;
                }
            }
        }

        updated
    }

    // ─── HITL 操作 ────────────────────────────────────────────────────────────

    /// 上下移动列表光标
    pub fn hitl_move(&mut self, delta: isize) {
        if let Some(p) = self.hitl_prompt.as_mut() {
            p.move_cursor(delta);
        }
    }

    /// 切换当前项批准/拒绝
    pub fn hitl_toggle(&mut self) {
        if let Some(p) = self.hitl_prompt.as_mut() {
            p.toggle_current();
        }
    }

    /// 发送 approval_resolved 到 Relay，通知所有端清除 HITL 弹窗
    fn send_hitl_resolved(&mut self) {
        if let Some(ref relay) = self.relay_client {
            if let Some(ref hitl_prompt) = self.pending_hitl_items {
                relay.send_value(serde_json::json!({
                    "type": "approval_resolved",
                    "items": hitl_prompt
                }));
            }
        }
    }

    /// 全部批准并提交
    pub fn hitl_approve_all(&mut self) {
        if let Some(mut p) = self.hitl_prompt.take() {
            p.approve_all();
            self.pending_hitl_items = Some(
                p.items.iter().map(|item| item.tool_name.clone()).collect()
            );
            self.send_hitl_resolved();
            p.confirm();
        }
    }

    /// 全部拒绝并提交
    pub fn hitl_reject_all(&mut self) {
        if let Some(mut p) = self.hitl_prompt.take() {
            p.reject_all();
            self.pending_hitl_items = Some(
                p.items.iter().map(|item| item.tool_name.clone()).collect()
            );
            self.send_hitl_resolved();
            p.confirm();
        }
    }

    /// 按当前每项选择确认并提交
    pub fn hitl_confirm(&mut self) {
        if let Some(p) = self.hitl_prompt.take() {
            self.pending_hitl_items = Some(
                p.items.iter().map(|item| item.tool_name.clone()).collect()
            );
            self.send_hitl_resolved();
            p.confirm();
        }
    }

    // ─── AskUser 操作 ─────────────────────────────────────────────────────────

    pub fn ask_user_next_tab(&mut self) {
        if let Some(p) = self.ask_user_prompt.as_mut() {
            p.next_tab();
        }
    }

    pub fn ask_user_prev_tab(&mut self) {
        if let Some(p) = self.ask_user_prompt.as_mut() {
            p.prev_tab();
        }
    }

    pub fn ask_user_move(&mut self, delta: isize) {
        if let Some(p) = self.ask_user_prompt.as_mut() {
            p.current().move_option_cursor(delta);
            // 光标跟随滚动
            let cursor_row = p.current().option_cursor.max(0) as u16;
            p.scroll_offset = ensure_cursor_visible(cursor_row, p.scroll_offset, 10);
        }
    }

    pub fn ask_user_toggle(&mut self) {
        if let Some(p) = self.ask_user_prompt.as_mut() {
            p.current().toggle_current();
        }
    }

    pub fn ask_user_push_char(&mut self, c: char) {
        if let Some(p) = self.ask_user_prompt.as_mut() {
            p.current().push_char(c);
        }
    }

    pub fn ask_user_pop_char(&mut self) {
        if let Some(p) = self.ask_user_prompt.as_mut() {
            p.current().pop_char();
        }
    }

    /// Enter：确认当前问题。若全部问题均已确认则提交并关闭弹窗。
    /// 若当前问题没有选中任何选项（且不在自定义输入模式），自动选中光标所在选项。
    pub fn ask_user_confirm(&mut self) {
        let all_done = {
            let p = match self.ask_user_prompt.as_mut() {
                Some(p) => p,
                None => return,
            };
            let q = &mut p.questions[p.active_tab];
            // 没有选中任何选项且不在自定义输入模式：自动选中当前光标行
            if !q.in_custom_input
                && !q.selected.iter().any(|&v| v)
                && q.custom_input.trim().is_empty()
            {
                q.toggle_current();
            }
            p.confirm_current()
        };

        if all_done {
            // 通知所有端清除 AskUser 弹窗
            if let Some(ref relay) = self.relay_client {
                relay.send_value(serde_json::json!({
                    "type": "ask_user_resolved"
                }));
            }
            self.pending_ask_user = None;
            if let Some(p) = self.ask_user_prompt.take() {
                p.confirm();
            }
        }
    }

    // ─── Attachment 操作 ──────────────────────────────────────────────────────

    /// 添加一个图片附件到待发送列表
    pub fn add_pending_attachment(&mut self, att: PendingAttachment) {
        self.pending_attachments.push(att);
    }

    /// 删除最后一个图片附件
    pub fn pop_pending_attachment(&mut self) {
        self.pending_attachments.pop();
    }

    // ─── Thread 操作 ──────────────────────────────────────────────────────────

    /// 恢复历史 thread：加载消息，关闭 browser
    pub fn open_thread(&mut self, thread_id: ThreadId) {
        let store = self.thread_store.clone();
        let tid = thread_id.clone();
        let base_msgs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_messages(&tid))
                .unwrap_or_default()
        });
        self.view_messages.clear();
        self.agent_state_messages = base_msgs.clone();
        // 维护前一条 Ai 消息的 tool_calls，用于 Tool 消息获取工具名和参数
        let mut prev_ai_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
        for msg in &base_msgs {
            // 先收集 Ai 消息的 tool_calls
            if let BaseMessage::Ai { tool_calls, .. } = msg {
                prev_ai_tool_calls = tool_calls
                    .iter()
                    .map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                    .collect();
            }
            let vm = MessageViewModel::from_base_message(&msg, &prev_ai_tool_calls);
            // 跳过空的 AssistantBubble（只有 ToolUse，无可显示内容）
            if let MessageViewModel::AssistantBubble { blocks, .. } = &vm {
                if blocks
                    .iter()
                    .all(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                {
                    continue;
                }
            }
            self.view_messages.push(vm);
        }
        self.persisted_count = self.view_messages.len();
        self.current_thread_id = Some(thread_id);
        self.thread_browser = None;

        // 通知渲染线程加载历史消息
        let _ = self
            .render_tx
            .send(RenderEvent::LoadHistory(self.view_messages.clone()));
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        self.view_messages.clear();
        self.agent_state_messages.clear();
        self.current_thread_id = None;
        self.persisted_count = 0;
        self.todo_items.clear();
        self.pending_attachments.clear();
        self.thread_browser = None;
        let _ = self.render_tx.send(RenderEvent::Clear);
    }

    /// 压缩当前对话上下文：调用 LLM 生成摘要，替换 agent_state_messages
    pub fn start_compact(&mut self, instructions: String) {
        if self.agent_state_messages.is_empty() {
            let vm = MessageViewModel::system("无可压缩的上下文（历史消息为空）".to_string());
            self.view_messages.push(vm.clone());
            let _ = self
                .render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
            return;
        }

        let provider = match self
            .zen_config
            .as_ref()
            .and_then(agent::LlmProvider::from_config)
            .or_else(agent::LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                let vm = MessageViewModel::system(
                    "❌ 压缩失败: 未配置 LLM Provider（请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY）".to_string(),
                );
                self.view_messages.push(vm.clone());
                let _ = self
                    .render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                return;
            }
        };

        let messages = self.agent_state_messages.clone();
        let model = provider.into_model();

        let (tx, rx) = mpsc::channel::<AgentEvent>(8);
        self.agent_rx = Some(rx);
        self.set_loading(true);

        tokio::spawn(async move {
            agent::compact_task(messages, model, instructions, tx).await;
        });
    }

    /// 打开 thread 浏览面板（通过命令触发）
    pub fn open_thread_browser(&mut self) {
        let store = self.thread_store.clone();
        let threads = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.list_threads())
                .unwrap_or_default()
        });
        self.thread_browser = Some(ThreadBrowser::new(threads, self.thread_store.clone()));
    }

    // ─── Model 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /model 面板
    pub fn open_model_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.model_panel = Some(ModelPanel::from_config(cfg));
    }

    /// 关闭 /model 面板（不保存）
    pub fn close_model_panel(&mut self) {
        self.model_panel = None;
    }

    // ─── Agent 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /agents 面板（传入扫描到的 agent 列表）
    pub fn open_agent_panel(&mut self, agents: Vec<AgentItem>) {
        self.agent_panel = Some(AgentPanel::new(agents, self.agent_id.clone()));
    }

    /// 关闭 /agents 面板（不选择任何 agent）
    pub fn close_agent_panel(&mut self) {
        self.agent_panel = None;
    }

    /// 在 agent 面板中上移光标
    pub fn agent_panel_move_up(&mut self) {
        if let Some(panel) = self.agent_panel.as_mut() {
            panel.move_cursor(-1);
            panel.scroll_offset =
                ensure_cursor_visible(panel.cursor as u16, panel.scroll_offset, 10);
        }
    }

    /// 在 agent 面板中下移光标
    pub fn agent_panel_move_down(&mut self) {
        if let Some(panel) = self.agent_panel.as_mut() {
            panel.move_cursor(1);
            panel.scroll_offset =
                ensure_cursor_visible(panel.cursor as u16, panel.scroll_offset, 10);
        }
    }

    /// 确认选择当前 agent，关闭面板，设置 agent_id
    pub fn agent_panel_confirm(&mut self) {
        // 先取出 selection，避免同时借用 panel 和 agent_id
        let (is_none, agent_id, agent_name) = {
            let panel = match self.agent_panel.as_mut() {
                Some(p) => p,
                None => return,
            };
            let (is_none, agent_id) = panel.get_selection();
            let agent_name = if is_none {
                None
            } else {
                agent_id
                    .as_ref()
                    .and_then(|_id| panel.current_agent().map(|a| a.name.clone()))
            };
            (is_none, agent_id, agent_name)
        };

        if is_none {
            self.set_agent_id(None);
            self.view_messages.push(MessageViewModel::system(
                "Agent 已重置（未设置 agent_id）".to_string(),
            ));
        } else if let Some(id) = agent_id {
            self.set_agent_id(Some(id.clone()));
            let name = agent_name.unwrap_or_else(|| id.clone());
            self.view_messages.push(MessageViewModel::system(format!(
                "Agent 已切换为: {} ({})",
                name, id
            )));
        }
        self.agent_panel = None;
    }

    /// 取消选择（不改变当前 agent_id），关闭面板
    #[allow(dead_code)]
    pub fn agent_panel_clear(&mut self) {
        self.agent_panel = None;
    }

    /// 在面板中确认选择当前 provider（Browse 模式下，仅更新 active_id 显示）
    pub fn model_panel_confirm_select(&mut self) {
        let Some(panel) = self.model_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.confirm_select(cfg);
        let _ = crate::config::save(cfg);
    }

    /// 在面板中保存编辑/新建，写回配置
    pub fn model_panel_apply_edit(&mut self) {
        let Some(panel) = self.model_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_edit(cfg);
        let _ = crate::config::save(cfg);
    }

    /// 删除光标处的 provider
    pub fn model_panel_confirm_delete(&mut self) {
        let Some(panel) = self.model_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.confirm_delete(cfg);
        let _ = crate::config::save(cfg);
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

    /// 激活当前 Tab（写入 active_alias），保存配置，更新状态栏
    pub fn model_panel_activate_tab(&mut self) {
        let Some(panel) = self.model_panel.as_ref() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.activate_current_tab(cfg);
        panel.apply_alias_edit(cfg);
        let _ = crate::config::save(cfg);
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.model_panel = None;
    }

    /// 保存当前 Tab 的 provider/model 配置（不改变 active_alias）
    pub fn model_panel_save_alias(&mut self) {
        let Some(panel) = self.model_panel.as_ref() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_alias_edit(cfg);
        let _ = crate::config::save(cfg);
        // 更新状态栏（在 active_alias 对应的别名配置改变时）
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
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

// ─── 测试辅助方法（仅在 cfg(any(test, feature = "headless")) 下编译）──────────

#[cfg(any(test, feature = "headless"))]
impl App {
    /// 向事件队列注入 AgentEvent（测试用）
    pub fn push_agent_event(&mut self, event: AgentEvent) {
        self.agent_event_queue.push(event);
    }

    /// 批量处理队列中所有待处理事件，复用 handle_agent_event 逻辑
    pub fn process_pending_events(&mut self) {
        let events: Vec<AgentEvent> = std::mem::take(&mut self.agent_event_queue);
        for event in events {
            let (_updated, should_break, should_return) = self.handle_agent_event(event);
            if should_return || should_break {
                break;
            }
        }
    }

    /// 构造 Headless 测试用 App，使用 ratatui TestBackend 替代真实终端
    pub fn new_headless(width: u16, height: u16) -> (App, crate::ui::headless::HeadlessHandle) {
        use crate::thread::SqliteThreadStore;
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend should never fail");

        // 启动渲染线程
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(width);

        // 使用唯一临时 SQLite 存储，避免测试并发时文件锁冲突
        let db_name = format!("zen-threads-test-{}.db", uuid::Uuid::now_v7());
        let thread_store: Arc<dyn ThreadStore> = Arc::new(
            SqliteThreadStore::new(std::env::temp_dir().join(db_name))
                .expect("无法创建测试用 SQLite 数据库"),
        );

        let app = App {
            view_messages: Vec::new(),
            textarea: build_textarea(false, 0),
            loading: false,
            scroll_offset: u16::MAX,
            scroll_follow: true,
            cwd: "/tmp".to_string(),
            provider_name: "test".to_string(),
            model_name: "test-model".to_string(),
            agent_rx: None,
            hitl_prompt: None,
            ask_user_prompt: None,
            todo_items: Vec::new(),
            zen_config: None,
            model_panel: None,
            agent_panel: None,
            command_registry: crate::command::default_registry(),
            command_help_list: Vec::new(),
            skills: Vec::new(),
            hint_cursor: None,
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
            render_notify: Arc::clone(&render_notify),
            last_render_version: 0,
            agent_event_queue: Vec::new(),
            pending_messages: Vec::new(),
            pending_attachments: Vec::new(),
            show_tool_messages: false,
            relay_client: None,
            relay_event_rx: None,
            pending_hitl_items: None,
            pending_ask_user: None,
            langfuse_tracer: None,
        };

        let handle = crate::ui::headless::HeadlessHandle {
            terminal,
            render_notify,
        };

        (app, handle)
    }
}
