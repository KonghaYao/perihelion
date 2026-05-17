use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::agent::events::AgentEvent;
use crate::agent::react::{ReactLLM, Reasoning, ToolCall, ToolResult};
use crate::agent::state::State;
use crate::error::{AgentError, AgentResult};
use crate::messages::{message::MessageId, BaseMessage, ToolCallRequest};
use crate::tools::BaseTool;

use super::ReActAgent;

/// 工具审批 → 并发执行 → 结果收集（不写 state）→ 统一写入
pub(crate) async fn dispatch_tools<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    reasoning: &Reasoning,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
) -> AgentResult<Vec<(ToolCall, ToolResult)>> {
    let tc_reqs: Vec<ToolCallRequest> = reasoning
        .tool_calls
        .iter()
        .map(|tc| ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone()))
        .collect();
    let ai_msg = reasoning
        .source_message
        .clone()
        .unwrap_or_else(|| BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs));
    let ai_msg_id = ai_msg.id();

    // emit AI 工具前文本（非流式；流式模式下 LLM 适配器已通过 StreamingContext emit）
    if !reasoning.streamed && !reasoning.thought.trim().is_empty() {
        agent.emit(AgentEvent::TextChunk {
            message_id: ai_msg_id,
            chunk: reasoning.thought.clone(),
            source_agent_id: None,
        });
    }

    // 阶段 A：收集所有工具调用结果（不写 state）
    // 返回 Err 仅在 before_tool 错误路径（此时 state 干净，无 AI 消息）
    tracing::debug!(
        "[DEADLOCK] dispatch_tools: {} tool calls to dispatch, names={:?}",
        reasoning.tool_calls.len(),
        reasoning
            .tool_calls
            .iter()
            .map(|tc| tc.name.as_str())
            .collect::<Vec<_>>()
    );
    let (results, was_cancelled, deferred_error) = collect_tool_results(
        agent,
        state,
        reasoning.tool_calls.clone(),
        all_tools,
        cancel,
        ai_msg_id,
    )
    .await?;

    tracing::debug!(
        "[DEADLOCK] dispatch_tools: collect_tool_results done, {} results, was_cancelled={}, deferred={}",
        results.len(), was_cancelled, deferred_error.is_some()
    );

    // 阶段 B：一次性写入 state（Cancel / deferred_error 路径也写入，保证 state 一致）
    agent.emit(AgentEvent::MessageAdded(ai_msg.clone()));
    state.add_message(ai_msg);

    for (_, result) in &results {
        let tool_msg = if result.is_error {
            BaseMessage::tool_error(&result.tool_call_id, result.output.as_str())
        } else {
            BaseMessage::tool_result(&result.tool_call_id, result.output.as_str())
        };
        let tool_msg_clone = tool_msg.clone();
        state.add_message(tool_msg);
        agent.emit(AgentEvent::MessageAdded(tool_msg_clone));
    }

    // 写入完成后再返回错误
    if was_cancelled {
        tracing::warn!("[DEADLOCK] dispatch_tools: returning Interrupted (was_cancelled)");
        return Err(AgentError::Interrupted);
    }
    if let Some(msg) = deferred_error {
        tracing::warn!(
            "[DEADLOCK] dispatch_tools: returning MiddlewareError: {}",
            msg
        );
        return Err(AgentError::MiddlewareError {
            middleware: "chain".to_string(),
            reason: msg,
        });
    }

    tracing::debug!(
        "[DEADLOCK] dispatch_tools: complete, {} results",
        results.len()
    );
    Ok(results)
}

