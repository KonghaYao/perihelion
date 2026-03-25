use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::agent::events::{AgentEvent, AgentEventHandler};
use crate::agent::react::{AgentInput, AgentOutput, ReactLLM, ToolCall, ToolResult};
use crate::agent::state::State;
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
    /// 上次发送 StateSnapshot 后的消息数量（用于增量发送）
    last_message_count: std::sync::atomic::AtomicUsize,
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
            last_message_count: std::sync::atomic::AtomicUsize::new(0),
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

        // 重置消息计数，从用户消息之后开始跟踪
        self.last_message_count.store(state.messages().len(), std::sync::atomic::Ordering::SeqCst);

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

        let mut all_tool_calls: Vec<(ToolCall, ToolResult)> = Vec::new();

        for step in 0..self.max_iterations {
            state.set_current_step(step);

            // ── LLM 推理（与 cancel 竞争）────────────────────────────────────
            self.emit(AgentEvent::LlmCallStart {
                step,
                messages: state.messages().to_vec(),
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
                            self.chain.run_on_error(state, &e).await?;
                            return Err(e);
                        }
                    }
                }
            };
            {
                let llm_output = reasoning.final_answer.as_deref()
                    .unwrap_or(&reasoning.thought)
                    .to_string();
                self.emit(AgentEvent::LlmCallEnd {
                    step,
                    model: self.llm.model_name(),
                    output: llm_output,
                    usage: reasoning.usage.clone(),
                });
            }

            if reasoning.needs_tool_call() {
                {
                    let tc_reqs: Vec<ToolCallRequest> = reasoning
                        .tool_calls
                        .iter()
                        .map(|tc| {
                            ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone())
                        })
                        .collect();
                    // 优先使用带 Reasoning block 的原始消息，保留 thinking 内容
                    // source_message 的 tool_calls 字段在 LLM 解析阶段已填好
                    let ai_msg = reasoning.source_message.clone()
                        .unwrap_or_else(|| BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs));
                    let ai_msg_clone = ai_msg.clone();
                    state.add_message(ai_msg);
                    self.emit(AgentEvent::MessageAdded(ai_msg_clone));
                }
                // emit AI 推理内容到 Relay
                self.emit(AgentEvent::AiReasoning(reasoning.thought.clone()));

                // 阶段一：串行执行 before_tool（需要 &mut S，且 HITL 可能修改 call）
                let mut modified_calls: Vec<ToolCall> = Vec::new();
                for tool_call in reasoning.tool_calls {
                    // before_tool 阶段也检查取消
                    if cancel.is_cancelled() {
                        return Err(AgentError::Interrupted);
                    }
                    let modified_call =
                        match self.chain.run_before_tool(state, tool_call.clone()).await {
                            Ok(c) => c,
                            Err(AgentError::ToolRejected { ref reason, .. }) => {
                                // 拒绝不终止 Agent，将拒绝原因作为工具错误反馈给 LLM
                                let rejection_result = ToolResult::error(
                                    &tool_call.id,
                                    &tool_call.name,
                                    reason.clone(),
                                );
                                self.emit(AgentEvent::ToolStart {
                                    tool_call_id: tool_call.id.clone(),
                                    name: tool_call.name.clone(),
                                    input: tool_call.input.clone(),
                                });
                                self.emit(AgentEvent::ToolEnd {
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
                                all_tool_calls.push((tool_call, rejection_result));
                                continue;
                            }
                            Err(e) => {
                                self.chain.run_on_error(state, &e).await?;
                                return Err(e);
                            }
                        };
                    self.emit(AgentEvent::ToolStart {
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
                            let e = AgentError::ToolNotFound(name.clone());
                            self.chain.run_on_error(state, &e).await?;
                            return Err(e);
                        }
                        Err(ref e) => {
                            self.chain.run_on_error(state, e).await?;
                            ToolResult::error(
                                &modified_call.id,
                                &modified_call.name,
                                e.to_string(),
                            )
                        }
                    };

                    tracing::debug!(
                        tool.name = %result.tool_name,
                        tool.is_error = result.is_error,
                        "tool call completed"
                    );
                    self.emit(AgentEvent::ToolEnd {
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
                let msgs_since_human = state.messages()[self.last_message_count.load(std::sync::atomic::Ordering::SeqCst)..]
                    .to_vec();
                tracing::debug!(count = msgs_since_human.len(), "sending state snapshot");
                for msg in &msgs_since_human {
                    match msg {
                        BaseMessage::Ai { content: _, tool_calls } => {
                            tracing::debug!(has_tc = !tool_calls.is_empty(), tc_len = tool_calls.len(), "ai message in snapshot");
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
                self.last_message_count.store(state.messages().len(), std::sync::atomic::Ordering::SeqCst);
            } else {
                let answer = reasoning
                    .final_answer
                    .unwrap_or_else(|| reasoning.thought.clone());

                // 优先使用带 Reasoning block 的原始消息，保留 thinking 内容
                let ai_msg = reasoning.source_message
                    .unwrap_or_else(|| BaseMessage::ai(answer.as_str()));
                let ai_msg_clone = ai_msg.clone();
                state.add_message(ai_msg);
                self.emit(AgentEvent::MessageAdded(ai_msg_clone));

                self.emit(AgentEvent::TextChunk(answer.clone()));

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
            let has_tool_result = messages.iter().any(|m| matches!(m, BaseMessage::Tool { .. }));
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
            .register_tool(Box::new(SlowTool { tool_name: "slow_tool_a" }))
            .register_tool(Box::new(SlowTool { tool_name: "slow_tool_b" }));

        let mut state = AgentState::new("/tmp");
        let start = Instant::now();
        let output =
            agent.execute(AgentInput::text("go"), &mut state, None).await.unwrap();
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
            fn name(&self) -> &str { "hanging_tool" }
            fn description(&self) -> &str { "hangs forever" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
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
                let has_tool = messages.iter().any(|m| matches!(m, BaseMessage::Tool { .. }));
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
        let result = agent.execute(AgentInput::text("go"), &mut state, Some(cancel)).await;

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
            fn name(&self) -> &str { "RejectAllMiddleware" }
            async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
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
                let has_tool_result = messages.iter().any(|m| matches!(m, BaseMessage::Tool { .. }));
                if !has_tool_result {
                    Ok(Reasoning::with_tools("try tool", vec![
                        ToolCall::new("id1", "bash", serde_json::json!({"command": "ls"})),
                    ]))
                } else {
                    Ok(Reasoning::with_answer("adjusted", "done after rejection"))
                }
            }
        }

        let agent = ReActAgent::new(TestLLM)
            .max_iterations(5)
            .add_middleware(Box::new(RejectAllMiddleware));

        let mut state = AgentState::new("/tmp");
        let output = agent.execute(AgentInput::text("go"), &mut state, None).await.unwrap();

        assert_eq!(output.text, "done after rejection");
        // 拒绝结果应写入 state（is_error=true）
        let has_rejection = state.messages().iter().any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
        assert!(has_rejection, "拒绝结果应写入 state");
        // Agent 总工具调用记录中应有 1 条（被拒绝的）
        assert_eq!(output.tool_calls.len(), 1);
    }

}
