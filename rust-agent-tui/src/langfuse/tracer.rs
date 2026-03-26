use std::collections::HashMap;
use std::sync::Arc;

use langfuse_client_base::models::{
    ingestion_event_one_of_2,
    ingestion_event_one_of_4::Type as GenType,
    ingestion_event_one_of_8,
    CreateGenerationBody, CreateSpanBody, IngestionEvent,
    IngestionEventOneOf2, IngestionEventOneOf4, IngestionEventOneOf8,
    ObservationBody, ObservationType, UsageDetails,
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
/// 持有对 `LangfuseSession` 的引用，复用 client/batcher/session_id。
/// 生命周期：从 submit_message() 开始 → AgentEvent::Done/Error 时结束。
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,
    /// 当前对话轮次的 Trace ID（提前生成，所有观测对象共享）
    trace_id: String,
    /// Agent Observation 的 ID，所有子观测通过 parent_observation_id 挂在此下
    agent_span_id: String,
    /// step → (generation_id, input_messages, tools, start_time_rfc3339)
    generation_data: HashMap<usize, (String, Vec<BaseMessage>, Vec<ToolDefinition>, String)>,
    /// 工具调用缓冲数据：tool_call_id → PendingTool（start 时写入，end 时取出合并上报）
    pending_tools: HashMap<String, PendingTool>,
    /// 当前批次工具组 Span ID（第一个 ToolStart 时生成，下轮 LLM 开始或 trace 结束时提交）
    tools_batch_span_id: Option<String>,
    /// 当前批次工具组开始时间
    tools_batch_start_time: Option<String>,
    /// 当前批次工具组最后一次 ToolEnd 时间（延迟到 flush 时使用）
    tools_batch_end_time: Option<String>,
    /// 所有后台 spawn 的 JoinHandle；on_trace_end 统一 join 后再 flush，
    /// 消除 sleep(200ms) 竞态：确保所有 batcher.add 完成后才 flush。
    pending_handles: Vec<tokio::task::JoinHandle<()>>,
    /// 累积的最终回答（通过 on_text_chunk 逐块追加，on_trace_end 时上报）
    final_answer: String,
}

impl LangfuseTracer {
    /// 从共享 Session 构造 per-turn Tracer（同步）
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

    /// TextChunk 事件：累积最终回答（on_trace_end 时作为 Trace output 上报）
    pub fn on_text_chunk(&mut self, chunk: &str) {
        self.final_answer.push_str(chunk);
    }

