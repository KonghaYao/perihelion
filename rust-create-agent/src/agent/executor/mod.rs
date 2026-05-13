mod final_answer;
mod llm_step;
mod tool_dispatch;
mod tool_setup;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::agent::events::{AgentEvent, AgentEventHandler, BackgroundTaskResult};
use crate::agent::react::{AgentInput, AgentOutput, ReactLLM, ToolCall, ToolResult};
use crate::agent::state::State;
use crate::error::{AgentError, AgentResult};
use crate::messages::BaseMessage;
use crate::middleware::chain::MiddlewareChain;
use crate::middleware::r#trait::Middleware;
use crate::tools::BaseTool;
use std::collections::HashMap;

pub use tokio_util::sync::CancellationToken as AgentCancellationToken;

#[allow(clippy::type_complexity)]
/// Agent 执行器 - 管理 ReAct 循环
pub struct ReActAgent<L, S>
where
    L: ReactLLM,
    S: State,
{
    pub(crate) llm: L,
    pub(crate) tools: HashMap<String, Box<dyn BaseTool>>,
    pub(crate) chain: MiddlewareChain<S>,
    pub(crate) max_iterations: usize,
    /// 可选事件回调：在工具调用、答案生成等关键节点触发
    pub(crate) event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// 固定系统提示词：在所有中间件 before_agent 执行完毕后 prepend，无顺序约束
    pub(crate) system_prompt: Option<String>,
    /// 上下文窗口预算配置（用于监控 token 用量和触发 compact 建议）
    pub(crate) context_budget: Option<crate::agent::token::ContextBudget>,
    /// 后台任务通知接收端：后台 agent 完成时推送结果
    pub(crate) notification_rx:
        Option<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>>>,
    /// 工具过滤器：返回 true 的工具从 LLM 可见列表中移除（None = 不过滤，向后兼容）
    pub(crate) tool_filter: Option<fn(&str) -> bool>,
    /// 共享工具注册表：包含所有工具（包括 deferred），供 ExecuteExtraTool 代理执行使用
    pub(crate) shared_tools: Option<Arc<parking_lot::RwLock<HashMap<String, Arc<dyn BaseTool>>>>>,
    /// micro_compact 配置（None = 不在循环内自动压缩）
    pub(crate) compact_config: Option<crate::agent::compact::CompactConfig>,
}

impl<L: ReactLLM, S: State> ReActAgent<L, S> {
    pub fn new(llm: L) -> Self {
        Self {
            llm,
            tools: HashMap::new(),
            chain: MiddlewareChain::new(),
            max_iterations: 10,
            event_handler: None,
            system_prompt: None,
            context_budget: None,
            notification_rx: None,
            tool_filter: None,
            shared_tools: None,
            compact_config: None,
        }
    }

    pub fn max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    pub fn register_tool(mut self, tool: Box<dyn BaseTool>) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    pub fn add_middleware(mut self, middleware: Box<dyn Middleware<S>>) -> Self {
        self.chain.add(middleware);
        self
    }

    /// 注入事件回调（链式 builder）
    pub fn with_event_handler(mut self, handler: Arc<dyn AgentEventHandler>) -> Self {
        self.event_handler = Some(handler);
        self
    }

    /// 设置固定系统提示词
    ///
    /// 在所有中间件 `before_agent` 执行完毕之后、LLM 循环开始之前，
    /// 将 system 消息 prepend 到 state 消息列表最前。
    /// 不依赖中间件注册顺序，可在 builder 链任意位置调用。
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// 设置上下文窗口预算配置
    ///
    /// 用于监控 token 用量：当 context 使用率超过 `warning_threshold` 时发出日志警告，
    /// 提示用户使用 `/compact` 压缩上下文。设置为 None 则禁用监控。
    pub fn with_context_budget(mut self, budget: crate::agent::token::ContextBudget) -> Self {
        self.context_budget = Some(budget);
        self
    }

    /// 设置后台任务通知接收端
    ///
    /// 后台 agent 完成时通过此通道推送 `BackgroundTaskResult`，
    /// 主 agent 在 ReAct 循环中消费通知并注入到消息流。
    pub fn with_notification_rx(
        mut self,
        rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
    ) -> Self {
        self.notification_rx = Some(tokio::sync::Mutex::new(rx));
        self
    }

    /// 设置工具过滤器
    ///
    /// 返回 `true` 的工具从 LLM 可见列表中移除（延迟加载），
    /// 返回 `false` 或 `None` 时保留所有工具（向后兼容）。
    pub fn with_tool_filter(mut self, filter: fn(&str) -> bool) -> Self {
        self.tool_filter = Some(filter);
        self
    }

    /// 设置共享工具注册表
    ///
    /// 包含所有工具（包括 deferred tools），供 ExecuteExtraTool 代理执行使用。
    /// executor 在工具收集完成后将所有工具写入此注册表。
    pub fn with_shared_tools(
        mut self,
        tools: Arc<parking_lot::RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
    ) -> Self {
        self.shared_tools = Some(tools);
        self
    }

    /// 设置 micro_compact 配置
    ///
    /// 启用后，ReAct 循环在每次工具调用完成后检查上下文用量，
    /// 超过 warning 阈值时自动执行 micro_compact（压缩旧工具结果）。
    pub fn with_compact_config(mut self, config: crate::agent::compact::CompactConfig) -> Self {
        self.compact_config = Some(config);
        self
    }

