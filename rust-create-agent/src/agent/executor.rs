use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::agent::events::{AgentEvent, AgentEventHandler};
use crate::agent::react::{AgentInput, AgentOutput, ReactLLM, ToolCall, ToolResult};
use crate::agent::state::State;
use crate::agent::token::ContextBudget;
use crate::error::{AgentError, AgentResult};
use crate::messages::{BaseMessage, ToolCallRequest};
use crate::middleware::chain::MiddlewareChain;
use crate::middleware::r#trait::Middleware;
use crate::tools::{BaseTool, ToolProvider};
use std::collections::HashMap;

pub use tokio_util::sync::CancellationToken as AgentCancellationToken;

/// Agent 执行器 - 管理 ReAct 循环
pub struct ReActAgent<L, S>
where
    L: ReactLLM,
    S: State,
{
    llm: L,
    tools: HashMap<String, Box<dyn BaseTool>>,
    tool_providers: Vec<Box<dyn ToolProvider>>,
    chain: MiddlewareChain<S>,
    max_iterations: usize,
    /// 可选事件回调：在工具调用、答案生成等关键节点触发
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// 固定系统提示词：在所有中间件 before_agent 执行完毕后 prepend，无顺序约束
    system_prompt: Option<String>,
    /// 上下文窗口预算配置（用于监控 token 用量和触发 compact 建议）
    context_budget: Option<ContextBudget>,
}