    /// 提交当前批次 Tools Span（如果存在）。
    ///
    /// 延迟提交策略：批次 Span 不在最后一个 ToolEnd 时立即提交，而是在
    /// 下一轮 LLM 调用开始（on_llm_start）或 trace 结束（on_trace_end）时提交。
    /// 这样可以正确处理 HITL 拒绝场景：拒绝的工具会立即发出 ToolStart+ToolEnd，
    /// 导致 pending_tools 暂时为空，若立即提交批次则会错误地分裂同一 LLM 轮次的工具组。
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
                let body = CreateSpanBody {
                    id: Some(Some(batch_id)),
                    trace_id: Some(Some(trace_id.clone())),
                    name: Some(Some("Tools".to_string())),
                    start_time: Some(Some(batch_start)),
                    end_time: Some(Some(batch_end.clone())),
                    parent_observation_id: Some(Some(agent_span_id)),
                    input: None,
                    output: None,
                    status_message: None,
                    metadata: None,
                    level: None,
                    version: None,
                    environment: None,
                };
                let event = IngestionEventOneOf2 {
                    id: uuid::Uuid::now_v7().to_string(),
                    timestamp: batch_end,
                    body: Box::new(body),
                    r#type: ingestion_event_one_of_2::Type::SpanCreate,
                    metadata: None,
                };
                if let Err(e) = batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event))).await {
                    tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: tools batch span 入队失败（背压丢弃）");
                }
            });
            self.pending_handles.push(handle);
        }
    }

    /// 对话轮次开始：创建 Trace + Agent Observation（session_id 从共享 Session 读取）
    pub fn on_trace_start(&mut self, input: &str) {
        let client = Arc::clone(&self.session.client);
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let agent_span_id = self.agent_span_id.clone();
        let input = input.to_string();
        let session_id = self.session.session_id.clone();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let handle = tokio::spawn(async move {
            // 创建 Trace
            if let Err(e) = client
                .trace()
                .id(trace_id.clone())
                .name("agent-run")
                .input(serde_json::json!(input.clone()))
                .session_id(session_id)
                .call()
                .await
            {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: trace 创建失败");
            }

            // 创建 Agent Observation（包裹整个 ReAct 循环）
            let body = ObservationBody {
                id: Some(Some(agent_span_id)),
                trace_id: Some(Some(trace_id.clone())),
                r#type: ObservationType::Agent,
                name: Some(Some("Agent".to_string())),
                input: Some(Some(serde_json::json!(input))),
                start_time: Some(Some(start_time.clone())),
                ..Default::default()
            };
            let event = IngestionEventOneOf8 {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: start_time,
                body: Box::new(body),
                r#type: ingestion_event_one_of_8::Type::ObservationCreate,
                metadata: None,
            };
            if let Err(e) = batcher
                .add(IngestionEvent::IngestionEventOneOf8(Box::new(event)))
                .await
            {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: agent observation 入队失败（背压丢弃）");
            }
        });
        self.pending_handles.push(handle);
    }

    /// LLM 调用开始：提交上一轮工具批次 Span，缓存本轮 input messages/tools/start_time
    pub fn on_llm_start(&mut self, step: usize, messages: &[BaseMessage], tools: &[ToolDefinition]) {
        // 上一轮工具批次在此时已全部执行完毕，可以安全提交批次 Span
        // （延迟策略：避免 HITL 拒绝导致 pending_tools 提前清空而分裂批次）
        self.flush_tools_batch();
        let gen_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.generation_data
            .insert(step, (gen_id, messages.to_vec(), tools.to_vec(), start_time));
    }

    /// LLM 调用结束：通过 Batcher 直接构造 IngestionEvent，绕过 builder 的 usage bug
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
        // 显式序列化，失败时降级为描述性错误对象并记录 warn，而不是静默降级为 null
        let messages_val = serde_json::to_value(&messages).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: messages 序列化失败，降级为错误占位");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let tools_val = serde_json::to_value(&tools).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: tools 序列化失败，降级为错误占位");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let input_json = serde_json::json!({
            "messages": messages_val,
            "tools": tools_val,
        });

        // 构造 UsageDetails HashMap（新 API，支持缓存 token）
        let langfuse_usage_details = usage.map(|u| {
            let mut map = std::collections::HashMap::new();
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
            Box::new(UsageDetails::Object(map))
        });

        let handle = tokio::spawn(async move {
            let timestamp = end_time.clone();
            let body = CreateGenerationBody {
                id: Some(Some(gen_id.clone())),
                trace_id: Some(Some(trace_id.clone())),
                name: Some(Some(format!("Chat{}", provider_name))),
                input: Some(Some(input_json)),
                output: Some(Some(serde_json::json!(output))),
                model: Some(Some(model)),
                usage_details: langfuse_usage_details,
                parent_observation_id: Some(Some(agent_span_id)),
                start_time: Some(Some(start_time)),
                end_time: Some(Some(end_time)),
                ..Default::default()
            };
            let event = IngestionEventOneOf4 {
                id: gen_id.clone(),
                timestamp,
                body: Box::new(body),
                r#type: GenType::GenerationCreate,
                metadata: None,
            };
            if let Err(e) = batcher
                .add(IngestionEvent::IngestionEventOneOf4(Box::new(event)))
                .await
            {
                tracing::warn!(error = %e, trace_id = %trace_id, gen_id = %gen_id, "langfuse: generation 入队失败（背压丢弃）");
            }
        });
        self.pending_handles.push(handle);
    }

    /// 工具调用开始：缓存数据，等 on_tool_end 时合并成完整 span-create
    /// 第一个工具到来时初始化批次 Tools span，后续工具归属同一批次
    pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
        // 第一个工具：初始化批次 Tools span
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

    /// 工具调用结束：按 tool_call_id 取出缓冲数据，提交单个工具 Span
    pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool) {
        let Some(tool) = self.pending_tools.remove(tool_call_id) else {
            return;
        };
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let output = output.to_string();
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // 提交单个工具 Span（parent = 所属 Tools batch span）
        {
            let batcher = Arc::clone(&batcher);
            let trace_id_log = trace_id.clone();
            let tool_name_log = tool.name.clone();
            let end_time = end_time.clone();
            let handle = tokio::spawn(async move {
                let status_msg = if is_error { Some(Some("error".to_string())) } else { None };
                let body = CreateSpanBody {
                    id: Some(Some(tool.span_id)),
                    trace_id: Some(Some(trace_id_log.clone())),
                    name: Some(Some(tool.name)),
                    input: Some(Some(tool.input)),
                    output: Some(Some(serde_json::json!(output))),
                    start_time: Some(Some(tool.start_time)),
                    end_time: Some(Some(end_time.clone())),
                    parent_observation_id: Some(Some(tool.parent_span_id)),
                    status_message: status_msg,
                    metadata: None,
                    level: None,
                    version: None,
                    environment: None,
                };
                let event = IngestionEventOneOf2 {
                    id: uuid::Uuid::now_v7().to_string(),
                    timestamp: end_time,
                    body: Box::new(body),
                    r#type: ingestion_event_one_of_2::Type::SpanCreate,
                    metadata: None,
                };
                if let Err(e) = batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event))).await {
                    tracing::warn!(error = %e, trace_id = %trace_id_log, tool = %tool_name_log, "langfuse: tool span 入队失败（背压丢弃）");
                }
            });
            self.pending_handles.push(handle);
        }

        // 更新批次最后结束时间（延迟提交策略：批次 Span 在下轮 LLM 开始或 trace 结束时提交）
        // 不在此处判断 pending_tools.is_empty() 来提交，避免 HITL 拒绝路径导致批次分裂：
        // 拒绝的工具会立即发出 ToolStart+ToolEnd，使 pending_tools 瞬间清空，
        // 若立即提交批次，后续正常执行的工具将开启第二个批次，造成 Langfuse span 树断裂。
        self.tools_batch_end_time = Some(end_time);
    }

    /// 对话轮次结束：更新 Trace 的最终输出，并强制 flush。
    ///
    /// 返回 flush 任务的 `JoinHandle`，调用方应保存该 handle 并在进程退出前 await，
    /// 确保 batcher flush 在 tokio runtime 关闭前完成。
    ///
    /// 使用 pending_handles join 模式替代固定 sleep，确保所有 batcher.add 完成后才 flush。
    /// `error_output` 非 None 时表示以错误结束，优先使用错误信息作为输出。
    pub fn on_trace_end(&mut self, error_output: Option<&str>) -> tokio::task::JoinHandle<()> {
        // 提交最后一轮的工具批次 Span（若 Agent 以最终回答结束而非再次调用 LLM，
        // 则上一轮工具批次不会被 on_llm_start 触发，需在此处兜底提交）
        self.flush_tools_batch();

        let client = Arc::clone(&self.session.client);
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let output = if let Some(err) = error_output {
            err.to_string()
        } else {
            std::mem::take(&mut self.final_answer)
        };
        // 取出所有待 join 的 handle（含 flush_tools_batch 新增的），避免 on_trace_end 的 spawn 持有 &mut self
        let handles = std::mem::take(&mut self.pending_handles);

        tokio::spawn(async move {
            // 等待所有后台 spawn（on_trace_start/on_llm_end/on_tool_end）完成 batcher.add
            for h in handles {
                let _ = h.await;
            }
            // 更新 Trace 输出
            if let Err(e) = client
                .trace()
                .id(trace_id.clone())
                .name("agent-run")
                .output(serde_json::json!(output))
                .call()
                .await
            {
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
