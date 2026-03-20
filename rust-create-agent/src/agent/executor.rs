use std::sync::Arc;

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
    #[instrument(name = "agent.execute", skip(self, input, state),
        fields(max_iterations = self.max_iterations))]
    pub async fn execute(&self, input: AgentInput, state: &mut S) -> AgentResult<AgentOutput> {
        let human_msg = BaseMessage::human(input.content);
        state.add_message(human_msg);

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

            let reasoning = match self
                .llm
                .generate_reasoning(state.messages(), &tool_refs)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    self.chain.run_on_error(state, &e).await?;
                    return Err(e);
                }
            };

            if reasoning.needs_tool_call() {
                {
                    let tc_reqs: Vec<ToolCallRequest> = reasoning
                        .tool_calls
                        .iter()
                        .map(|tc| {
                            ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone())
                        })
                        .collect();
                    let ai_msg =
                        BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs);
                    state.add_message(ai_msg);
                }

                // 阶段一：串行执行 before_tool（需要 &mut S，且 HITL 可能修改 call）
                let mut modified_calls: Vec<ToolCall> = Vec::new();
                for tool_call in reasoning.tool_calls {
                    let modified_call =
                        match self.chain.run_before_tool(state, tool_call.clone()).await {
                            Ok(c) => c,
                            Err(e) => {
                                self.chain.run_on_error(state, &e).await?;
                                return Err(e);
                            }
                        };
                    // 工具调用开始事件
                    self.emit(AgentEvent::ToolStart {
                        name: modified_call.name.clone(),
                        input: modified_call.input.clone(),
                    });
                    modified_calls.push(modified_call);
                }

                // 阶段二：并发执行所有工具（不涉及 &mut S）
                let tool_results: Vec<Result<String, AgentError>> = {
                    let futures: Vec<_> = modified_calls
                        .iter()
                        .map(|call| {
                            let tool_name = call.name.clone();
                            let call_id = call.id.clone();
                            let input = call.input.clone();
                            let tool = all_tools.get(&call.name).copied();
                            async move {
                                let span = tracing::info_span!(
                                    "agent.tool_call",
                                    tool.name = %tool_name,
                                    tool.call_id = %call_id,
                                );
                                let _enter = span.enter();
                                match tool {
                                    Some(t) => t.invoke(input).await.map_err(|e| {
                                        AgentError::ToolExecutionFailed {
                                            tool: tool_name,
                                            reason: e.to_string(),
                                        }
                                    }),
                                    None => Err(AgentError::ToolNotFound(tool_name)),
                                }
                            }
                        })
                        .collect();
                    futures::future::join_all(futures).await
                };

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
                    state.add_message(tool_msg);

                    all_tool_calls.push((modified_call, result));
                }

                // 步骤完成事件
                tracing::debug!(step, "react step done");
                self.emit(AgentEvent::StepDone { step });
            } else {
                let answer = reasoning
                    .final_answer
                    .unwrap_or_else(|| reasoning.thought.clone());

                state.add_message(BaseMessage::ai(answer.as_str()));

                // 最终文字输出事件
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
            // 第一步（只有 human 消息）：返回两个工具调用
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
        let output = agent.execute(AgentInput::text("go"), &mut state).await.unwrap();
        let elapsed = start.elapsed();

        assert_eq!(output.text, "parallel ok");
        assert_eq!(output.tool_calls.len(), 2);
        assert!(
            elapsed < Duration::from_millis(160),
            "并行执行耗时 {:?}，应 < 160ms（串行需 ≥ 200ms）",
            elapsed
        );
    }
}