/// 执行 before_tool 审批 + 并发工具调用，收集所有结果。
///
/// **不变量**：调用期间 state 中不包含本轮 AI 消息。所有 `run_on_error` /
/// `run_after_tool` 实现均不依赖 `state.messages()` 包含本轮新增内容
/// （已验证：全部 17 个中间件的这些钩子均使用 `_state: &mut S` 模式）。
/// 新增中间件时必须遵守此约束。
///
/// 不写入 state，由 `dispatch_tools` 统一写入。
///
/// 返回 `(results, was_cancelled, deferred_error)`。
/// - 正常路径：`(results, false, None)`
/// - Cancel 路径：`(results, true, None)`
/// - after_tool 错误：`(results, false, Some(msg))`
/// - before_tool 错误 / Cancel in before_tool：返回 `Err`（state 未修改）
async fn collect_tool_results<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    original_calls: Vec<ToolCall>,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
    ai_msg_id: MessageId,
) -> AgentResult<(Vec<(ToolCall, ToolResult)>, bool, Option<String>)> {
    let mut ready_calls: Vec<ToolCall> = Vec::with_capacity(original_calls.len());
    let mut settled_results: Vec<(ToolCall, ToolResult)> = Vec::new();

    // 阶段一：批量 before_tool
    let before_results = agent
        .chain
        .run_before_tools_batch(state, original_calls.clone())
        .await;

    for (tool_call, before_result) in original_calls.iter().zip(before_results) {
        // before_tool 阶段也检查取消
        if cancel.is_cancelled() {
            // 为已 emit ToolStart 的 ready_calls 补发 ToolEnd，
            // 避免 TUI 的 pending_tools 短暂残留
            for tc in &ready_calls {
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    output: "interrupted by user".to_string(),
                    is_error: true,
                    source_agent_id: None,
                });
            }
            return Err(AgentError::Interrupted);
        }
        match before_result {
            Ok(modified_call) => {
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: modified_call.id.clone(),
                    name: modified_call.name.clone(),
                    input: modified_call.input.clone(),
                    source_agent_id: None,
                });
                ready_calls.push(modified_call);
            }
            Err(AgentError::ToolRejected { ref reason, .. }) => {
                let rejection_result =
                    ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: tool_call.input.clone(),
                    source_agent_id: None,
                });
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    output: rejection_result.output.clone(),
                    is_error: true,
                    source_agent_id: None,
                });
                settled_results.push((tool_call.clone(), rejection_result));
            }
            Err(e) => {
                let _ = agent.chain.run_on_error(state, &e).await;
                // 为已 emit ToolStart 的 ready_calls 补发 ToolEnd
                for tc in &ready_calls {
                    agent.emit(AgentEvent::ToolEnd {
                        message_id: ai_msg_id,
                        tool_call_id: tc.id.clone(),
                        name: tc.name.clone(),
                        output: e.to_string(),
                        is_error: true,
                        source_agent_id: None,
                    });
                }
                return Err(e);
            }
        }
    }

    // 阶段二：执行所有工具。
    // Agent 工具（SubAgent）顺序执行以避免共享 event_handler 中的锁竞争（如 Langfuse Mutex）
    // 导致的死锁；其他工具仍然并发执行以保持性能。
    let tool_results: Vec<Result<String, AgentError>> = {
        // 分离 Agent 调用和非 Agent 调用的索引
        let mut agent_indices: Vec<usize> = Vec::new();
        let mut other_indices: Vec<usize> = Vec::new();
        for (i, call) in ready_calls.iter().enumerate() {
            if call.name == "Agent" {
                agent_indices.push(i);
            } else {
                other_indices.push(i);
            }
        }

        let mut results: Vec<Option<Result<String, AgentError>>> =
            (0..ready_calls.len()).map(|_| None).collect();

        // 非工具调用：并发执行
        if !other_indices.is_empty() {
            let other_futures: Vec<_> = other_indices
                .iter()
                .map(|&i| {
                    let call = &ready_calls[i];
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
            let other_results = futures::future::join_all(other_futures).await;
            let mut other_results_iter = other_results.into_iter();
            for &i in &other_indices {
                results[i] = other_results_iter.next();
            }
        }

        // Agent 调用：顺序执行（避免并发 SubAgent 死锁）
        tracing::debug!(
            "[DEADLOCK] dispatch: {} Agent tools to execute sequentially, call_ids={:?}",
            agent_indices.len(),
            agent_indices
                .iter()
                .map(|&i| &ready_calls[i].id)
                .collect::<Vec<_>>()
        );
        for (agent_seq, &i) in agent_indices.iter().enumerate() {
            if cancel.is_cancelled() {
                let call = &ready_calls[i];
                tracing::debug!(
                    "[DEADLOCK] dispatch: agent_seq={} cancelled before start, call_id={}",
                    agent_seq,
                    call.id
                );
                results[i] = Some(Err(AgentError::ToolExecutionFailed {
                    tool: call.name.clone(),
                    reason: "interrupted by user".to_string(),
                }));
                continue;
            }
            let call = &ready_calls[i];
            let tool = all_tools.get(&call.name).copied();
            let tool_name = call.name.clone();
            let input = call.input.clone();
            let cancel_clone = cancel.clone();
            tracing::info!(
                "[DEADLOCK] dispatch: agent_seq={}/{} START invoke, call_id={}, subagent_type={}",
                agent_seq + 1,
                agent_indices.len(),
                call.id,
                input
                    .get("subagent_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(none)")
            );
            let invoke_start = std::time::Instant::now();
            let invoke_fut = async {
                match tool {
                    Some(t) => t
                        .invoke(input)
                        .await
                        .map_err(|e| AgentError::ToolExecutionFailed {
                            tool: tool_name.clone(),
                            reason: e.to_string(),
                        }),
                    None => Err(AgentError::ToolNotFound(tool_name)),
                }
            };
            let result = tokio::select! {
                biased;
                _ = cancel_clone.cancelled() => {
                    tracing::warn!(
                        "[DEADLOCK] dispatch: agent_seq={}/{} CANCELLED during invoke, call_id={}",
                        agent_seq + 1, agent_indices.len(), call.id
                    );
                    Err(AgentError::ToolExecutionFailed {
                        tool: ready_calls[i].name.clone(),
                        reason: "interrupted by user".to_string(),
                    })
                }
                r = invoke_fut => r,
            };
            let elapsed = invoke_start.elapsed();
            tracing::info!(
                "[DEADLOCK] dispatch: agent_seq={}/{} END invoke ({:.1?}), call_id={}, is_ok={}",
                agent_seq + 1,
                agent_indices.len(),
                elapsed,
                call.id,
                result.is_ok()
            );
            results[i] = Some(result);
        }

        // 将 Option<Result> 转为 Result（所有槽位都已填充）
        results.into_iter().map(|r| r.unwrap()).collect()
    };

    let was_cancelled = cancel.is_cancelled();

    // 阶段三：串行处理结果——所有 tool_result 收集到 results 中，
    // 不写 state，由 dispatch_tools 统一写入。
    // 工具执行错误不终止循环——错误 ToolResult 收集后由 LLM 下一轮修正。
    // after_tool 中间件错误收集到 deferred_error。
    let mut deferred_error: Option<String> = None;
    let mut exec_results: Vec<(ToolCall, ToolResult)> = Vec::with_capacity(ready_calls.len());

    for (modified_call, tool_result) in ready_calls.into_iter().zip(tool_results) {
        let result = match tool_result {
            Ok(output) => ToolResult::success(&modified_call.id, &modified_call.name, output),
            Err(AgentError::ToolNotFound(ref name)) => {
                tracing::warn!(tool.name = %name, "工具未找到，作为错误结果返回");
                ToolResult::error(
                    &modified_call.id,
                    &modified_call.name,
                    format!("工具 '{}' 不存在", name),
                )
            }
            Err(ref e) => {
                let _ = agent.chain.run_on_error(state, e).await;
                ToolResult::error(&modified_call.id, &modified_call.name, e.to_string())
            }
        };

        if result.is_error {
            tracing::warn!(
                tool.name = %result.tool_name,
                tool.is_error = true,
                error_len = result.output.len(),
                "tool call failed"
            );
        }
        agent.emit(AgentEvent::ToolEnd {
            message_id: ai_msg_id,
            tool_call_id: modified_call.id.clone(),
            name: modified_call.name.clone(),
            output: result.output.clone(),
            is_error: result.is_error,
            source_agent_id: None,
        });

        if modified_call.name == "Agent" {
            tracing::debug!(
                "[DEADLOCK] dispatch: about to run_after_tool for Agent, call_id={}",
                modified_call.id
            );
        }
        if let Err(e) = agent
            .chain
            .run_after_tool(state, &modified_call, &result)
            .await
        {
            let _ = agent.chain.run_on_error(state, &e).await;
            deferred_error = deferred_error.or(Some(e.to_string()));
        }
        if modified_call.name == "Agent" {
            tracing::debug!(
                "[DEADLOCK] dispatch: run_after_tool for Agent completed, call_id={}",
                modified_call.id
            );
        }

        exec_results.push((modified_call, result));
    }

    // 合并 settled（rejected）+ executed 结果
    settled_results.extend(exec_results);

    // Cancel / deferred_error 不在此返回 Err，由 dispatch_tools 在写入 state 后再检查
    Ok((settled_results, was_cancelled, deferred_error))
}

#[cfg(test)]
#[path = "tool_dispatch_test.rs"]
mod tests;
