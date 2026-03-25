use std::sync::Arc;
use tokio::sync::mpsc;

use super::hitl::{ApprovalEvent, TuiAskUserHandler, TuiHitlHandler};
pub(crate) use super::provider::LlmProvider;
use super::AgentEvent;
use rust_agent_middlewares::prelude::*;
use rust_agent_middlewares::tools::{AskUserInvoker, AskUserTool, TodoItem};
use rust_create_agent::agent::events::{AgentEvent as ExecutorEvent, FnEventHandler};
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::state::AgentState;
use rust_create_agent::agent::{AgentCancellationToken, ReActAgent};
use rust_create_agent::llm::BaseModelReactLLM;

// ─── 主入口 ───────────────────────────────────────────────────────────────────

/// run_universal_agent 的参数集合（避免超过 clippy 的参数数量限制）
pub struct AgentRunConfig {
    pub provider: LlmProvider,
    pub input: AgentInput,
    pub cwd: String,
    pub history: Vec<rust_create_agent::messages::BaseMessage>,
    pub approval_tx: mpsc::Sender<ApprovalEvent>,
    pub tx: mpsc::Sender<AgentEvent>,
    pub cancel: AgentCancellationToken,
    pub agent_id: Option<String>,
    pub relay_client: Option<Arc<rust_relay_server::client::RelayClient>>,
    pub langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
}

