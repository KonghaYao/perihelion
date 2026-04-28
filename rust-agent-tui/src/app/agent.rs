use std::sync::Arc;
use tokio::sync::mpsc;

pub(crate) use super::provider::LlmProvider;
use super::interaction_broker::TuiInteractionBroker;
use super::AgentEvent;
use rust_agent_middlewares::prelude::*;
use rust_agent_middlewares::tools::{AskUserTool, TodoItem};
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
    pub tx: mpsc::Sender<AgentEvent>,
    pub cancel: AgentCancellationToken,
    pub agent_id: Option<String>,
    pub langfuse_tracer: Option<Arc<parking_lot::Mutex<crate::langfuse::LangfuseTracer>>>,
    pub thread_store: Arc<dyn rust_create_agent::thread::ThreadStore>,
    pub thread_id: rust_create_agent::thread::ThreadId,
    pub preload_skills: Vec<String>,
    pub config: Arc<crate::config::ZenConfig>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<rust_agent_middlewares::cron::CronScheduler>>>,
    pub permission_mode: Arc<SharedPermissionMode>,
}

pub async fn run_universal_agent(cfg: AgentRunConfig) {
    let AgentRunConfig {
        provider,
        input,
        cwd,
        history,
        tx,
        cancel,
        agent_id,
        langfuse_tracer,
        thread_store,
        thread_id,
        preload_skills,
        config: zen_config,
        cron_scheduler,
        permission_mode,
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

    // 事件回调 → TUI AgentEvent channel（在 model 之前创建，供 RetryableLLM 使用）
    let tx_event = tx.clone();
    let cwd_for_handler = cwd.clone();
    let langfuse_for_handler = langfuse_tracer.clone();
    let provider_name_for_handler = provider_name.clone();
    let handler: Arc<dyn rust_create_agent::agent::events::AgentEventHandler> = Arc::new(FnEventHandler(move |event: ExecutorEvent| {
        // Langfuse hook（在 TUI 事件映射前执行，使用原始 ExecutorEvent）
        if let Some(ref tracer) = langfuse_for_handler {
            let mut t = tracer.lock();
            match &event {
                ExecutorEvent::LlmCallStart { step, messages, tools } =>
                    t.on_llm_start(*step, messages, tools),
                ExecutorEvent::LlmCallEnd { step, model, output, usage } =>
                    t.on_llm_end(*step, model, &provider_name_for_handler, output, usage.as_ref()),
                ExecutorEvent::ToolStart { tool_call_id, name, input, .. } =>
                    t.on_tool_start(tool_call_id, name, input),
                ExecutorEvent::ToolEnd { tool_call_id, is_error, output, .. } =>
                    t.on_tool_end(tool_call_id, output, *is_error),
                // 累积最终回答（避免从 UI 截断视图提取）
                ExecutorEvent::TextChunk { chunk: text, .. } =>
                    t.on_text_chunk(text),
                _ => {}
            }
        }

        // 映射为 TUI AgentEvent
        if let Some(msg) = map_executor_event(event, &cwd_for_handler) {
            let _ = tx_event.try_send(msg);
        }
    }));

    // 不使用 .with_system()，改由 with_system_prompt() 注入到 state，使 Langfuse 可见
    let model = rust_create_agent::llm::RetryableLLM::new(
        BaseModelReactLLM::new(provider.into_model()),
        rust_create_agent::llm::RetryConfig::default(),
    ).with_event_handler(Arc::clone(&handler));

    // Todo channel：TodoMiddleware → TUI
    let (todo_tx, mut todo_rx) = mpsc::channel::<Vec<TodoItem>>(8);
    let tx_todo = tx.clone();
    tokio::spawn(async move {
        while let Some(todos) = todo_rx.recv().await {
            if tx_todo.send(AgentEvent::TodoUpdate(todos)).await.is_err() {
                tracing::warn!("todo forwarding: TUI channel closed, stopping");
                break;
            }
        }
    });

    // 统一人机交互 broker（取代旧的 TuiHitlHandler + TuiAskUserHandler）
    let broker = TuiInteractionBroker::new(tx.clone());

    // HITL 中间件：使用 with_shared_mode 注入共享权限模式
    // 为 Auto 模式创建 LLM 分类器（独立于主 agent 的 BaseModel 实例）
    let auto_classifier: Option<Arc<dyn AutoClassifier>> = Some(Arc::new(
        LlmAutoClassifier::new(Arc::new(tokio::sync::Mutex::new(
            provider_for_factory.clone().into_model(),
        ))),
    ));
    let hitl = HumanInTheLoopMiddleware::with_shared_mode(
        broker.clone() as Arc<dyn rust_create_agent::interaction::UserInteractionBroker>,
        default_requires_approval,
        permission_mode,
        auto_classifier,
    );

    // AskUser 工具
    let ask_user_tool = AskUserTool::new(
        broker as Arc<dyn rust_create_agent::interaction::UserInteractionBroker>,
    );

    // 构建父工具集（供子 agent 继承），来自 Filesystem + Terminal
    let mut parent_tools: Vec<Box<dyn rust_create_agent::tools::BaseTool>> =
        FilesystemMiddleware::build_tools(&cwd);
    parent_tools.extend(TerminalMiddleware::build_tools(&cwd));

    // LLM 工厂：每次为子 agent 创建裸 LLM（不设 system）
    // 系统提示词由 system_builder + with_system_prompt() 注入，使其在 Langfuse 中可见
    // model_alias: None 表示继承父模型；有值时通过 from_config_for_alias 解析
    let provider_clone = provider_for_factory;
    let config_for_factory = zen_config;
    let llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn rust_create_agent::agent::react::ReactLLM + Send + Sync> + Send + Sync> = Arc::new(move |model_alias: Option<&str>| {
        if let Some(alias) = model_alias {
            if let Some(p) = LlmProvider::from_config_for_alias(&config_for_factory, alias) {
                return Box::new(rust_create_agent::llm::RetryableLLM::new(
                    BaseModelReactLLM::new(p.into_model()),
                    rust_create_agent::llm::RetryConfig::default(),
                ));
            }
        }
        Box::new(rust_create_agent::llm::RetryableLLM::new(
            BaseModelReactLLM::new(provider_clone.clone().into_model()),
            rust_create_agent::llm::RetryConfig::default(),
        ))
    });

    // 系统提示构建器：根据 agent overrides 构建包含 tone/proactiveness 的完整系统提示
    let system_builder: Arc<dyn Fn(Option<&rust_agent_middlewares::AgentOverrides>, &str) -> String + Send + Sync> =
        Arc::new(|overrides, cwd| crate::prompt::build_system_prompt(overrides, cwd));

    // SubAgent 中间件
    let subagent = SubAgentMiddleware::new(
        parent_tools,
        Some(Arc::clone(&handler) as Arc<dyn rust_create_agent::agent::events::AgentEventHandler>),
        llm_factory,
    )
    .with_system_builder(system_builder);

    // 构建 ReActAgent
    // FilesystemMiddleware 和 TerminalMiddleware 通过 collect_tools 自动提供工具
    let executor = ReActAgent::new(model)
        .max_iterations(500)
        .with_system_prompt(system_prompt)  // executor 内部固定 prepend，无顺序约束
        .add_middleware(Box::new(AgentsMdMiddleware::new()))
        .add_middleware(Box::new(AgentDefineMiddleware::new()))
        .add_middleware(Box::new(SkillsMiddleware::new()))
        .add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
        .add_middleware(Box::new(FilesystemMiddleware::new()))
        .add_middleware(Box::new(TerminalMiddleware::new()))
        .add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
        .add_middleware(Box::new(rust_agent_middlewares::cron::CronMiddleware::new(
            cron_scheduler.unwrap_or_else(|| Arc::new(parking_lot::Mutex::new(
                rust_agent_middlewares::cron::CronScheduler::new(tokio::sync::mpsc::unbounded_channel().0)
            )))
        )))
        .add_middleware(Box::new(hitl))
        .add_middleware(Box::new(subagent))
        .with_event_handler(Arc::clone(&handler))
        .register_tool(Box::new(ask_user_tool));

    // 捕获 history 长度，用于后续从全量状态中截取本轮新增消息
    let history_len = history.len();
    let mut state = AgentState::with_messages(cwd, history)
        .with_persistence(thread_store, thread_id);
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

/// 将 ExecutorEvent 映射为 TUI AgentEvent；不需转发的内部事件返回 None
fn map_executor_event(event: ExecutorEvent, cwd: &str) -> Option<AgentEvent> {
    Some(match event {
        ExecutorEvent::AiReasoning(text) => AgentEvent::AssistantChunk(text),
        ExecutorEvent::TextChunk { chunk: text, .. } => AgentEvent::AssistantChunk(text),
        // launch_agent ToolStart → SubAgentStart（在通用 ToolStart 分支之前）
        ExecutorEvent::ToolStart { name, input, .. } if name == "launch_agent" => {
            let agent_id = input["agent_id"].as_str().unwrap_or("unknown").to_string();
            let task_preview = input["task"].as_str().unwrap_or("").chars().take(40).collect();
            AgentEvent::SubAgentStart { agent_id, task_preview }
        }
        ExecutorEvent::ToolStart { tool_call_id, name, input, .. } => AgentEvent::ToolStart {
            tool_call_id,
            name: name.clone(),
            display: format_tool_name(&name),
            args: format_tool_args(&name, &input, Some(cwd)).unwrap_or_default(),
            input: input.clone(),
        },
        // launch_agent ToolEnd → SubAgentEnd（在通用 ToolEnd 分支之前）
        ExecutorEvent::ToolEnd { name, output, is_error, .. } if name == "launch_agent" => {
            AgentEvent::SubAgentEnd { result: output, is_error }
        }
        // ask_user 成功：显示用户的回答
        ExecutorEvent::ToolEnd { tool_call_id, name, output, is_error: false, .. } if name == "ask_user" => {
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output: format!("? → {}", truncate(&output, 60)),
                is_error: false,
            }
        }
        // 工具执行出错
        ExecutorEvent::ToolEnd { tool_call_id, name, output, is_error: true, .. } => AgentEvent::ToolEnd {
            tool_call_id,
            name,
            output: format!("✗ {}", truncate(&output, 60)),
            is_error: true,
        },
        // 无需转发的内部事件
        ExecutorEvent::ToolEnd { .. }
        | ExecutorEvent::StepDone { .. }
        | ExecutorEvent::StateSnapshot(_)
        | ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. } => return None,
        ExecutorEvent::LlmCallEnd { usage: Some(usage), model, .. } => {
            AgentEvent::TokenUsageUpdate { usage, model }
        }
        ExecutorEvent::LlmCallEnd { usage: None, .. } => return None,
        // 核心层事件，TUI 层不消费
        ExecutorEvent::ContextWarning { .. } => return None,
        ExecutorEvent::LlmRetrying { attempt, max_attempts, delay_ms, error } => {
            AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error }
        }
    })
}