    pub fn middleware_names(&self) -> Vec<&str> {
        self.chain.names()
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// 发出事件（无 handler 时静默忽略）
    pub(crate) fn emit(&self, event: AgentEvent) {
        if let Some(h) = &self.event_handler {
            h.on_event(event);
        }
    }

    /// 执行 Agent（ReAct 循环主入口）
    ///
    /// `cancel` 可选；触发后：
    /// - LLM 请求进行中 → 立即返回 `AgentError::Interrupted`
    /// - 工具执行进行中 → 所有未完成工具以 error 结果写入状态，然后返回 `AgentError::Interrupted`
    #[instrument(name = "agent.execute", skip(self, input, state, cancel),
        fields(max_iterations = self.max_iterations))]
    pub async fn execute(
        &self,
        input: AgentInput,
        state: &mut S,
        cancel: Option<CancellationToken>,
    ) -> AgentResult<AgentOutput> {
        // 若未提供 token，创建一个永不触发的占位符，简化后续逻辑
        let cancel = cancel.unwrap_or_default();

        let human_msg = BaseMessage::human(input.content);
        state.add_message(human_msg.clone());
        self.emit(AgentEvent::MessageAdded(human_msg));

        // 消息计数：从用户消息之后开始跟踪（局部变量，避免并发 execute 时的竞态）
        let mut last_message_count: usize = state.messages().len();

        // 从中间件收集工具，手动注册的同名工具优先级最高
        let middleware_tools = self.chain.collect_tools(state.cwd());

        // 将所有工具转为 Arc 并收集
        let tool_arcs: Vec<Arc<dyn BaseTool>> = middleware_tools
            .into_iter()
            .map(self::tool_setup::box_to_arc)
            .collect();

        // 将所有工具写入共享注册表（供 ExecuteExtraTool 代理执行使用）
        if let Some(ref shared) = self.shared_tools {
            let mut map = shared.write();
            for arc in &tool_arcs {
                map.insert(arc.name().to_string(), Arc::clone(arc));
            }
        }

        // 构建引用 map（用于 executor 内部工具查找）
        let mut all_tools: HashMap<String, &dyn BaseTool> = HashMap::new();
        for arc in &tool_arcs {
            all_tools.insert(arc.name().to_string(), arc.as_ref());
        }
        for (name, tool) in &self.tools {
            all_tools.insert(name.clone(), tool.as_ref());
        }

        let mut tool_refs: Vec<&dyn BaseTool> = if let Some(filter) = self.tool_filter {
            all_tools
                .values()
                .copied()
                .filter(|t| !filter(t.name()))
                .collect()
        } else {
            all_tools.values().copied().collect()
        };
        tool_refs.sort_by_key(|t| t.name());

        tracing::debug!(
            total_tools = all_tools.len(),
            middleware_tools = tool_arcs.len(),
            registered_tools = self.tools.len(),
            visible_tools = tool_refs.len(),
            tool_names = ?tool_refs.iter().map(|t| t.name()).collect::<Vec<_>>(),
            has_filter = self.tool_filter.is_some(),
            "agent: final tool set after collect"
        );

        self.chain.run_before_agent(state).await?;

        // 固定 system prompt：在所有中间件 before_agent 之后 prepend，无顺序约束
        if let Some(ref prompt) = self.system_prompt {
            state.prepend_message(BaseMessage::system(prompt.clone()));
        }

        let mut all_tool_calls: Vec<(ToolCall, ToolResult)> = Vec::new();

        for step in 0..self.max_iterations {
            state.set_current_step(step);

            // LLM 推理
            let reasoning =
                self::llm_step::call_llm(self, state, &tool_refs, step, &cancel).await?;

            if reasoning.needs_tool_call() {
                // 工具分发
                let step_calls = self::tool_dispatch::dispatch_tools(
                    self, state, &reasoning, &all_tools, &cancel,
                )
                .await?;
                all_tool_calls.extend(step_calls);

                // StateSnapshot + 通知消费
                self::final_answer::emit_snapshot_and_drain_notifications(
                    self,
                    state,
                    &mut last_message_count,
                )
                .await;

                // micro_compact：工具调用后检查上下文用量，压缩旧工具结果释放空间
                if let Some(ref config) = self.compact_config {
                    if let Some(ref budget) = self.context_budget {
                        if budget.should_warn(state.token_tracker()) {
                            let cleared = crate::agent::compact::micro_compact_enhanced(
                                config,
                                state.messages_mut(),
                            );
                            if cleared > 0 {
                                tracing::info!(
                                    cleared,
                                    "micro-compact: cleared stale tool results in ReAct loop"
                                );
                            }
                        }
                    }
                }
            } else {
                // 最终回答
                let output = self::final_answer::handle_final_answer(
                    self,
                    state,
                    &reasoning,
                    all_tool_calls,
                    last_message_count,
                    step,
                )
                .await?;
                return Ok(output);
            }
        }

        tracing::warn!(
            max_iterations = self.max_iterations,
            tool_call_count = all_tool_calls.len(),
            last_tools = ?all_tool_calls.iter().rev().take(3)
                .map(|(_, r)| r.tool_name.as_str())
                .collect::<Vec<_>>(),
            "ReAct 循环达到最大迭代次数"
        );
        Err(AgentError::MaxIterationsExceeded(self.max_iterations))
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