pub async fn run_universal_agent(cfg: AgentRunConfig) {
    let AgentRunConfig {
        provider,
        input,
        cwd,
        history,
        approval_tx,
        tx,
        cancel,
        agent_id,
        relay_client,
        langfuse_tracer,
    } = cfg;
    // 如果设置了 agent_id，提前解析 agent.md 获取可覆盖部分（persona / tone / proactiveness），
    // 替换 system prompt 中对应占位符；安全策略、代码规范等硬约束始终保留。
    // 使用 spawn_blocking 避免同步 I/O 阻塞 tokio 运行时。
    let overrides = if let Some(id) = agent_id.as_deref() {
        let cwd_clone = cwd.clone();
        let id_owned = id.to_string();
        tokio::task::spawn_blocking(move || {
            rust_agent_middlewares::AgentDefineMiddleware::load_overrides(&cwd_clone, &id_owned)
        })
        .await
        .unwrap_or(None)
    } else {
        None
    };
    let system_prompt = crate::prompt::build_system_prompt(overrides.as_ref(), &cwd);
    let provider_for_factory = provider.clone();
    let provider_name = provider.display_name().to_string();
    // 不使用 .with_system()，改由 PrependSystemMiddleware 注入到 state，使 Langfuse 可见
    let model = BaseModelReactLLM::new(provider.into_model());

    // Todo channel：TodoMiddleware → TUI
    let (todo_tx, mut todo_rx) = mpsc::channel::<Vec<TodoItem>>(8);
    let tx_todo = tx.clone();
    tokio::spawn(async move {
        while let Some(todos) = todo_rx.recv().await {
            let _ = tx_todo.send(AgentEvent::TodoUpdate(todos)).await;
        }
    });

    // HITL 中间件
    let hitl = HumanInTheLoopMiddleware::from_env(TuiHitlHandler::new(approval_tx.clone()));

    // AskUser 工具
    let ask_user_invoker: Arc<dyn AskUserInvoker> = TuiAskUserHandler::new(approval_tx);
    let ask_user_tool = AskUserTool::new(ask_user_invoker);

    // 事件回调 → TUI AgentEvent channel + Relay 转发
    let tx_event = tx.clone();
    let cwd_for_handler = cwd.clone();
    let relay_for_handler = relay_client.clone();
    let langfuse_for_handler = langfuse_tracer.clone();
    let provider_name_for_handler = provider_name.clone();
    let handler: Arc<dyn rust_create_agent::agent::events::AgentEventHandler> = Arc::new(FnEventHandler(move |event: ExecutorEvent| {
        // 转发到 Relay
        if let Some(ref relay) = relay_for_handler {
            match &event {
                // BaseMessage 走新的 relay.send_message 路径
                ExecutorEvent::MessageAdded(msg) => relay.send_message(msg),
                // 其他事件走原有路径（兼容性保留）
                _ => relay.send_agent_event(&event),
            }
        }

        // Langfuse hook（在 TUI 事件映射前执行，使用原始 ExecutorEvent）
        if let Some(ref tracer) = langfuse_for_handler {
            let mut t = tracer.lock();
            match &event {
                ExecutorEvent::LlmCallStart { step, messages, tools } =>
                    t.on_llm_start(*step, messages, tools),
                ExecutorEvent::LlmCallEnd { step, model, output, usage } =>
                    t.on_llm_end(*step, model, &provider_name_for_handler, output, usage.as_ref()),
                ExecutorEvent::ToolStart { tool_call_id, name, input } =>
                    t.on_tool_start(tool_call_id, name, input),
                ExecutorEvent::ToolEnd { tool_call_id, is_error, output, .. } =>
                    t.on_tool_end(tool_call_id, output, *is_error),
                // 累积最终回答（避免从 UI 截断视图提取）
                ExecutorEvent::TextChunk(text) =>
                    t.on_text_chunk(text),
                _ => {}
            }
        }

        // 映射为 TUI AgentEvent
        let msg = match event {
            ExecutorEvent::AiReasoning(text) => AgentEvent::AssistantChunk(text),
            ExecutorEvent::TextChunk(text) => AgentEvent::AssistantChunk(text),
            ExecutorEvent::MessageAdded(msg) => AgentEvent::MessageAdded(msg),
            ExecutorEvent::ToolStart { tool_call_id, name, input } => AgentEvent::ToolCall {
                tool_call_id,
                args: format_tool_args(&name, &input, Some(cwd_for_handler.as_str())),
                display: format_tool_name(&name),
                name,
                is_error: false,
            },
            // ask_user 成功：显示用户的回答
            ExecutorEvent::ToolEnd {
                name,
                output,
                is_error: false,
                ..
            } if name == "ask_user" => AgentEvent::ToolCall {
                tool_call_id: String::new(),
                display: "AskUser".to_string(),
                args: Some(format!("? → {}", truncate(&output, 60))),
                name,
                is_error: false,
            },
            // 工具执行出错
            ExecutorEvent::ToolEnd {
                name,
                output,
                is_error: true,
                ..
            } => AgentEvent::ToolCall {
                tool_call_id: String::new(),
                display: format_tool_name(&name),
                args: Some(format!("✗ {}", truncate(&output, 60))),
                name,
                is_error: true,
            },
            // 无需转发的内部事件（含新增的 LLM hook 事件，已在 Langfuse 分支处理）
            ExecutorEvent::ToolEnd { .. }
            | ExecutorEvent::StepDone { .. }
            | ExecutorEvent::StateSnapshot(_)
            | ExecutorEvent::LlmCallStart { .. }
            | ExecutorEvent::LlmCallEnd { .. } => return,
        };
        let _ = tx_event.try_send(msg);
    }));

    // 构建父工具集（供子 agent 继承），来自 Filesystem + Terminal
    let parent_tools: Arc<Vec<Arc<dyn rust_create_agent::tools::BaseTool>>> = {
        use rust_create_agent::tools::ToolProvider;
        let fs_tools = FilesystemMiddleware::new().tools(&cwd);
        let term_tools = TerminalMiddleware::new().tools(&cwd);
        let tools = fs_tools
            .into_iter()
            .chain(term_tools)
            .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn rust_create_agent::tools::BaseTool>)
            .collect();
        Arc::new(tools)
    };

    // LLM 工厂：每次为子 agent 创建裸 LLM（不设 system）
    // 系统提示词由 system_builder + PrependSystemMiddleware 注入，使其在 Langfuse 中可见
    let provider_clone = provider_for_factory;
    let llm_factory: Arc<dyn Fn() -> Box<dyn rust_create_agent::agent::react::ReactLLM + Send + Sync> + Send + Sync> = Arc::new(move || {
        Box::new(BaseModelReactLLM::new(provider_clone.clone().into_model()))
    });

    // 系统提示构建器：根据 agent overrides 构建包含 tone/proactiveness 的完整系统提示
    let system_builder: Arc<dyn Fn(Option<&rust_agent_middlewares::AgentOverrides>, &str) -> String + Send + Sync> =
        Arc::new(|overrides, cwd| crate::prompt::build_system_prompt(overrides, cwd));

    // SubAgent 中间件
    let subagent = SubAgentMiddleware::new(
        Arc::clone(&parent_tools),
        Some(Arc::clone(&handler) as Arc<dyn rust_create_agent::agent::events::AgentEventHandler>),
        llm_factory,
    )
    .with_system_builder(system_builder);

    // 构建 ReActAgent
    // FilesystemMiddleware 和 TerminalMiddleware 通过 collect_tools 自动提供工具
    let executor = ReActAgent::new(model)
        .max_iterations(500)
        .add_middleware(Box::new(AgentsMdMiddleware::new()))
        .add_middleware(Box::new(AgentDefineMiddleware::new()))
        .add_middleware(Box::new(SkillsMiddleware::new()))
        .add_middleware(Box::new(FilesystemMiddleware::new()))
        .add_middleware(Box::new(TerminalMiddleware::new()))
        .add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
        .add_middleware(Box::new(hitl))
        .add_middleware(Box::new(subagent))
        // 最后注册 → before_agent 最后执行 → prepend_message 最后写入 → 位于消息列表最前
        .add_middleware(Box::new(rust_agent_middlewares::PrependSystemMiddleware::new(system_prompt)))
        .with_event_handler(Arc::clone(&handler))
        .register_tool(Box::new(ask_user_tool));

    // 捕获 history 长度，用于后续从全量状态中截取本轮新增消息
    let history_len = history.len();
    let mut state = AgentState::with_messages(cwd, history);
    if let Some(id) = agent_id {
        state = state.with_context("agent_id", id);
    }
    let agent_input = input;

    let result = executor
        .execute(agent_input, &mut state, Some(cancel))
        .await;

    // 无论成功/中断/失败，只把本轮新增消息（非 System、跳过 history）发回 App。
    // 避免将 history 重复追加到 agent_state_messages 并在 DB 产生重复写入。
    let new_msgs: Vec<_> = state
        .into_messages()
        .into_iter()
        .filter(|m| !matches!(m, rust_create_agent::messages::BaseMessage::System { .. }))
        .skip(history_len)
        .collect();
    let _ = tx.send(AgentEvent::StateSnapshot(new_msgs)).await;

    match result {
        Ok(_) => {
            let _ = tx.send(AgentEvent::Done).await;
        }
        Err(rust_create_agent::error::AgentError::Interrupted) => {
            let _ = tx.send(AgentEvent::Interrupted).await;
            let _ = tx.send(AgentEvent::Done).await;
        }
        Err(e) => {
            let _ = tx.send(AgentEvent::Error(e.to_string())).await;
            let _ = tx.send(AgentEvent::Done).await;
        }
    }
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