impl<L: ReactLLM, S: State> ReActAgent<L, S> {
    pub fn new(llm: L) -> Self {
        Self {
            llm,
            tools: HashMap::new(),
            tool_providers: Vec::new(),
            chain: MiddlewareChain::new(),
            max_iterations: 10,
            event_handler: None,
            system_prompt: None,
            context_budget: None,
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

    /// 注册工具提供者（独立于中间件，专注于工具供给）
    pub fn add_tool_provider(mut self, provider: Box<dyn ToolProvider>) -> Self {
        self.tool_providers.push(provider);
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
    pub fn with_context_budget(mut self, budget: ContextBudget) -> Self {
        self.context_budget = Some(budget);
        self
    }

    pub fn middleware_names(&self) -> Vec<&str> {
        self.chain.names()
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// 发出事件（无 handler 时静默忽略）
    fn emit(&self, event: AgentEvent) {
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

        // 从 ToolProvider 和中间件各收集工具，手动注册的同名工具优先级最高
        let provider_tools: Vec<Box<dyn BaseTool>> = self
            .tool_providers
            .iter()
            .flat_map(|p| p.tools(state.cwd()))
            .collect();
        let middleware_tools = self.chain.collect_tools(state.cwd());
        let mut all_tools: HashMap<String, &dyn BaseTool> = provider_tools
            .iter()
            .chain(middleware_tools.iter())
            .map(|t| (t.name().to_string(), t.as_ref()))
            .collect();
        for (name, tool) in &self.tools {
            all_tools.insert(name.clone(), tool.as_ref());
        }

        let tool_refs: Vec<&dyn BaseTool> = all_tools.values().copied().collect();

        self.chain.run_before_agent(state).await?;

        // 固定 system prompt：在所有中间件 before_agent 之后 prepend，无顺序约束
        if let Some(ref prompt) = self.system_prompt {
            state.prepend_message(BaseMessage::system(prompt.clone()));
        }

        let mut all_tool_calls: Vec<(ToolCall, ToolResult)> = Vec::new();

        for step in 0..self.max_iterations {
            state.set_current_step(step);

            // ── LLM 推理（与 cancel 竞争）────────────────────────────────────
            self.emit(AgentEvent::LlmCallStart {
                step,
                messages: state.messages().to_vec(),
                tools: tool_refs.iter().map(|t| t.definition()).collect(),
            });
            let reasoning = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    return Err(AgentError::Interrupted);
                }
                result = self.llm.generate_reasoning(state.messages(), &tool_refs) => {
                    match result {
                        Ok(r) => r,
                        Err(e) => {
                                tracing::error!(
                                    step,
                                    model = %self.llm.model_name(),
                                    error = %e,
                                    "LLM generate_reasoning 失败"
                                );
                                // LLM 报错时仍 emit LlmCallEnd，确保 Langfuse generation 可观测
                            self.emit(AgentEvent::LlmCallEnd {
                                step,
                                model: self.llm.model_name(),
                                output: format!("ERROR: {}", e),
                                usage: None,
                            });
                            self.chain.run_on_error(state, &e).await?;
                            return Err(e);
                        }
                    }
                }
            };
            {
                let llm_output = reasoning
                    .final_answer
                    .as_deref()
                    .unwrap_or(&reasoning.thought)
                    .to_string();
                self.emit(AgentEvent::LlmCallEnd {
                    step,
                    model: self.llm.model_name(),
                    output: llm_output,
                    usage: reasoning.usage.clone(),
                });
                // 自动累积 token 用量到 state
                if let Some(ref usage) = reasoning.usage {
                    state.token_tracker_mut().accumulate(usage);
                    // 使用 ContextBudget（若已设置）进行上下文用量监控
                    if let Some(ref budget) = self.context_budget {
                        let tracker = state.token_tracker();
                        if let Some(pct_used) = tracker.estimated_context_tokens() {
                            if budget.should_warn(tracker) {
                                tracing::warn!(
                                    used_tokens = ?tracker.estimated_context_tokens(),
                                    total_tokens = budget.context_window,
                                    percentage = tracker.context_usage_percent(budget.context_window).map(|p| p as u32).unwrap_or(0),
                                    model = %self.llm.model_name(),
                                    step,
                                    "context 接近上限"
                                );
                            }
                            if budget.should_warn(tracker) || budget.should_auto_compact(tracker) {
                                if let Some(percentage) =
                                    tracker.context_usage_percent(budget.context_window)
                                {
                                    self.emit(AgentEvent::ContextWarning {
                                        used_tokens: pct_used,
                                        total_tokens: budget.context_window as u64,
                                        percentage,
                                    });
                                }
                            }
                        }
                    } else {
                        // Fallback: 无 ContextBudget 时使用硬编码 80% 阈值（向后兼容）
                        let tracker = state.token_tracker();
                        let total = self.llm.context_window();
                        if let Some(used) = tracker.estimated_context_tokens() {
                            let pct = used as f64 / total as f64 * 100.0;
                            if pct as u32 >= 80 {
                                tracing::warn!(
                                    used_tokens = used,
                                    total_tokens = total,
                                    percentage = pct as u32,
                                    model = %self.llm.model_name(),
                                    step,
                                    "context 接近上限"
                                );
                            }
                            if pct as u32 >= 80 {
                                self.emit(AgentEvent::ContextWarning {
                                    used_tokens: used,
                                    total_tokens: total as u64,
                                    percentage: pct,
                                });
                            }
                        }
                    }
                }
            }

            if reasoning.needs_tool_call() {
                let tc_reqs: Vec<ToolCallRequest> = reasoning
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone())
                    })
                    .collect();
                // 优先使用带 Reasoning block 的原始消息，保留 thinking 内容
                // source_message 的 tool_calls 字段在 LLM 解析阶段已填好
                let ai_msg = reasoning.source_message.clone().unwrap_or_else(|| {
                    BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs)
                });
                let ai_msg_id = ai_msg.id(); // 捕获 message_id（Copy，供后续 ToolStart/ToolEnd 使用）
                let ai_msg_clone = ai_msg.clone();
                state.add_message(ai_msg);
                self.emit(AgentEvent::MessageAdded(ai_msg_clone));
                // emit AI 推理内容
                self.emit(AgentEvent::AiReasoning(reasoning.thought.clone()));

                // 阶段一：批量 before_tool（利用中间件的 batch 方法，如 HITL 批量审批）
                let original_calls: Vec<ToolCall> = reasoning.tool_calls.clone();
                let before_results = self
                    .chain
                    .run_before_tools_batch(state, original_calls.clone())
                    .await;
                let mut modified_calls: Vec<ToolCall> = Vec::with_capacity(original_calls.len());

                for (_idx, (tool_call, before_result)) in original_calls
                    .iter()
                    .zip(before_results.into_iter())
                    .enumerate()
                {
                    // before_tool 阶段也检查取消
                    if cancel.is_cancelled() {
                        return Err(AgentError::Interrupted);
                    }
                    let modified_call = match before_result {
                        Ok(c) => c,
                        Err(AgentError::ToolRejected { ref reason, .. }) => {
                            // 拒绝不终止 Agent，将拒绝原因作为工具错误反馈给 LLM
                            let rejection_result =
                                ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
                            self.emit(AgentEvent::ToolStart {
                                message_id: ai_msg_id,
                                tool_call_id: tool_call.id.clone(),
                                name: tool_call.name.clone(),
                                input: tool_call.input.clone(),
                            });
                            self.emit(AgentEvent::ToolEnd {
                                message_id: ai_msg_id,
                                tool_call_id: tool_call.id.clone(),
                                name: tool_call.name.clone(),
                                output: rejection_result.output.clone(),
                                is_error: true,
                            });
                            let tool_msg = BaseMessage::tool_error(
                                &rejection_result.tool_call_id,
                                rejection_result.output.as_str(),
                            );
                            let tool_msg_clone = tool_msg.clone();
                            state.add_message(tool_msg);
                            self.emit(AgentEvent::MessageAdded(tool_msg_clone));
                            all_tool_calls.push((tool_call.clone(), rejection_result));
                            continue;
                        }
                        Err(e) => {
                            self.chain.run_on_error(state, &e).await?;
                            return Err(e);
                        }
                    };
                    self.emit(AgentEvent::ToolStart {
                        message_id: ai_msg_id,
                        tool_call_id: modified_call.id.clone(),
                        name: modified_call.name.clone(),
                        input: modified_call.input.clone(),
                    });
                    modified_calls.push(modified_call);
                }

                // 阶段二：并发执行所有工具；取消时每个工具以 error 收尾
                let tool_results: Vec<Result<String, AgentError>> = {
                    let futures: Vec<_> = modified_calls
                        .iter()
                        .map(|call| {
                            let tool_name = call.name.clone();
                            let call_id = call.id.clone();
                            let input = call.input.clone();
                            let tool = all_tools.get(&call.name).copied();
                            let cancel = cancel.clone();
                            async move {
                                let span = tracing::info_span!(
                                    "agent.tool_call",
                                    tool.name = %tool_name,
                                    tool.call_id = %call_id,
                                );
                                let _enter = span.enter();
                                let invoke_fut = async {
                                    match tool {
                                        Some(t) => t.invoke(input).await.map_err(|e| {
                                            AgentError::ToolExecutionFailed {
                                                tool: tool_name.clone(),
                                                reason: e.to_string(),
                                            }
                                        }),
                                        None => Err(AgentError::ToolNotFound(tool_name.clone())),
                                    }
                                };
                                tokio::select! {
                                    biased;
                                    _ = cancel.cancelled() => {
                                        Err(AgentError::ToolExecutionFailed {
                                            tool: tool_name,
                                            reason: "interrupted by user".to_string(),
                                        })
                                    }
                                    result = invoke_fut => result,
                                }
                            }
                        })
                        .collect();
                    futures::future::join_all(futures).await
                };

                // 检查是否已取消（工具全部结束后再决定是否继续）
                let was_cancelled = cancel.is_cancelled();

                // 阶段三：串行处理结果、after_tool、state 更新
                for (modified_call, tool_result) in
                    modified_calls.into_iter().zip(tool_results.into_iter())
                {
                    let result = match tool_result {
                        Ok(output) => {
                            ToolResult::success(&modified_call.id, &modified_call.name, output)
                        }
                        Err(AgentError::ToolNotFound(ref name)) => {
                            tracing::warn!(tool.name = %name, "工具未找到，作为错误结果返回");
                            ToolResult::error(
                                &modified_call.id,
                                &modified_call.name,
                                format!("工具 '{}' 不存在", name),
                            )
                        }
                        Err(ref e) => {
                            self.chain.run_on_error(state, e).await?;
                            ToolResult::error(&modified_call.id, &modified_call.name, e.to_string())
                        }
                    };

                    tracing::debug!(
                        tool.name = %result.tool_name,
                        tool.is_error = result.is_error,
                        "tool call completed"
                    );
                    if result.is_error {
                        tracing::warn!(
                            tool.name = %result.tool_name,
                            tool.is_error = true,
                            error_len = result.output.len(),
                            "tool call failed"
                        );
                    }
                    self.emit(AgentEvent::ToolEnd {
                        message_id: ai_msg_id,
                        tool_call_id: modified_call.id.clone(),
                        name: modified_call.name.clone(),
                        output: result.output.clone(),
                        is_error: result.is_error,
                    });

                    if let Err(e) = self
                        .chain
                        .run_after_tool(state, &modified_call, &result)
                        .await
                    {
                        self.chain.run_on_error(state, &e).await?;
                        return Err(e);
                    }

                    let tool_msg = if result.is_error {
                        BaseMessage::tool_error(&result.tool_call_id, result.output.as_str())
                    } else {
                        BaseMessage::tool_result(&result.tool_call_id, result.output.as_str())
                    };
                    let tool_msg_clone = tool_msg.clone();
                    state.add_message(tool_msg);
                    self.emit(AgentEvent::MessageAdded(tool_msg_clone));

                    all_tool_calls.push((modified_call, result));
                }

                // 工具结果全部写入状态后，若已取消则以 Interrupted 退出
                // （调用方可保存此刻的 state.messages 实现断点续跑）
                if was_cancelled {
                    return Err(AgentError::Interrupted);
                }

                tracing::debug!(step, "react step done");
                self.emit(AgentEvent::StepDone { step });

                // 发送状态快照（从用户消息开始的所有消息），便于增量持久化
                let msgs_since_human = state.messages()[last_message_count..].to_vec();
                tracing::debug!(count = msgs_since_human.len(), "sending state snapshot");
                for msg in &msgs_since_human {
                    match msg {
                        BaseMessage::Ai {
                            content: _,
                            tool_calls,
                            ..
                        } => {
                            tracing::debug!(
                                has_tc = !tool_calls.is_empty(),
                                tc_len = tool_calls.len(),
                                "ai message in snapshot"
                            );
                        }
                        BaseMessage::Tool { tool_call_id, .. } => {
                            tracing::debug!(tc_id = %tool_call_id, "tool message in snapshot");
                        }
                        _ => {}
                    }
                }
                if !msgs_since_human.is_empty() {
                    self.emit(AgentEvent::StateSnapshot(msgs_since_human));
                }
                last_message_count = state.messages().len();
            } else {
                let answer = reasoning
                    .final_answer
                    .unwrap_or_else(|| reasoning.thought.clone());

                if answer.trim().is_empty() {
                    tracing::warn!(
                        step,
                        "LLM 返回空最终回答（无 tool_calls 且 final_answer/thought 为空）"
                    );
                }

                // 优先使用带 Reasoning block 的原始消息，保留 thinking 内容
                let ai_msg = reasoning
                    .source_message
                    .unwrap_or_else(|| BaseMessage::ai(answer.as_str()));
                let ai_msg_id = ai_msg.id(); // 捕获 message_id（Copy，供 TextChunk 使用）
                let ai_msg_clone = ai_msg.clone();
                state.add_message(ai_msg);
                self.emit(AgentEvent::MessageAdded(ai_msg_clone));

                self.emit(AgentEvent::TextChunk {
                    message_id: ai_msg_id,
                    chunk: answer.clone(),
                });

                // 发送包含最终回答的 StateSnapshot，确保 TUI 侧的 agent_state_messages
                // 包含完整对话历史（包括本次最终回答），否则下一轮对话上下文会丢失
                let msgs_since_last = state.messages()[last_message_count..].to_vec();
                if !msgs_since_last.is_empty() {
                    self.emit(AgentEvent::StateSnapshot(msgs_since_last));
                }

                let output = AgentOutput {
                    text: answer,
                    steps: step + 1,
                    tool_calls: all_tool_calls,
                };

                tracing::info!(
                    steps = output.steps,
                    tool_calls = output.tool_calls.len(),
                    "agent finished"
                );

                return match self.chain.run_after_agent(state, output).await {
                    Ok(o) => Ok(o),
                    Err(e) => {
                        self.chain.run_on_error(state, &e).await?;
                        Err(e)
                    }
                };
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
mod tests {
    use super::*;
    use crate::agent::react::{AgentInput, Reasoning};
    use crate::agent::state::AgentState;
    use crate::messages::BaseMessage;
    use crate::tools::BaseTool;
    use std::time::{Duration, Instant};

    // ─── Mock LLM：第一步返回两个并发工具调用，第二步返回最终答案 ───────────

    struct TwoToolCallLLM;

    #[async_trait::async_trait]
    impl ReactLLM for TwoToolCallLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "need both tools",
                    vec![
                        ToolCall::new("id1", "slow_tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "slow_tool_b", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "parallel ok"))
            }
        }
    }

    // ─── Mock 工具：sleep 100ms ────────────────────────────────────────────────

    struct SlowTool {
        tool_name: &'static str,
    }

    #[async_trait::async_trait]
    impl BaseTool for SlowTool {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "slow test tool"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(format!("{} done", self.tool_name))
        }
    }

