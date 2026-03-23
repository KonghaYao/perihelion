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

pub async fn run_universal_agent(
    provider: LlmProvider,
    input: String,
    cwd: String,
    _system_prompt: String,
    _thread_id: String,
    history: Vec<rust_create_agent::messages::BaseMessage>,
    approval_tx: mpsc::Sender<ApprovalEvent>,
    tx: mpsc::Sender<AgentEvent>,
    cancel: AgentCancellationToken,
    agent_id: Option<String>,
    relay_client: Option<Arc<rust_relay_server::client::RelayClient>>,
) {
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
    let model = BaseModelReactLLM::new(provider.into_model()).with_system(system_prompt);

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
    let handler: Arc<dyn rust_create_agent::agent::events::AgentEventHandler> = Arc::new(FnEventHandler(move |event: ExecutorEvent| {
        // 转发到 Relay（如果已连接）
        if let Some(ref relay) = relay_for_handler {
            relay.send_agent_event(&event);
        }
        let msg = match event {
            ExecutorEvent::TextChunk(text) => AgentEvent::AssistantChunk(text),
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
            } => AgentEvent::ToolCall {
                tool_call_id: String::new(),
                display: format_tool_name(&name),
                args: Some(format!("✗ {}", truncate(&output, 60))),
                name,
                is_error: true,
            },
            ExecutorEvent::ToolEnd { .. } | ExecutorEvent::StepDone { .. } => return,
            // StateSnapshot 由 TUI poll_agent 通过 run_universal_agent 的返回值直接获取
            ExecutorEvent::StateSnapshot(_) => return,
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

    // LLM 工厂：每次为子 agent 创建独立实例
    let provider_clone = provider_for_factory;
    let cwd_clone = cwd.clone();
    let llm_factory: Arc<dyn Fn() -> Box<dyn rust_create_agent::agent::react::ReactLLM + Send + Sync> + Send + Sync> = Arc::new(move || {
        let overrides = rust_agent_middlewares::AgentDefineMiddleware::load_overrides(&cwd_clone, "");
        let system = crate::prompt::build_system_prompt(overrides.as_ref(), &cwd_clone);
        Box::new(BaseModelReactLLM::new(provider_clone.clone().into_model()).with_system(system))
    });

    // SubAgent 中间件
    let subagent = SubAgentMiddleware::new(
        Arc::clone(&parent_tools),
        Some(Arc::clone(&handler) as Arc<dyn rust_create_agent::agent::events::AgentEventHandler>),
        llm_factory,
    );

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
        .with_event_handler(Arc::clone(&handler))
        .register_tool(Box::new(ask_user_tool));

    let mut state = AgentState::with_messages(cwd, history);
    if let Some(id) = agent_id {
        state = state.with_context("agent_id", id);
    }
    let agent_input = AgentInput::text(input);

    let result = executor
        .execute(agent_input, &mut state, Some(cancel))
        .await;

    // 无论成功/中断/失败，先把最新的消息历史快照发回 App
    let _ = tx
        .send(AgentEvent::StateSnapshot(state.into_messages()))
        .await;

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