use super::tool_display::{format_tool_args, format_tool_name, truncate};

// ─── 上下文压缩任务 ────────────────────────────────────────────────────────────

/// 独立的上下文压缩异步任务：单次 LLM 调用，生成摘要后通过 channel 发回 App
pub async fn compact_task(
    messages: Vec<rust_create_agent::messages::BaseMessage>,
    model: Box<dyn rust_create_agent::llm::BaseModel>,
    instructions: String,
    tx: mpsc::Sender<super::AgentEvent>,
) {
    use rust_create_agent::llm::types::LlmRequest;
    use rust_create_agent::messages::BaseMessage;

    // ── 1. 格式化消息历史 ──────────────────────────────────────────────────────

    fn truncate_content(s: &str, max: usize) -> String {
        if s.chars().count() > max {
            let end: String = s.chars().take(max).collect();
            format!("{}...(已截断)", end)
        } else {
            s.to_string()
        }
    }

    let mut lines = Vec::new();
    for msg in &messages {
        match msg {
            BaseMessage::System { .. } => {
                // 跳过系统消息，避免将之前的摘要再次嵌入
            }
            BaseMessage::Human { .. } => {
                let content = truncate_content(&msg.content(), 500);
                lines.push(format!("[用户] {}", content));
            }
            BaseMessage::Ai { tool_calls, .. } => {
                let text = msg.content();
                let tool_names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                let line = if tool_names.is_empty() {
                    format!("[助手] {}", truncate_content(&text, 500))
                } else {
                    format!(
                        "[助手] {}（调用了工具: {}）",
                        truncate_content(&text, 300),
                        tool_names.join(", ")
                    )
                };
                lines.push(line);
            }
            BaseMessage::Tool { tool_call_id, .. } => {
                let content = truncate_content(&msg.content(), 500);
                lines.push(format!("[工具结果:{}] {}", tool_call_id, content));
            }
        }
    }

    if lines.is_empty() {
        let fallback = "## 目标\n（无有效对话历史）\n\n## 已完成操作\n无\n\n## 关键发现\n无".to_string();
        let _ = tx.send(super::AgentEvent::CompactDone(fallback)).await;
        return;
    }

    let conversation_text = lines.join("\n");

    // ── 2. 构造 LLM 请求 ───────────────────────────────────────────────────────

    let system_prompt = "\
你是一个对话上下文压缩工具。将以下对话历史压缩为一份结构化摘要，要求：\n\
1. 保留用户的核心目标和意图\n\
2. 记录已完成的关键操作（文件读写、命令执行结果等）\n\
3. 记录发现的重要信息（文件路径、错误信息、代码结构等）\n\
4. 保留对话中的重要决策和约束\n\
5. 格式：Markdown，分 ## 目标、## 已完成操作、## 关键发现 三个小节\n\
6. 语言：中文\n\
7. 尽量简洁，控制在 500 字以内";

    let mut user_content = format!(
        "以下是需要压缩的对话历史：\n<conversation>\n{}\n</conversation>",
        conversation_text
    );

    if !instructions.trim().is_empty() {
        user_content.push_str(&format!("\n\n压缩时请特别注意：{}", instructions.trim()));
    }

    let request = LlmRequest::new(vec![BaseMessage::human(user_content)])
        .with_system(system_prompt.to_string());

    // ── 3. 调用 LLM ───────────────────────────────────────────────────────────

    match model.invoke(request).await {
        Ok(response) => {
            let summary = response.message.content();
            let _ = tx.send(super::AgentEvent::CompactDone(summary)).await;
        }
        Err(e) => {
            let _ = tx.send(super::AgentEvent::CompactError(e.to_string())).await;
        }
    }
}
