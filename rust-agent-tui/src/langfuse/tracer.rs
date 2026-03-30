use std::collections::HashMap;
use std::sync::Arc;

use langfuse_client::{
    GenerationBody, IngestionEvent, ObservationBody,
    ObservationType, SpanBody, TraceBody,
};
use rust_create_agent::llm::types::TokenUsage;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::tools::ToolDefinition;

use super::session::LangfuseSession;

/// 工具调用的中间缓冲数据（start 时存储，end 时取出组合成完整 span-create）
struct PendingTool {
    span_id: String,
    name: String,
    input: serde_json::Value,
    start_time: String,
    /// 父 span ID（= 所属批次的 tools_batch_span_id）
    parent_span_id: String,
}

/// Langfuse 单轮追踪器（per-turn）
///
/// 持有对 LangfuseSession 的引用，复用 client/batcher/session_id。
/// 生命周期：从 submit_message() 开始 → AgentEvent::Done/Error 时结束。
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,
    /// 当前对话轮次的 Trace ID（提前生成，所有观测对象共享）
    trace_id: String,
    /// Agent Observation 的 ID，所有子观测通过 parent_observation_id 挂在此下
    agent_span_id: String,
    /// step → (generation_id, input_messages, tools, start_time_rfc3339)
    generation_data: HashMap<usize, (String, Vec<BaseMessage>, Vec<ToolDefinition>, String)>,
    /// 工具调用缓冲数据：tool_call_id → PendingTool
    pending_tools: HashMap<String, PendingTool>,
    /// 当前批次工具组 Span ID
    tools_batch_span_id: Option<String>,
    /// 当前批次工具组开始时间
    tools_batch_start_time: Option<String>,
    /// 当前批次工具组最后一次 ToolEnd 时间
    tools_batch_end_time: Option<String>,
    /// 所有后台 spawn 的 JoinHandle
    pending_handles: Vec<tokio::task::JoinHandle<()>>,
    /// 累积的最终回答
    final_answer: String,
}

impl LangfuseTracer {
    /// 从共享 Session 构造 per-turn Tracer
    pub fn new(session: Arc<LangfuseSession>) -> Self {
        Self {
            session,
            trace_id: uuid::Uuid::now_v7().to_string(),
            agent_span_id: uuid::Uuid::now_v7().to_string(),
            generation_data: HashMap::new(),
            pending_tools: HashMap::new(),
            tools_batch_span_id: None,
            tools_batch_start_time: None,
            tools_batch_end_time: None,
            pending_handles: Vec::new(),
            final_answer: String::new(),
        }
    }

    /// TextChunk 事件：累积最终回答
    pub fn on_text_chunk(&mut self, chunk: &str) {
        self.final_answer.push_str(chunk);
    }

    /// 提交当前批次 Tools Span
    fn flush_tools_batch(&mut self) {
        if let (Some(batch_id), Some(batch_start), Some(batch_end)) = (
            self.tools_batch_span_id.take(),
            self.tools_batch_start_time.take(),
            self.tools_batch_end_time.take(),
        ) {
            let batcher = Arc::clone(&self.session.batcher);
            let trace_id = self.trace_id.clone();
            let agent_span_id = self.agent_span_id.clone();
            let handle = tokio::spawn(async move {
                let body = SpanBody {
                    id: Some(batch_id),
                    trace_id: Some(trace_id.clone()),
                    name: Some("Tools".to_string()),
                    start_time: Some(batch_start),
                    end_time: Some(batch_end.clone()),
                    parent_observation_id: Some(agent_span_id),
                    input: None,
                    output: None,
                    status_message: None,
                    metadata: None,
                    level: None,
                    version: None,
                    environment: None,
                };
                let event = IngestionEvent::SpanCreate {
                    id: uuid::Uuid::now_v7().to_string(),
                    timestamp: batch_end,
                    body,
                    metadata: None,
                };
                if let Err(e) = batcher.add(event).await {
                    tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: tools batch span 入队失败（背压丢弃）");
                }
            });
            self.pending_handles.push(handle);
        }
    }

