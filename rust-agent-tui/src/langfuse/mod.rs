pub mod config;
pub use config::LangfuseConfig;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use langfuse_client_base::models::{
    ingestion_event_one_of_2,
    ingestion_event_one_of_4::Type as GenType,
    ingestion_event_one_of_8,
    CreateGenerationBody, CreateSpanBody, IngestionEvent,
    IngestionEventOneOf2, IngestionEventOneOf4, IngestionEventOneOf8,
    ObservationBody, ObservationType, UsageDetails,
};
use langfuse_ergonomic::{BackpressurePolicy, Batcher, ClientBuilder, LangfuseClient};
use rust_create_agent::llm::types::TokenUsage;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::tools::ToolDefinition;

/// Langfuse Thread 级别会话，持有跨多轮复用的共享连接状态。
///
/// 生命周期：Thread 创建/打开时构造，new_thread()/open_thread() 时重置（= None）。
/// 同一 Thread 内所有 `LangfuseTracer` 共享同一个 client + batcher + session_id。
pub struct LangfuseSession {
    pub client: Arc<LangfuseClient>,
    pub batcher: Arc<Batcher>,
    /// session_id = thread_id，Thread 内所有 Trace 共享
    pub session_id: String,
}

impl LangfuseSession {
    /// 从配置和 session_id 构造 Session，失败时返回 None（静默降级）
    pub async fn new(config: LangfuseConfig, session_id: String) -> Option<Self> {
        let client = ClientBuilder::new()
            .public_key(config.public_key)
            .secret_key(config.secret_key)
            .base_url(config.host)
            .build()
            .ok()?;

        // max_events=50: 每批最多 50 个事件
        // flush_interval=10s: 10 秒自动 flush 一次
        // backpressure_policy=DropNew: 队列满时丢弃新事件，避免 OOM
        let batcher = Batcher::builder()
            .client(client.clone())
            .max_events(50)
            .flush_interval(Duration::from_secs(10))
            .backpressure_policy(BackpressurePolicy::DropNew)
            .build()
            .await;

        Some(Self {
            client: Arc::new(client),
            batcher: Arc::new(batcher),
            session_id,
        })
    }
}

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
/// 生命周期：从 submit_message() 开始 → AgentEvent::Done 时结束。
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
    /// 当前批次工具组 Span ID（第一个 ToolStart 时生成，最后一个 ToolEnd 时随批次 Span 一起提交）
    tools_batch_span_id: Option<String>,
    /// 当前批次工具组开始时间
    tools_batch_start_time: Option<String>,
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

        tokio::spawn(async move {
            // 创建 Trace
            let _ = client
                .trace()
                .id(trace_id.clone())
                .name("agent-run")
                .input(serde_json::json!(input.clone()))
                .session_id(session_id)
                .call()
                .await;

            // 创建 Agent Observation（包裹整个 ReAct 循环）
            let body = ObservationBody {
                id: Some(Some(agent_span_id)),
                trace_id: Some(Some(trace_id)),
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
            let _ = batcher
                .add(IngestionEvent::IngestionEventOneOf8(Box::new(event)))
                .await;
        });
    }

    /// LLM 调用开始：缓存 input messages、工具定义和开始时间戳，等 on_llm_end 时一并上报 Generation
    pub fn on_llm_start(&mut self, step: usize, messages: &[BaseMessage], tools: &[ToolDefinition]) {
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
        let input_json = serde_json::json!({
            "messages": messages,
            "tools": tools,
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

        tokio::spawn(async move {
            let timestamp = end_time.clone();
            let body = CreateGenerationBody {
                id: Some(Some(gen_id.clone())),
                trace_id: Some(Some(trace_id)),
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
                id: gen_id,
                timestamp,
                body: Box::new(body),
                r#type: GenType::GenerationCreate,
                metadata: None,
            };
            let _ = batcher
                .add(IngestionEvent::IngestionEventOneOf4(Box::new(event)))
                .await;
        });
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

    /// 工具调用结束：按 tool_call_id 取出缓冲数据，提交单个工具 Span；
    /// 若为批次最后一个工具，额外提交批次 Tools Span（parent = agent_span_id）。
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
            let trace_id = trace_id.clone();
            let end_time = end_time.clone();
            tokio::spawn(async move {
                let status_msg = if is_error { Some(Some("error".to_string())) } else { None };
                let body = CreateSpanBody {
                    id: Some(Some(tool.span_id)),
                    trace_id: Some(Some(trace_id)),
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
                let _ = batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event))).await;
            });
        }

        // 最后一个工具结束时，提交批次 Tools Span（parent = agent_span_id）
        if self.pending_tools.is_empty() {
            if let (Some(batch_id), Some(batch_start)) = (
                self.tools_batch_span_id.take(),
                self.tools_batch_start_time.take(),
            ) {
                let agent_span_id = self.agent_span_id.clone();
                tokio::spawn(async move {
                    let body = CreateSpanBody {
                        id: Some(Some(batch_id)),
                        trace_id: Some(Some(trace_id)),
                        name: Some(Some("Tools".to_string())),
                        start_time: Some(Some(batch_start)),
                        end_time: Some(Some(end_time.clone())),
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
                        timestamp: end_time,
                        body: Box::new(body),
                        r#type: ingestion_event_one_of_2::Type::SpanCreate,
                        metadata: None,
                    };
                    let _ = batcher.add(IngestionEvent::IngestionEventOneOf2(Box::new(event))).await;
                });
            }
        }
    }

    /// 对话轮次结束：更新 Trace 的最终输出，并强制 flush
    pub fn on_trace_end(&mut self, final_answer: &str) {
        let client = Arc::clone(&self.session.client);
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let output = final_answer.to_string();

        tokio::spawn(async move {
            // 更新 Trace 输出
            let _ = client
                .trace()
                .id(trace_id)
                .name("agent-run")
                .output(serde_json::json!(output))
                .call()
                .await;

            // 等待所有并发 spawn（on_llm_end/on_tool_end）完成 batcher.add，再 flush
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = batcher.flush().await;
        });
    }
}