    /// 验证两个各耗时 100ms 的工具并发执行，总耗时应 < 160ms（串行需 ≥ 200ms）
    #[tokio::test]
    async fn test_parallel_tool_execution() {
        let agent = ReActAgent::new(TwoToolCallLLM)
            .max_iterations(5)
            .register_tool(Box::new(SlowTool {
                tool_name: "slow_tool_a",
            }))
            .register_tool(Box::new(SlowTool {
                tool_name: "slow_tool_b",
            }));

        let mut state = AgentState::new("/tmp");
        let start = Instant::now();
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(output.text, "parallel ok");
        assert_eq!(output.tool_calls.len(), 2);
        assert!(
            elapsed < Duration::from_millis(160),
            "并行执行耗时 {:?}，应 < 160ms（串行需 ≥ 200ms）",
            elapsed
        );
    }

    /// 验证取消 token 触发时，工具以 error 收尾并返回 Interrupted
    #[tokio::test]
    async fn test_cancel_during_tool_execution() {
        struct HangingTool;
        #[async_trait::async_trait]
        impl BaseTool for HangingTool {
            fn name(&self) -> &str {
                "hanging_tool"
            }
            fn description(&self) -> &str {
                "hangs forever"
            }
            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(
                &self,
                _input: serde_json::Value,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok("never".to_string())
            }
        }

        struct OneToolLLM;
        #[async_trait::async_trait]
        impl ReactLLM for OneToolLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                let has_tool = messages
                    .iter()
                    .any(|m| matches!(m, BaseMessage::Tool { .. }));
                if !has_tool {
                    Ok(Reasoning::with_tools(
                        "call tool",
                        vec![ToolCall::new("id1", "hanging_tool", serde_json::json!({}))],
                    ))
                } else {
                    Ok(Reasoning::with_answer("done", "ok"))
                }
            }
        }

        let cancel = CancellationToken::new();
        let agent = ReActAgent::new(OneToolLLM)
            .max_iterations(5)
            .register_tool(Box::new(HangingTool));

        // 50ms 后触发取消
        let token = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            token.cancel();
        });

        let mut state = AgentState::new("/tmp");
        let result = agent
            .execute(AgentInput::text("go"), &mut state, Some(cancel))
            .await;

        assert!(matches!(result, Err(AgentError::Interrupted)));
        // 工具 error 结果已写入 state（可用于断点续跑）
        let has_tool_error = state
            .messages()
            .iter()
            .any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
        assert!(has_tool_error, "取消后工具 error 消息应已写入 state");
    }

    /// 验证 HITL 拒绝（ToolRejected）不终止 Agent，LLM 能收到拒绝原因后继续
    #[tokio::test]
    async fn test_tool_rejection_continues_loop() {
        use crate::middleware::r#trait::Middleware;

        struct RejectAllMiddleware;
        #[async_trait::async_trait]
        impl<S: State> Middleware<S> for RejectAllMiddleware {
            fn name(&self) -> &str {
                "RejectAllMiddleware"
            }
            async fn before_tool(
                &self,
                _state: &mut S,
                tool_call: &ToolCall,
            ) -> AgentResult<ToolCall> {
                Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason: "用户拒绝".to_string(),
                })
            }
        }

        // LLM：先调用工具，收到拒绝结果后返回最终答案
        struct TestLLM;
        #[async_trait::async_trait]
        impl ReactLLM for TestLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> AgentResult<Reasoning> {
                let has_tool_result = messages
                    .iter()
                    .any(|m| matches!(m, BaseMessage::Tool { .. }));
                if !has_tool_result {
                    Ok(Reasoning::with_tools(
                        "try tool",
                        vec![ToolCall::new(
                            "id1",
                            "bash",
                            serde_json::json!({"command": "ls"}),
                        )],
                    ))
                } else {
                    Ok(Reasoning::with_answer("adjusted", "done after rejection"))
                }
            }
        }

        let agent = ReActAgent::new(TestLLM)
            .max_iterations(5)
            .add_middleware(Box::new(RejectAllMiddleware));

        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();

        assert_eq!(output.text, "done after rejection");
        // 拒绝结果应写入 state（is_error=true）
        let has_rejection = state
            .messages()
            .iter()
            .any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
        assert!(has_rejection, "拒绝结果应写入 state");
        // Agent 总工具调用记录中应有 1 条（被拒绝的）
        assert_eq!(output.tool_calls.len(), 1);
    }

    /// 验证 TextChunk 携带的 message_id 与前一条 MessageAdded(Ai) 的 id 一致
    #[tokio::test]
    async fn test_text_chunk_message_id() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct FinalAnswerLLM;
        #[async_trait::async_trait]
        impl ReactLLM for FinalAnswerLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                Ok(Reasoning::with_answer("thinking", "final answer"))
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let agent = ReActAgent::new(FinalAnswerLLM)
            .max_iterations(3)
            .with_event_handler(Arc::new(FnEventHandler(move |event| {
                events_clone.lock().unwrap().push(event);
            })));

        let mut state = AgentState::new("/tmp");
        agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();

        let evs = events.lock().unwrap();

        // 找到 MessageAdded(Ai) 的 id（最终答案那条）
        let ai_msg_id = evs.iter().find_map(|e| {
            if let AgentEvent::MessageAdded(BaseMessage::Ai { id, tool_calls, .. }) = e {
                if tool_calls.is_empty() {
                    Some(*id)
                } else {
                    None
                }
            } else {
                None
            }
        });

        // 找到 TextChunk 的 message_id
        let chunk_msg_id = evs.iter().find_map(|e| {
            if let AgentEvent::TextChunk { message_id, .. } = e {
                Some(*message_id)
            } else {
                None
            }
        });

        assert!(ai_msg_id.is_some(), "应有 MessageAdded(Ai) 事件");
        assert!(chunk_msg_id.is_some(), "应有 TextChunk 事件");
        assert_eq!(
            ai_msg_id.unwrap(),
            chunk_msg_id.unwrap(),
            "TextChunk.message_id 应与 MessageAdded(Ai).id 相同"
        );
    }

    /// 验证 ToolStart/ToolEnd 携带的 message_id 与同轮次 MessageAdded(Ai) 的 id 一致
    #[tokio::test]
    async fn test_tool_message_id() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct OneToolLLM;
        #[async_trait::async_trait]
        impl ReactLLM for OneToolLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                if messages
                    .iter()
                    .any(|m| matches!(m, BaseMessage::Tool { .. }))
                {
                    Ok(Reasoning::with_answer("done", "ok"))
                } else {
                    Ok(Reasoning::with_tools(
                        "call tool",
                        vec![ToolCall::new("tc1", "echo_tool", serde_json::json!({}))],
                    ))
                }
            }
        }

        struct EchoTool;
        #[async_trait::async_trait]
        impl BaseTool for EchoTool {
            fn name(&self) -> &str {
                "echo_tool"
            }
            fn description(&self) -> &str {
                "echoes"
            }
            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(
                &self,
                _: serde_json::Value,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok("echo".to_string())
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let agent = ReActAgent::new(OneToolLLM)
            .max_iterations(5)
            .register_tool(Box::new(EchoTool))
            .with_event_handler(Arc::new(FnEventHandler(move |event| {
                events_clone.lock().unwrap().push(event);
            })));

        let mut state = AgentState::new("/tmp");
        agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();

        let evs = events.lock().unwrap();

        // 找到第一个带工具调用的 MessageAdded(Ai) 的 id
        let ai_msg_id = evs.iter().find_map(|e| {
            if let AgentEvent::MessageAdded(BaseMessage::Ai { id, tool_calls, .. }) = e {
                if !tool_calls.is_empty() {
                    Some(*id)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let tool_start_msg_id = evs.iter().find_map(|e| {
            if let AgentEvent::ToolStart { message_id, .. } = e {
                Some(*message_id)
            } else {
                None
            }
        });
        let tool_end_msg_id = evs.iter().find_map(|e| {
            if let AgentEvent::ToolEnd { message_id, .. } = e {
                Some(*message_id)
            } else {
                None
            }
        });

        assert!(
            ai_msg_id.is_some(),
            "应有带工具调用的 MessageAdded(Ai) 事件"
        );
        assert!(tool_start_msg_id.is_some(), "应有 ToolStart 事件");
        assert!(tool_end_msg_id.is_some(), "应有 ToolEnd 事件");
        assert_eq!(
            ai_msg_id.unwrap(),
            tool_start_msg_id.unwrap(),
            "ToolStart.message_id 应与 MessageAdded(Ai).id 相同"
        );
        assert_eq!(
            ai_msg_id.unwrap(),
            tool_end_msg_id.unwrap(),
            "ToolEnd.message_id 应与 MessageAdded(Ai).id 相同"
        );
    }

    /// 验证 with_system_prompt 注入的 system 消息位于消息列表第一位
    #[tokio::test]
    async fn test_system_prompt_is_first() {
        struct EchoLLM;
        #[async_trait::async_trait]
        impl ReactLLM for EchoLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                Ok(Reasoning::with_answer("", "done"))
            }
        }

        let agent = ReActAgent::new(EchoLLM)
            .max_iterations(3)
            .with_system_prompt("system content here");

        let mut state = AgentState::new("/tmp");
        agent
            .execute(AgentInput::text("hi"), &mut state, None)
            .await
            .unwrap();

        let messages = state.messages();
        let first = messages.first().expect("应至少有一条消息");
        assert!(
            matches!(first, BaseMessage::System { .. }),
            "第一条消息应为 System，实际为: {:?}",
            first
        );
        assert!(
            first.content().contains("system content here"),
            "system 内容应包含注入文本"
        );
    }

    /// 验证不论其他中间件注册顺序如何，with_system_prompt 的 system 消息始终在最前
    #[tokio::test]
    async fn test_system_prompt_order_independent() {
        use crate::middleware::r#trait::Middleware;

        // 一个会在 before_agent 中 prepend 自己消息的中间件
        struct PrefixMiddleware;
        #[async_trait::async_trait]
        impl<S: State> Middleware<S> for PrefixMiddleware {
            fn name(&self) -> &str {
                "PrefixMiddleware"
            }
            async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
                state.prepend_message(BaseMessage::system("middleware injected"));
                Ok(())
            }
        }

        struct EchoLLM;
        #[async_trait::async_trait]
        impl ReactLLM for EchoLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                Ok(Reasoning::with_answer("", "done"))
            }
        }

        // 中间件在 with_system_prompt 之前注册——但 system prompt 应在最前
        let agent = ReActAgent::new(EchoLLM)
            .add_middleware(Box::new(PrefixMiddleware))
            .with_system_prompt("top level system");

        let mut state = AgentState::new("/tmp");
        agent
            .execute(AgentInput::text("hi"), &mut state, None)
            .await
            .unwrap();

        let messages = state.messages();
        let first = messages.first().expect("应至少有一条消息");
        assert!(
            first.content().contains("top level system"),
            "with_system_prompt 注入的消息应在最前，实际第一条: {:?}",
            first.content()
        );
    }

    /// 验证 TextChunk/ToolStart/ToolEnd 序列化后含 message_id 字段
    #[test]
    fn test_agent_event_message_id_serialization() {
        use crate::agent::events::AgentEvent;
        use crate::messages::MessageId;

        let mid = MessageId::new();

        let ev = AgentEvent::TextChunk {
            message_id: mid,
            chunk: "hello".to_string(),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert!(
            json["message_id"].is_string(),
            "TextChunk JSON 应含 message_id 字段"
        );
        assert_eq!(json["chunk"].as_str().unwrap(), "hello");

        let ev = AgentEvent::ToolStart {
            message_id: mid,
            tool_call_id: "tc1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({}),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert!(
            json["message_id"].is_string(),
            "ToolStart JSON 应含 message_id 字段"
        );

        let ev = AgentEvent::ToolEnd {
            message_id: mid,
            tool_call_id: "tc1".to_string(),
            name: "bash".to_string(),
            output: "ok".to_string(),
            is_error: false,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert!(
            json["message_id"].is_string(),
            "ToolEnd JSON 应含 message_id 字段"
        );
    }

    /// 验证最终回答路径也会发出 StateSnapshot，确保多轮对话不丢失 AI 回复
    #[tokio::test]
    async fn test_state_snapshot_on_final_answer() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct FinalAnswerLLM;
        #[async_trait::async_trait]
        impl ReactLLM for FinalAnswerLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                Ok(Reasoning::with_answer("thinking", "final answer"))
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let agent = ReActAgent::new(FinalAnswerLLM)
            .max_iterations(3)
            .with_event_handler(Arc::new(FnEventHandler(move |event| {
                events_clone.lock().unwrap().push(event);
            })));

        let mut state = AgentState::new("/tmp");
        agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();

        let evs = events.lock().unwrap();
        let snapshots: Vec<_> = evs
            .iter()
            .filter(|e| matches!(e, AgentEvent::StateSnapshot(_)))
            .collect();

        assert!(!snapshots.is_empty(), "最终回答路径应发出 StateSnapshot");

        // 最后一个 snapshot 应包含 AI 最终回答
        if let AgentEvent::StateSnapshot(msgs) = snapshots.last().unwrap() {
            let has_ai_text = msgs
                .iter()
                .any(|m| matches!(m, BaseMessage::Ai { tool_calls, .. } if tool_calls.is_empty()));
            assert!(has_ai_text, "StateSnapshot 应包含不带工具调用的 AI 消息");
        }
    }

    /// 验证达到最大迭代次数时返回 MaxIterationsExceeded 错误
    #[tokio::test]
    async fn test_max_iterations_exceeded() {
        struct AlwaysToolLLM;
        #[async_trait::async_trait]
        impl ReactLLM for AlwaysToolLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                Ok(Reasoning::with_tools(
                    "loop",
                    vec![ToolCall::new("id1", "echo_tool", serde_json::json!({}))],
                ))
            }
        }

        struct EchoTool;
        #[async_trait::async_trait]
        impl BaseTool for EchoTool {
            fn name(&self) -> &str {
                "echo_tool"
            }
            fn description(&self) -> &str {
                "echoes"
            }
            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(
                &self,
                _: serde_json::Value,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok("echo".to_string())
            }
        }

        let agent = ReActAgent::new(AlwaysToolLLM)
            .max_iterations(3)
            .register_tool(Box::new(EchoTool));

        let mut state = AgentState::new("/tmp");
        let result = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await;

        assert!(matches!(result, Err(AgentError::MaxIterationsExceeded(3))));
        // 1 human + 3*(ai + tool_result)
        assert_eq!(state.messages().len(), 7);
    }

    /// 验证两个工具调用通过批量 before_tools_batch 处理（HITL 批量审批路径）
    #[tokio::test]
    async fn test_batch_before_tools_execution() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct TwoToolLLM;
        #[async_trait::async_trait]
        impl ReactLLM for TwoToolLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                if messages
                    .iter()
                    .any(|m| matches!(m, BaseMessage::Tool { .. }))
                {
                    Ok(Reasoning::with_answer("done", "ok"))
                } else {
                    Ok(Reasoning::with_tools(
                        "need both",
                        vec![
                            ToolCall::new("id1", "tool_a", serde_json::json!({})),
                            ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ],
                    ))
                }
            }
        }

        struct EchoTool {
            name_str: &'static str,
        }
        #[async_trait::async_trait]
        impl BaseTool for EchoTool {
            fn name(&self) -> &str {
                self.name_str
            }
            fn description(&self) -> &str {
                "echo"
            }
            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(
                &self,
                _: serde_json::Value,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok(format!("{} done", self.name_str))
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let agent = ReActAgent::new(TwoToolLLM)
            .max_iterations(5)
            .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
            .register_tool(Box::new(EchoTool { name_str: "tool_b" }))
            .with_event_handler(Arc::new(FnEventHandler(move |event| {
                events_clone.lock().unwrap().push(event);
            })));

        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();

        assert_eq!(output.text, "ok");
        assert_eq!(output.tool_calls.len(), 2);

        let evs = events.lock().unwrap();
        let tool_starts: Vec<_> = evs
            .iter()
            .filter(|e| matches!(e, AgentEvent::ToolStart { .. }))
            .collect();
        assert_eq!(tool_starts.len(), 2, "应有 2 个 ToolStart 事件");
    }

    /// 验证 with_context_budget 设置后 executor 使用 ContextBudget 阈值
    #[tokio::test]
    async fn test_context_budget_wiring() {
        struct TokenLLM {
            input_tokens: u32,
            output_tokens: u32,
        }
        #[async_trait::async_trait]
        impl ReactLLM for TokenLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                let mut r = Reasoning::with_answer("", "ok");
                r.usage = Some(crate::llm::types::TokenUsage {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                });
                Ok(r)
            }
        }

        // context_window=1000, warning_threshold=0.5 → 600/1000=60% > 50%
        let budget = ContextBudget::new(1000).with_warning_threshold(0.5);
        let agent = ReActAgent::new(TokenLLM {
            input_tokens: 400,
            output_tokens: 200,
        })
        .max_iterations(3)
        .with_context_budget(budget);
        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();
        assert_eq!(output.text, "ok");
        let t = state.token_tracker();
        assert_eq!(t.total_input_tokens, 400);
        assert_eq!(t.total_output_tokens, 200);
        assert_eq!(t.llm_call_count, 1);
    }

    /// 验证无 ContextBudget 时回退到硬编码 80% 阈值（向后兼容）
    #[tokio::test]
    async fn test_no_context_budget_fallback() {
        struct LowTokenLLM;
        #[async_trait::async_trait]
        impl ReactLLM for LowTokenLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                let mut r = Reasoning::with_answer("", "ok");
                r.usage = Some(crate::llm::types::TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                });
                Ok(r)
            }
        }
        let agent = ReActAgent::new(LowTokenLLM).max_iterations(3);
        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();
        assert_eq!(output.text, "ok");
        assert_eq!(state.token_tracker().llm_call_count, 1);
    }

    /// 验证 ContextBudget 路径下 ContextWarning 事件被发出
    #[tokio::test]
    async fn test_context_budget_emits_warning_event() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct TokenLLM {
            input_tokens: u32,
            output_tokens: u32,
        }
        #[async_trait::async_trait]
        impl ReactLLM for TokenLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                let mut r = Reasoning::with_answer("", "ok");
                r.usage = Some(crate::llm::types::TokenUsage {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                });
                Ok(r)
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
        let events_clone = events.clone();

        // context_window=1000, warning_threshold=0.5 → 400+200=600/1000=60% > 50%
        let budget = ContextBudget::new(1000).with_warning_threshold(0.5);
        let agent = ReActAgent::new(TokenLLM {
            input_tokens: 400,
            output_tokens: 200,
        })
        .max_iterations(3)
        .with_context_budget(budget)
        .with_event_handler(Arc::new(FnEventHandler(move |ev| {
            events_clone.lock().unwrap().push(ev);
        })));

        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();
        assert_eq!(output.text, "ok");

        let evs = events.lock().unwrap();
        let warnings: Vec<_> = evs
            .iter()
            .filter(|e| matches!(e, AgentEvent::ContextWarning { .. }))
            .collect();
        assert_eq!(warnings.len(), 1, "ContextWarning 应在超过警告阈值时发出");
        if let AgentEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        } = warnings[0]
        {
            assert_eq!(*used_tokens, 600, "used_tokens = input+output = 400+200");
            assert_eq!(*total_tokens, 1000, "total_tokens = budget.context_window");
            assert!((*percentage - 60.0).abs() < 1.0, "percentage ≈ 60%");
        }
    }

    /// 验证无 ContextBudget 时回退路径也发出 ContextWarning 事件
    #[tokio::test]
    async fn test_fallback_path_emits_warning_event() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct HighTokenLLM;
        #[async_trait::async_trait]
        impl ReactLLM for HighTokenLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                let mut r = Reasoning::with_answer("", "ok");
                // context_window 默认 200K，90K+80K=170K = 85% > 80% 硬编码阈值
                r.usage = Some(crate::llm::types::TokenUsage {
                    input_tokens: 90000,
                    output_tokens: 80000,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                });
                Ok(r)
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
        let events_clone = events.clone();

        let agent = ReActAgent::new(HighTokenLLM)
            .max_iterations(3)
            .with_event_handler(Arc::new(FnEventHandler(move |ev| {
                events_clone.lock().unwrap().push(ev);
            })));

        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();
        assert_eq!(output.text, "ok");

        let evs = events.lock().unwrap();
        let warnings: Vec<_> = evs
            .iter()
            .filter(|e| matches!(e, AgentEvent::ContextWarning { .. }))
            .collect();
        assert_eq!(
            warnings.len(),
            1,
            "无 budget 时回退路径也应发出 ContextWarning"
        );
        if let AgentEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        } = warnings[0]
        {
            assert_eq!(*used_tokens, 170000, "used_tokens = input+output");
            assert!((*percentage - 85.0).abs() < 1.0, "percentage ≈ 85%");
        }
    }

    /// 验证低 token 用量时不发出 ContextWarning
    #[tokio::test]
    async fn test_low_usage_no_warning_event() {
        use crate::agent::events::{AgentEvent, FnEventHandler};
        use std::sync::{Arc, Mutex};

        struct LowTokenLLM;
        #[async_trait::async_trait]
        impl ReactLLM for LowTokenLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> crate::error::AgentResult<Reasoning> {
                let mut r = Reasoning::with_answer("", "ok");
                r.usage = Some(crate::llm::types::TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                });
                Ok(r)
            }
        }

        let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
        let events_clone = events.clone();

        let agent = ReActAgent::new(LowTokenLLM)
            .max_iterations(3)
            .with_event_handler(Arc::new(FnEventHandler(move |ev| {
                events_clone.lock().unwrap().push(ev);
            })));

        let mut state = AgentState::new("/tmp");
        let output = agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .unwrap();
        assert_eq!(output.text, "ok");

        let evs = events.lock().unwrap();
        // LLM 必然有 LlmCallEnd，但 low usage 不触发 ContextWarning
        let has_warning = evs
            .iter()
            .any(|e| matches!(e, AgentEvent::ContextWarning { .. }));
        assert!(!has_warning, "低 token 用量不应发出 ContextWarning");
    }
}
