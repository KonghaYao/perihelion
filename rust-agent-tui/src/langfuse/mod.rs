pub mod config;
pub use config::LangfuseConfig;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use langfuse_client_base::models::{
    ingestion_event_one_of_4::Type as GenType, CreateGenerationBody, IngestionEvent,
    IngestionEventOneOf4,
};
use langfuse_ergonomic::{BackpressurePolicy, Batcher, ClientBuilder, LangfuseClient};
use rust_create_agent::llm::types::TokenUsage;
use rust_create_agent::messages::BaseMessage;

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

/// Langfuse 单轮追踪器（per-turn）
///
/// 持有对 `LangfuseSession` 的引用，复用 client/batcher/session_id。
/// 生命周期：从 submit_message() 开始 → AgentEvent::Done 时结束。
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,
    /// 当前对话轮次的 Trace ID（提前生成，所有观测对象共享）
    trace_id: String,
    /// step → (generation_id, input_messages)
    generation_data: HashMap<usize, (String, Vec<BaseMessage>)>,
    /// FIFO 队列：工具调用 span_id
    pending_spans: VecDeque<String>,
}

impl LangfuseTracer {
    /// 从共享 Session 构造 per-turn Tracer（同步）
    pub fn new(session: Arc<LangfuseSession>) -> Self {
        Self {
            session,
            trace_id: uuid::Uuid::now_v7().to_string(),
            generation_data: HashMap::new(),
            pending_spans: VecDeque::new(),
        }
    }

    /// 对话轮次开始：创建 Trace（session_id 从共享 Session 读取）
    pub fn on_trace_start(&mut self, input: &str) {
        let client = Arc::clone(&self.session.client);
        let trace_id = self.trace_id.clone();
        let input = input.to_string();
        let session_id = self.session.session_id.clone();
        tokio::spawn(async move {
            let _ = client
                .trace()
                .id(trace_id)
                .name("agent-run")
                .input(serde_json::json!(input))
                .session_id(session_id)
                .call()
                .await;
        });
    }

    /// LLM 调用开始：缓存 input messages，等 on_llm_end 时一并上报 Generation
    pub fn on_llm_start(&mut self, step: usize, messages: &[BaseMessage]) {
        let gen_id = uuid::Uuid::now_v7().to_string();
        self.generation_data
            .insert(step, (gen_id, messages.to_vec()));
    }

    /// LLM 调用结束：通过 Batcher 直接构造 IngestionEvent，绕过 builder 的 usage bug
    pub fn on_llm_end(
        &mut self,
        step: usize,
        model: &str,
        output: &str,
        usage: Option<&TokenUsage>,
    ) {
        let Some((gen_id, messages)) = self.generation_data.remove(&step) else {
            return;
        };
        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let model = model.to_string();
        let output = output.to_string();
        let step_for_closure = step;
        let input_json = serde_json::to_value(&messages).unwrap_or(serde_json::Value::Null);

        // 构造 IngestionUsage：使用 langfuse_client_base 的原生类型
        let langfuse_usage = usage.map(|u| {
            use langfuse_client_base::models::{IngestionUsage, Usage};
            Box::new(IngestionUsage::Usage(Box::new(Usage {
                input: Some(Some(u.input_tokens as i32)),
                output: Some(Some(u.output_tokens as i32)),
                total: Some(Some((u.input_tokens + u.output_tokens) as i32)),
                unit: None,
                input_cost: None,
                output_cost: None,
                total_cost: None,
            })))
        });

        tokio::spawn(async move {
            let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let body = CreateGenerationBody {
                id: Some(Some(gen_id.clone())),
                trace_id: Some(Some(trace_id)),
                name: Some(Some(format!("llm-call-step-{}", step_for_closure))),
                input: Some(Some(input_json)),
                output: Some(Some(serde_json::json!(output))),
                model: Some(Some(model)),
                usage: langfuse_usage,
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

    /// 工具调用开始：创建 Span，将 span_id 加入 FIFO 队列
    pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
        let span_id = uuid::Uuid::now_v7().to_string();
        self.pending_spans.push_back(span_id.clone());
        let client = Arc::clone(&self.session.client);
        let trace_id = self.trace_id.clone();
        let name = name.to_string();
        let input = input.clone();
        let _tool_call_id = tool_call_id.to_string();
        tokio::spawn(async move {
            let _ = client
                .span()
                .id(span_id)
                .trace_id(trace_id)
                .name(name)
                .input(input)
                .call()
                .await;
        });
    }

    /// 工具调用结束：按 FIFO 顺序取 span_id，更新 Span 的 output
    pub fn on_tool_end_by_name_order(&mut self, output: &str, is_error: bool) {
        let Some(span_id) = self.pending_spans.pop_front() else {
            return;
        };
        let client = Arc::clone(&self.session.client);
        let trace_id = self.trace_id.clone();
        let output = output.to_string();
        let status_msg = if is_error {
            Some("error".to_string())
        } else {
            None
        };
        tokio::spawn(async move {
            let _ = client
                .update_span()
                .id(span_id)
                .trace_id(trace_id)
                .output(serde_json::json!(output))
                .maybe_status_message(status_msg)
                .call()
                .await;
        });
    }

    /// 对话轮次结束：更新 Trace 的最终输出（Langfuse 支持同 ID upsert）
    pub fn on_trace_end(&mut self, final_answer: &str) {
        let client = Arc::clone(&self.session.client);
        let trace_id = self.trace_id.clone();
        let output = final_answer.to_string();
        tokio::spawn(async move {
            let _ = client
                .trace()
                .id(trace_id)
                .name("agent-run")
                .output(serde_json::json!(output))
                .call()
                .await;
        });
    }
}