    /// 对话轮次开始：创建 Trace + Agent Observation
    pub fn on_trace_start(&mut self, input: &str) {
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let agent_span_id = self.agent_span_id.clone();
        let input = input.to_string();
        let session_id = self.session.session_id.clone();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let handle = tokio::spawn(async move {
            // 创建 Trace
            let trace_body = TraceBody {
                id: Some(trace_id.clone()),
                name: Some("agent-run".to_string()),
                input: Some(serde_json::json!(input.clone())),
                session_id: Some(session_id),
                ..Default::default()
            };
            let trace_event = IngestionEvent::TraceCreate {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: start_time.clone(),
                body: trace_body,
                metadata: None,
            };
            if let Err(e) = batcher.add(trace_event).await {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 创建失败");
            }

            // 创建 Agent Observation
            let timestamp_obs = start_time.clone();
            let body = ObservationBody {
                id: Some(agent_span_id),
                trace_id: Some(trace_id.clone()),
                r#type: ObservationType::Agent,
                name: Some("Agent".to_string()),
                input: Some(serde_json::json!(input)),
                start_time: Some(start_time),
                end_time: None,
                completion_start_time: None,
                parent_observation_id: None,
                output: None,
                metadata: None,
                model: None,
                model_parameters: None,
                level: None,
                status_message: None,
                version: None,
                environment: None,
            };
            let event = IngestionEvent::ObservationCreate {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: timestamp_obs,
                body,
                metadata: None,
            };
            if let Err(e) = batcher.add(event).await {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: agent observation 入队失败（背压丢弃）");
            }
        });
        self.pending_handles.push(handle);
    }

    /// LLM 调用开始：提交上一轮工具批次 Span，缓存本轮 input
    pub fn on_llm_start(&mut self, step: usize, messages: &[BaseMessage], tools: &[ToolDefinition]) {
        self.flush_tools_batch();
        let gen_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.generation_data
            .insert(step, (gen_id, messages.to_vec(), tools.to_vec(), start_time));
    }

    /// LLM 调用结束
    pub fn on_llm_end(
        &mut self,
        step: usize,
        model: &str,
        provider: &str,
        output: &str,
        usage: Option<&TokenUsage>,
    ) {
        let Some((gen_id, messages, tools, start_time)) = self.generation_data.remove(&step) else {
            return;
        };
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let agent_span_id = self.agent_span_id.clone();
        let model = model.to_string();
        let provider_name = provider.to_string();
        let output = output.to_string();

        let messages_val = serde_json::to_value(&messages).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: messages 序列化失败");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let tools_val = serde_json::to_value(&tools).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: tools 序列化失败");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let input_json = serde_json::json!({
            "messages": messages_val,
            "tools": tools_val,
        });

        let langfuse_usage_details: Option<HashMap<String, i32>> = usage.map(|u| {
            let mut map = HashMap::new();
            let cache_creation = u.cache_creation_input_tokens.unwrap_or(0);
            let cache_read = u.cache_read_input_tokens.unwrap_or(0);
            let total = u.input_tokens + u.output_tokens + cache_creation + cache_read;
            map.insert("input".to_string(), u.input_tokens as i32);
            map.insert("output".to_string(), u.output_tokens as i32);
            map.insert("total".to_string(), total as i32);
            if cache_creation > 0 {
                map.insert("cache_creation_input_tokens".to_string(), cache_creation as i32);
            }
            if cache_read > 0 {
                map.insert("cache_read_input_tokens".to_string(), cache_read as i32);
            }
            map
        });

