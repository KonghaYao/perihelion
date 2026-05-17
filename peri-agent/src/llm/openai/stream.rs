use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::invoke::build_request_body;
use crate::agent::events::AgentEvent;
use crate::error::{AgentError, AgentResult};
use crate::llm::sse::SseParser;
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, MessageContent, ToolCallRequest};

/// 流式工具调用参数累积器（按 index 管理，处理多工具交错场景）
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments_fragments: Vec<String>,
}

/// OpenAI SSE 流式处理
///
/// 从 `invoke_streaming()` 中提取的流式解析逻辑，
/// 负责发送请求、解析 SSE 事件流、构建最终响应。
pub(super) async fn do_invoke_streaming(
    adapter: &super::ChatOpenAI,
    request: LlmRequest,
    ctx: StreamingContext,
) -> AgentResult<LlmResponse> {
    let msg_count = request.messages.len();
    let start = std::time::Instant::now();

    let body = build_request_body(adapter, &request, true);

    let chat_url = format!(
        "{}/chat/completions",
        adapter.base_url.trim_end_matches('/')
    );

    let resp = adapter
        .client
        .post(&chat_url)
        .bearer_auth(&adapter.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(
                provider = "openai", model = %adapter.model,
                elapsed_ms = start.elapsed().as_millis() as u64, error = %e,
                "LLM 流式网络请求失败"
            );
            AgentError::LlmError(e.to_string())
        })?;

    let status = resp.status();
    if !status.is_success() {
        let resp_text = resp.text().await.unwrap_or_default();
        let error_msg = serde_json::from_str::<Value>(&resp_text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "未知错误".to_string());
        tracing::error!(
            provider = "openai", model = %adapter.model, status = %status,
            error_message = %error_msg,
            elapsed_ms = start.elapsed().as_millis() as u64,
            msg_count,
            "LLM 流式 API 错误"
        );
        return Err(AgentError::LlmHttpError {
            status: status.as_u16(),
            message: format!("API 错误 {status}: {error_msg}"),
        });
    }

    let mut stream = resp.bytes_stream();
    let mut parser = SseParser::new();
    let mut reasoning_text = String::new();
    let mut content_text = String::new();
    let mut tool_accums: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();
    let mut finish_reason: Option<String> = None;
    let mut final_usage: Option<Value> = None;
    let mut stream_request_id: Option<String> = None;

    loop {
        // 在接收每个 SSE chunk 前检查取消（支持 Ctrl+C 中断长时间 LLM 调用）
        let chunk = tokio::select! {
            biased;
            _ = ctx.cancel.cancelled() => {
                tracing::info!(
                    provider = "openai",
                    model = %adapter.model,
                    "LLM streaming cancelled by user"
                );
                return Err(AgentError::Interrupted);
            }
            result = stream.next() => {
                match result {
                    Some(Ok(c)) => c,
                    Some(Err(e)) => return Err(AgentError::LlmError(format!("流式读取失败: {e}"))),
                    None => break,
                }
            }
        };

        for (_event_type, data) in parser.push(&chunk) {
            let parsed: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract request_id from first chunk
            if stream_request_id.is_none() {
                stream_request_id = parsed["id"].as_str().map(|s| s.to_string());
            }

            // Usage from last chunk (stream_options: include_usage)
            if let Some(u) = parsed["usage"].as_object() {
                final_usage = Some(json!(u));
            }

            let choices = match parsed["choices"].as_array() {
                Some(c) if !c.is_empty() => c,
                _ => continue,
            };

            let delta = &choices[0]["delta"];

            // Finish reason
            if let Some(fr) = choices[0]["finish_reason"].as_str() {
                if !fr.is_empty() {
                    finish_reason = Some(fr.to_string());
                }
            }

            // Reasoning delta (双字段兼容)
            if let Some(r) = delta["reasoning_content"]
                .as_str()
                .or_else(|| delta["reasoning"].as_str())
            {
                if !r.is_empty() {
                    ctx.event_handler
                        .on_event(AgentEvent::AiReasoning(r.to_string()));
                    reasoning_text.push_str(r);
                }
            }

            // Text delta
            if let Some(c) = delta["content"].as_str() {
                if !c.is_empty() {
                    ctx.event_handler.on_event(AgentEvent::TextChunk {
                        message_id: ctx.message_id,
                        chunk: c.to_string(),
                        source_agent_id: None,
                    });
                    content_text.push_str(c);
                }
            }

            // Tool call accumulation (multi-index interleaved)
            if let Some(tc_array) = delta["tool_calls"].as_array() {
                for tc in tc_array {
                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                    let acc = tool_accums
                        .entry(idx)
                        .or_insert_with(|| ToolCallAccumulator {
                            id: None,
                            name: None,
                            arguments_fragments: Vec::new(),
                        });
                    if let Some(id) = tc["id"].as_str() {
                        acc.id = Some(id.to_string());
                    }
                    if let Some(name) = tc["function"]["name"].as_str() {
                        acc.name = Some(name.to_string());
                    }
                    if let Some(args) = tc["function"]["arguments"].as_str() {
                        acc.arguments_fragments.push(args.to_string());
                    }
                }
            }
        }

        if parser.is_done() {
            break;
        }
    }

    // Build tool calls from accumulators
    let tool_call_requests: Vec<ToolCallRequest> = tool_accums
        .values()
        .filter_map(|acc| {
            let id = acc.id.clone()?;
            let name = acc.name.clone()?;
            let args_str = acc.arguments_fragments.join("");
            let arguments = match serde_json::from_str::<Value>(&args_str) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(
                        tool = name,
                        raw_args = %args_str,
                        "流式工具调用参数 JSON 解析失败，使用空对象"
                    );
                    serde_json::json!({"_raw_arguments": args_str})
                }
            };
            Some(ToolCallRequest::new(id, name, arguments))
        })
        .collect();

    // Build content blocks
    let mut blocks: Vec<ContentBlock> = Vec::new();

    if !reasoning_text.is_empty() {
        blocks.push(ContentBlock::reasoning(&reasoning_text));
    }

    let stop_reason = StopReason::from_openai(finish_reason.as_deref().unwrap_or("stop"));

    if stop_reason == StopReason::ToolUse {
        for tc in &tool_call_requests {
            blocks.push(ContentBlock::tool_use(
                &tc.id,
                &tc.name,
                tc.arguments.clone(),
            ));
        }
        if content_text.is_empty() && blocks.is_empty() {
            blocks.push(ContentBlock::text(""));
        }
        let content = MessageContent::Blocks(blocks);
        let source_message = BaseMessage::ai_with_tool_calls(content, tool_call_requests);

        let usage = final_usage.as_ref().and_then(|u| {
            let input = u["prompt_tokens"].as_u64().map(|v| v as u32);
            let output = u["completion_tokens"].as_u64().map(|v| v as u32);
            let cache_read = u["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .map(|v| v as u32);
            match (input, output) {
                (Some(i), Some(o)) => Some(crate::llm::types::TokenUsage {
                    input_tokens: i,
                    output_tokens: o,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: cache_read,
                    request_id: stream_request_id.clone(),
                }),
                _ => None,
            }
        });

        Ok(LlmResponse {
            message: source_message,
            stop_reason,
            usage,
            request_id: stream_request_id,
        })
    } else {
        if !content_text.is_empty() {
            blocks.push(ContentBlock::text(&content_text));
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::text(""));
        }
        let content = if blocks.len() == 1 && blocks[0].as_text().is_some() {
            MessageContent::text(content_text)
        } else {
            MessageContent::Blocks(blocks)
        };
        let source_message = BaseMessage::ai(content);

        let usage = final_usage.as_ref().and_then(|u| {
            let input = u["prompt_tokens"].as_u64().map(|v| v as u32);
            let output = u["completion_tokens"].as_u64().map(|v| v as u32);
            let cache_read = u["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .map(|v| v as u32);
            match (input, output) {
                (Some(i), Some(o)) => Some(crate::llm::types::TokenUsage {
                    input_tokens: i,
                    output_tokens: o,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: cache_read,
                    request_id: stream_request_id.clone(),
                }),
                _ => None,
            }
        });

        Ok(LlmResponse {
            message: source_message,
            stop_reason,
            usage,
            request_id: stream_request_id,
        })
    }
}