// ─── 上下文压缩任务 ────────────────────────────────────────────────────────────

/// 独立的上下文压缩异步任务：调用核心层 full_compact + re_inject 三阶段流程
pub async fn compact_task(
    messages: Vec<rust_create_agent::messages::BaseMessage>,
    model: Box<dyn rust_create_agent::llm::BaseModel>,
    instructions: String,
    config: rust_create_agent::agent::compact::CompactConfig,
    cwd: String,
    tx: mpsc::Sender<super::AgentEvent>,
    cancel: AgentCancellationToken,
) {
    use rust_create_agent::agent::compact::{full_compact, re_inject};
    use rust_create_agent::llm::BaseModel;

    tracing::info!(msg_count = messages.len(), "compact_task: 开始 Full Compact");

    // full_compact 调用 LLM，支持取消
    let compact_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            tracing::info!("compact_task: 被用户取消");
            let _ = tx.send(super::AgentEvent::CompactError("已取消".to_string())).await;
            return;
        }
        result = full_compact(&messages, model.as_ref(), &config, &instructions) => {
            match result {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "compact_task: Full Compact 失败");
                    let _ = tx.send(super::AgentEvent::CompactError(e.to_string())).await;
                    return;
                }
            }
        }
    };

    // 取消检查：re_inject 之前
    if cancel.is_cancelled() {
        tracing::info!("compact_task: re_inject 前被取消");
        let _ = tx.send(super::AgentEvent::CompactError("已取消".to_string())).await;
        return;
    }

    tracing::info!(
        summary_len = compact_result.summary.len(),
        messages_used = compact_result.messages_used,
        "compact_task: Full Compact 完成"
    );

    let re_inject_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            tracing::info!("compact_task: re_inject 阶段被取消");
            let _ = tx.send(super::AgentEvent::CompactError("已取消".to_string())).await;
            return;
        }
        result = re_inject(&messages, &config, &cwd) => result,
    };

    tracing::info!(
        files_injected = re_inject_result.files_injected,
        skills_injected = re_inject_result.skills_injected,
        "compact_task: 重新注入完成"
    );

    let summary_text = format!(
        "此会话从之前的对话延续。以下是之前对话的摘要。\n\n{}",
        compact_result.summary
    );

    let re_inject_content = if re_inject_result.messages.is_empty() {
        String::new()
    } else {
        let mut parts = Vec::new();
        for msg in &re_inject_result.messages {
            parts.push(msg.content());
        }
        format!("\n\n---RE_INJECT_SEPARATOR---\n{}", parts.join("\n\n"))
    };

    let combined_summary = format!("{}{}", summary_text, re_inject_content);

    let _ = tx
        .send(super::AgentEvent::CompactDone {
            summary: combined_summary,
            new_thread_id: String::new(),
        })
        .await;
}