        let handle = tokio::spawn(async move {
            let timestamp = end_time.clone();
            let body = GenerationBody {
                id: Some(gen_id.clone()),
                trace_id: Some(trace_id.clone()),
                name: Some(format!("Chat{}", provider_name)),
                input: Some(input_json),
                output: Some(serde_json::json!(output)),
                model: Some(model),
                usage_details: langfuse_usage_details,
                parent_observation_id: Some(agent_span_id),
                start_time: Some(start_time),
                end_time: Some(end_time),
                ..Default::default()
            };
            let event = IngestionEvent::GenerationCreate {
                id: gen_id.clone(),
                timestamp,
                body,
                metadata: None,
            };
            if let Err(e) = batcher.add(event).await {
                tracing::warn!(error = %e, trace_id = %trace_id, gen_id = %gen_id, "langfuse: generation 入队失败（背压丢弃）");
            }
        });
        self.pending_handles.push(handle);
    }

    /// 工具调用开始
    pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
        if self.pending_tools.is_empty() {
            self.tools_batch_span_id = Some(uuid::Uuid::now_v7().to_string());
            self.tools_batch_start_time = Some(
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            );
        }
        let parent_span_id = self.tools_batch_span_id
            .clone()
            .unwrap_or_else(|| self.agent_span_id.clone());

        let span_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.pending_tools.insert(tool_call_id.to_string(), PendingTool {
            span_id,
            name: name.to_string(),
            input: input.clone(),
            start_time,
            parent_span_id,
        });
    }

    /// 工具调用结束
    pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool) {
        let Some(tool) = self.pending_tools.remove(tool_call_id) else {
            return;
        };
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let output = output.to_string();
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        {
            let batcher = Arc::clone(&batcher);
            let trace_id_log = trace_id.clone();
            let tool_name_log = tool.name.clone();
            let end_time = end_time.clone();
            let handle = tokio::spawn(async move {
                let status_msg = if is_error { Some("error".to_string()) } else { None };
                let body = SpanBody {
                    id: Some(tool.span_id),
                    trace_id: Some(trace_id_log.clone()),
                    name: Some(tool.name),
                    input: Some(tool.input),
                    output: Some(serde_json::json!(output)),
                    start_time: Some(tool.start_time),
                    end_time: Some(end_time.clone()),
                    parent_observation_id: Some(tool.parent_span_id),
                    status_message: status_msg,
                    metadata: None,
                    level: None,
                    version: None,
                    environment: None,
                };
                let event = IngestionEvent::SpanCreate {
                    id: uuid::Uuid::now_v7().to_string(),
                    timestamp: end_time,
                    body,
                    metadata: None,
                };
                if let Err(e) = batcher.add(event).await {
                    tracing::warn!(error = %e, trace_id = %trace_id_log, tool = %tool_name_log, "langfuse: tool span 入队失败（背压丢弃）");
                }
            });
            self.pending_handles.push(handle);
        }

        self.tools_batch_end_time = Some(end_time);
    }

    /// 对话轮次结束：更新 Trace 输出，并强制 flush。
    pub fn on_trace_end(&mut self, error_output: Option<&str>) -> tokio::task::JoinHandle<()> {
        self.flush_tools_batch();

        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let output = if let Some(err) = error_output {
            err.to_string()
        } else {
            std::mem::take(&mut self.final_answer)
        };
        let handles = std::mem::take(&mut self.pending_handles);

        tokio::spawn(async move {
            for h in handles {
                let _ = h.await;
            }
            let trace_body = TraceBody {
                id: Some(trace_id.clone()),
                name: Some("agent-run".to_string()),
                output: Some(serde_json::json!(output)),
                ..Default::default()
            };
            let trace_event = IngestionEvent::TraceCreate {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                body: trace_body,
                metadata: None,
            };
            if let Err(e) = batcher.add(trace_event).await {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 输出更新失败");
            }
            if let Err(e) = batcher.flush().await {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: batcher flush 失败");
            }
        })
    }
}

impl Drop for LangfuseTracer {
    fn drop(&mut self) {
        if !self.pending_handles.is_empty() {
            tracing::warn!(
                trace_id = %self.trace_id,
                count = self.pending_handles.len(),
                "LangfuseTracer dropped with pending handles — on_trace_end was not called, Langfuse data may be incomplete"
            );
        }
    }
}
