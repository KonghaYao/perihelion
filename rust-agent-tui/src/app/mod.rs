pub mod agent;
pub mod hitl;
pub mod model_panel;
mod provider;

use ratatui::style::{Color, Style};
use ratatui_textarea::TextArea;
use rust_agent_middlewares::ask_user::{AskUserBatchRequest, AskUserQuestionData};
use rust_agent_middlewares::prelude::{
    BatchItem, HitlDecision, SkillMetadata, TodoItem, TodoStatus,
};
use rust_create_agent::messages::BaseMessage;
use tokio::sync::mpsc;

use crate::command::CommandRegistry;
use crate::config::ZenConfig;
use crate::thread::{FilesystemThreadStore, ThreadBrowser, ThreadId, ThreadMeta, ThreadStore};
use agent::LlmProvider;
pub use hitl::{ApprovalEvent, BatchApprovalRequest};
pub use model_panel::ModelPanel;
use std::sync::Arc;
use tracing::Instrument;

// ─── ChatMessage ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub inner: BaseMessage,
    pub display_name: Option<String>,
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            inner: BaseMessage::human(content.into()),
            display_name: None,
            tool_name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            inner: BaseMessage::ai(content.into()),
            display_name: None,
            tool_name: None,
        }
    }

    pub fn tool(
        raw_name: impl Into<String>,
        display: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        let raw_name = raw_name.into();
        let display = display.into();
        let content = content.into();
        let msg = if is_error {
            BaseMessage::tool_error(&raw_name, content.as_str())
        } else {
            BaseMessage::tool_result(&raw_name, content.as_str())
        };
        Self {
            inner: msg,
            display_name: Some(display),
            tool_name: Some(raw_name),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            inner: BaseMessage::system(content.into()),
            display_name: None,
            tool_name: None,
        }
    }

    pub fn todo_status(content: impl Into<String>) -> Self {
        Self {
            inner: BaseMessage::system(content.into()),
            display_name: None,
            tool_name: Some("__todo_status__".to_string()),
        }
    }

    pub fn content(&self) -> String {
        self.inner.content()
    }

    pub fn is_assistant(&self) -> bool {
        matches!(self.inner, BaseMessage::Ai { .. })
    }

    pub fn push_str(&mut self, chunk: &str) {
        if let BaseMessage::Ai { content, .. } = &mut self.inner {
            match content {
                rust_create_agent::messages::MessageContent::Text(s) => s.push_str(chunk),
                _ => {
                    let mut s = content.text_content();
                    s.push_str(chunk);
                    *content = rust_create_agent::messages::MessageContent::Text(s);
                }
            }
        }
    }
}

// ─── AgentEvent ───────────────────────────────────────────────────────────────

pub enum AgentEvent {
    ToolCall {
        name: String,
        display: String,
        is_error: bool,
    },
    AssistantChunk(String),
    Done,
    Error(String),
    /// HITL 批量审批请求
    ApprovalNeeded(BatchApprovalRequest),
    /// AskUser 批量提问请求
    AskUserBatch(AskUserBatchRequest),
    /// Todo 列表更新
    TodoUpdate(Vec<TodoItem>),
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
    pub messages: Vec<ChatMessage>,
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
    /// 当前等待用户输入的 AskUser 批量弹窗
    pub ask_user_prompt: Option<AskUserBatchPrompt>,
    /// 消息列表中 todo 状态消息的下标（用于替换更新而非追加）
    pub todo_message_index: Option<usize>,
    /// 内存中的配置快照（来自 ~/.zen-code/settings.json）
    pub zen_config: Option<ZenConfig>,
    /// /model 面板状态
    pub model_panel: Option<ModelPanel>,
    /// 命令注册表
    pub command_registry: CommandRegistry,
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
    /// 当前 Agent 任务开始时间（用于计算运行时长）
    task_start_time: Option<std::time::Instant>,
    /// 上一次任务的总运行时长（任务结束后保留显示）
    last_task_duration: Option<std::time::Duration>,
}

impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let textarea = build_textarea(false);

        // 优先从 ~/.zen-code/settings.json 加载配置，失败时 fallback 到环境变量
        let zen_config = crate::config::load().ok();

        let provider_from_config = zen_config.as_ref().and_then(LlmProvider::from_config);
        let (provider_name, model_name, status_msg) =
            match provider_from_config.or_else(LlmProvider::from_env) {
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
            Arc::new(FilesystemThreadStore::default_path().unwrap_or_else(|_| {
                FilesystemThreadStore::new(std::env::temp_dir().join("zen-threads"))
            }));

        let mut app = Self {
            messages: Vec::new(),
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
            todo_message_index: None,
            zen_config,
            model_panel: None,
            command_registry: crate::command::default_registry(),
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
            task_start_time: None,
            last_task_duration: None,
        };

        app.messages.push(ChatMessage::system(format!(
            "Rust Agent TUI 已启动 | {} | 工作目录: {} | 工具: read_file, write_file, glob_files, search_files_rg, bash",
            status_msg, cwd
        )));

        app
    }

    /// 把自上次持久化之后的新消息追加到 thread
    fn persist_pending_messages(&mut self) {
        let Some(id) = self.current_thread_id.clone() else {
            return;
        };
        let new_msgs: Vec<BaseMessage> = self.messages[self.persisted_count..]
            .iter()
            // 跳过纯 UI 用途的 todo_status 和 system 消息
            .filter(|m| !matches!(m.inner, BaseMessage::System { .. }))
            .filter(|m| m.tool_name.as_deref() != Some("__todo_status__"))
            .map(|m| {
                // Tool 消息：将 display_name 写入 content（content 运行时为空），
                // 加载时可从 content 还原 display_name，从 tool_call_id 还原 tool_name。
                if let BaseMessage::Tool { ref tool_call_id, ref content, is_error } = m.inner {
                    if content.text_content().is_empty() {
                        let display = m.display_name.as_deref().unwrap_or(tool_call_id);
                        return if is_error {
                            BaseMessage::tool_error(tool_call_id, display)
                        } else {
                            BaseMessage::tool_result(tool_call_id, display)
                        };
                    }
                }
                m.inner.clone()
            })
            .collect();
        let new_count = self.messages.len();
        if new_msgs.is_empty() {
            self.persisted_count = new_count;
            return;
        }
        let store = self.thread_store.clone();
        tokio::spawn(async move {
            let _ = store.append_messages(&id, &new_msgs).await;
        });
        self.persisted_count = new_count;
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
                self.textarea = build_textarea(false);
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
                self.textarea = build_textarea(false);
                self.textarea.insert_str(&format!("#{} ", skill.name));
                self.hint_cursor = None;
            }
        }
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
        self.textarea = build_textarea(loading);
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

        self.messages.push(ChatMessage::user(input.clone()));
        self.set_loading(true);
        self.scroll_offset = u16::MAX;
        self.scroll_follow = true;
        self.todo_message_index = None;
        
        // 开始计时新任务
        self.task_start_time = Some(std::time::Instant::now());
        self.last_task_duration = None;

        let provider = match self
            .zen_config
            .as_ref()
            .and_then(LlmProvider::from_config)
            .or_else(LlmProvider::from_env)
        {
            Some(p) => p,
            None => {
                self.messages.push(ChatMessage::tool(
                    "error", "config-error",
                    "请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY 环境变量后重启，或输入 /model 配置 provider",
                    true,
                ));
                self.set_loading(false);
                return;
            }
        };

        let (tx, rx) = mpsc::channel(32);
        self.agent_rx = Some(rx);

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

        // 确保当前 thread 存在，持久化用户消息
        let thread_id = self.ensure_thread_id();
        let user_msg = BaseMessage::human(input.clone());
        let store = self.thread_store.clone();
        let tid = thread_id.clone();
        tokio::spawn(async move {
            let _ = store.append_messages(&tid, &[user_msg]).await;
        });
        // 用户消息已追加到 self.messages，更新已持久化计数
        self.persisted_count = self.messages.len();

        let span = tracing::info_span!(
            "thread.run",
            thread.id = %thread_id,
            thread.cwd = %cwd,
        );
        tokio::spawn(
            async move {
                agent::run_universal_agent(
                    provider,
                    input,
                    cwd,
                    system_prompt,
                    thread_id,
                    approval_tx,
                    tx,
                )
                .await;
            }
            .instrument(span),
        );
    }

    /// 每帧调用：消费 channel 事件，返回是否有 UI 更新
    pub fn poll_agent(&mut self) -> bool {
        let Some(rx) = self.agent_rx.as_mut() else {
            return false;
        };

        let mut updated = false;

        loop {
            match rx.try_recv() {
                Ok(AgentEvent::ToolCall {
                    name,
                    display,
                    is_error,
                }) => {
                    self.messages
                        .push(ChatMessage::tool(name, display, "", is_error));
                    updated = true;
                }
                Ok(AgentEvent::AssistantChunk(chunk)) => {
                    match self.messages.last_mut() {
                        Some(m) if m.is_assistant() => m.push_str(&chunk),
                        _ => self.messages.push(ChatMessage::assistant(chunk)),
                    }
                    updated = true;
                }
                Ok(AgentEvent::Done) => {
                    self.set_loading(false);
                    self.agent_rx = None;
                    // 记录任务运行时长
                    if let Some(start) = self.task_start_time {
                        self.last_task_duration = Some(start.elapsed());
                    }
                    // 持久化本轮所有 AI/Tool 消息（Done 时批量追加）
                    self.persist_pending_messages();
                    return true;
                }
                Ok(AgentEvent::Error(e)) => {
                    self.messages
                        .push(ChatMessage::tool("error", "agent-error", e, true));
                    self.set_loading(false);
                    self.agent_rx = None;
                    // 记录任务运行时长
                    if let Some(start) = self.task_start_time {
                        self.last_task_duration = Some(start.elapsed());
                    }
                    self.persist_pending_messages();
                    return true;
                }
                Ok(AgentEvent::ApprovalNeeded(req)) => {
                    self.hitl_prompt = Some(HitlBatchPrompt::new(req.items, req.response_tx));
                    updated = true;
                    break; // 暂停消费，等待用户确认
                }
                Ok(AgentEvent::AskUserBatch(req)) => {
                    self.ask_user_prompt = Some(AskUserBatchPrompt::from_request(req));
                    updated = true;
                    break; // 暂停消费，等待用户输入
                }
                Ok(AgentEvent::TodoUpdate(todos)) => {
                    let rendered = render_todos(&todos);
                    match self.todo_message_index {
                        Some(idx) if idx < self.messages.len() => {
                            self.messages[idx] = ChatMessage::todo_status(rendered);
                        }
                        _ => {
                            self.messages.push(ChatMessage::todo_status(rendered));
                            self.todo_message_index = Some(self.messages.len() - 1);
                        }
                    }
                    updated = true;
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.messages.push(ChatMessage::tool(
                        "error",
                        "agent-error",
                        "Agent 任务意外终止",
                        true,
                    ));
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

    /// 全部批准并提交
    pub fn hitl_approve_all(&mut self) {
        if let Some(mut p) = self.hitl_prompt.take() {
            p.approve_all();
            p.confirm();
        }
    }

    /// 全部拒绝并提交
    pub fn hitl_reject_all(&mut self) {
        if let Some(mut p) = self.hitl_prompt.take() {
            p.reject_all();
            p.confirm();
        }
    }

    /// 按当前每项选择确认并提交
    pub fn hitl_confirm(&mut self) {
        if let Some(p) = self.hitl_prompt.take() {
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
            if let Some(p) = self.ask_user_prompt.take() {
                p.confirm();
            }
        }
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
        self.messages.clear();
        for msg in base_msgs {
            // Tool 消息：tool_call_id = 工具名，content = display_name（持久化时写入）
            let (tool_name, display_name) =
                if let BaseMessage::Tool { ref tool_call_id, ref content, .. } = msg {
                    let name = tool_call_id.clone();
                    let text = content.text_content();
                    let display = if text.is_empty() { name.clone() } else { text };
                    (Some(name), Some(display))
                } else {
                    (None, None)
                };
            self.messages.push(ChatMessage {
                inner: msg,
                display_name,
                tool_name,
            });
        }
        self.persisted_count = self.messages.len();
        self.current_thread_id = Some(thread_id);
        self.thread_browser = None;
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        self.messages.clear();
        self.current_thread_id = None;
        self.persisted_count = 0;
        self.thread_browser = None;
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

    /// 在面板中确认选择当前 provider，保存配置，更新 provider_name/model_name
    pub fn model_panel_confirm_select(&mut self) {
        let Some(panel) = self.model_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.confirm_select(cfg);
        let _ = crate::config::save(cfg);
        if let Some(p) = LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.model_panel = None;
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
        if let Some(p) = LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }
}

/// 将 todo 列表渲染为可读文本
pub fn render_todos(todos: &[TodoItem]) -> String {
    let mut lines = vec![format!("📋 Todo ({})", todos.len())];
    for item in todos {
        let icon = match item.status {
            TodoStatus::Completed => "✓",
            TodoStatus::InProgress => "→",
            TodoStatus::Pending => "○",
        };
        lines.push(format!("  {} {}", icon, item.content));
    }
    lines.join("\n")
}

pub fn build_textarea(disabled: bool) -> TextArea<'static> {
    let mut ta = TextArea::default();

    // Loading 状态：黄色边框 + "处理中…" 标题
    // 空闲状态：青色边框 + "输入" 标题
    let (border_color, title_text, title_color) = if disabled {
        (Color::Yellow, " 处理中… ", Color::Yellow)
    } else {
        (Color::Cyan, " 输入 ", Color::Cyan)
    };

    let text_color = if disabled {
        Color::DarkGray
    } else {
        Color::White
    };

    ta.set_cursor_line_style(Style::default());
    ta.set_style(Style::default().fg(text_color));
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
