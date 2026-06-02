use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::{
    agent::{
        events::AgentEvent,
        react::{ReactLLM, Reasoning},
        state::State,
    },
    error::{AgentError, AgentResult},
    llm::types::StreamingContext,
    messages::MessageId,
    tools::BaseTool,
};

use super::ReActAgent;

/// LLM 推理 + Token 预算监控
pub(crate) async fn call_llm<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    tool_refs: &[&dyn BaseTool],
    step: usize,
    cancel: &CancellationToken,
) -> AgentResult<Reasoning> {
    // ── LLM 推理（与 cancel 竞争）────────────────────────────────────
    agent.emit(AgentEvent::LlmCallStart {
        step,
        messages: Arc::new(state.messages().to_vec()),
        tools: tool_refs.iter().map(|t| t.definition()).collect(),
    });

    // 构建 StreamingContext：若 agent 有 event_handler 则启用流式
    let message_id = MessageId::new();
    let streaming = agent.event_handler.as_ref().map(|h| StreamingContext {
        event_handler: Arc::clone(h),
        message_id,
        cancel: cancel.clone(),
    });

    let reasoning = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Err(AgentError::Interrupted);
        }
        result = agent.llm.generate_reasoning(state.messages(), tool_refs, streaming) => {
            match result {
                Ok(r) => r,
                Err(e) => {
                        tracing::error!(
                            step,
                            model = %agent.llm.model_name(),
                            error = %e,
                            "LLM generate_reasoning 失败"
                        );
                        // LLM 报错时仍 emit LlmCallEnd，确保 Langfuse generation 可观测
                    agent.emit(AgentEvent::LlmCallEnd {
                        step,
                        model: agent.llm.model_name(),
                        output: format!("ERROR: {}", e),
                        usage: None,
                        stop_reason: None,
                    });
                    agent.chain.run_on_error(state, &e).await?;
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
        agent.emit(AgentEvent::LlmCallEnd {
            step,
            model: agent.llm.model_name(),
            output: llm_output,
            usage: reasoning.usage.clone(),
            stop_reason: Some(reasoning.stop_reason.clone()),
        });
        // 自动累积 token 用量到 state
        if let Some(ref usage) = reasoning.usage {
            state.token_tracker_mut().accumulate(usage);
            // 使用 ContextBudget（若已设置）进行上下文用量监控
            if let Some(ref budget) = agent.context_budget {
                let tracker = state.token_tracker();
                if let Some(pct_used) = tracker.estimated_context_tokens() {
                    let should_warn = budget.should_warn(tracker);
                    if should_warn {
                        tracing::warn!(
                            used_tokens = ?tracker.estimated_context_tokens(),
                            total_tokens = budget.context_window,
                            percentage = tracker.context_usage_percent(budget.context_window).map(|p| p as u32).unwrap_or(0),
                            model = %agent.llm.model_name(),
                            step,
                            "context 接近上限"
                        );
                    }
                    if should_warn || budget.should_auto_compact(tracker) {
                        if let Some(percentage) =
                            tracker.context_usage_percent(budget.context_window)
                        {
                            agent.emit(AgentEvent::ContextWarning {
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
                let total = agent.llm.context_window();
                if let Some(used) = tracker.estimated_context_tokens() {
                    let pct = used as f64 / total as f64 * 100.0;
                    let exceeded = pct as u32 >= 80;
                    if exceeded {
                        tracing::warn!(
                            used_tokens = used,
                            total_tokens = total,
                            percentage = pct as u32,
                            model = %agent.llm.model_name(),
                            step,
                            "context 接近上限"
                        );
                        agent.emit(AgentEvent::ContextWarning {
                            used_tokens: used,
                            total_tokens: total as u64,
                            percentage: pct,
                        });
                    }
                }
            }
        }
    }

    Ok(reasoning)
}
