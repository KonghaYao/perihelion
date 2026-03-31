use std::collections::HashMap;
use std::sync::Arc;

use langfuse_client::{
    GenerationBody, IngestionEvent, ObservationBody, ObservationType,
    SpanBody,
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
///
/// 所有事件通过 `batcher.try_add()` 同步入队，保证事件顺序与调用顺序一致，
/// 确保 Langfuse 层级关系正确（父 span 先于子 span 入队）。
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,
    /// 当前对话轮次的 Trace ID（提前生成，所有观测对象共享）
    trace_id: String,
    /// 主 Agent Observation 的 ID
    agent_observation_id: String,
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
    /// 累积的最终回答
    final_answer: String,
    /// SubAgent 栈：保存当前活动的 subagent observation IDs
    /// 支持 subagent 嵌套调用（subagent 中再调用 subagent）
    subagent_stack: Vec<SubAgentContext>,
}

/// SubAgent 追踪上下文
struct SubAgentContext {
    /// SubAgent 的 Observation ID
    observation_id: String,
    /// SubAgent 的 agent_id（如 "code-reviewer"）
    agent_id: String,
    /// 开始时间（RFC3339）
    start_time: String,
    /// 当前 subagent 下的 tools batch 信息
    tools_batch_span_id: Option<String>,
    tools_batch_start_time: Option<String>,
    tools_batch_end_time: Option<String>,
    /// SubAgent 下的工具调用缓冲
    pending_tools: HashMap<String, PendingTool>,
}

impl LangfuseTracer {
    /// 从共享 Session 构造 per-turn Tracer
    pub fn new(session: Arc<LangfuseSession>) -> Self {
        Self {
            session,
            trace_id: uuid::Uuid::now_v7().to_string(),
            agent_observation_id: uuid::Uuid::now_v7().to_string(),
            generation_data: HashMap::new(),
            pending_tools: HashMap::new(),
            tools_batch_span_id: None,
            tools_batch_start_time: None,
            tools_batch_end_time: None,
            final_answer: String::new(),
            subagent_stack: Vec::new(),
        }
    }

    /// 获取当前活动的 agent observation ID
    /// 若有 subagent 栈，返回栈顶的 subagent ID；否则返回主 agent ID
    fn current_agent_id(&self) -> String {
        self.subagent_stack
            .last()
            .map(|ctx| ctx.observation_id.clone())
            .unwrap_or_else(|| self.agent_observation_id.clone())
    }

    /// 获取当前活动的 tools batch 上下文
    fn current_tools_context(&mut self) -> (&mut Option<String>, &mut Option<String>, &mut Option<String>, &mut HashMap<String, PendingTool>) {
        if let Some(subagent) = self.subagent_stack.last_mut() {
            (
                &mut subagent.tools_batch_span_id,
                &mut subagent.tools_batch_start_time,
                &mut subagent.tools_batch_end_time,
                &mut subagent.pending_tools,
            )
        } else {
            (
                &mut self.tools_batch_span_id,
                &mut self.tools_batch_start_time,
                &mut self.tools_batch_end_time,
                &mut self.pending_tools,
            )
        }
    }

    /// TextChunk 事件：累积最终回答
    pub fn on_text_chunk(&mut self, chunk: &str) {
        self.final_answer.push_str(chunk);
    }

    /// 提交当前批次 Tools Span
    fn flush_tools_batch(&mut self) {
        let (batch_id, batch_start, batch_end, parent_id) = {
            let (batch_id_ref, batch_start_ref, batch_end_ref, _) = self.current_tools_context();
            if let (Some(batch_id), Some(batch_start), Some(batch_end)) = (
                batch_id_ref.take(),
                batch_start_ref.take(),
                batch_end_ref.take(),
            ) {
                (batch_id, batch_start, batch_end, self.current_agent_id())
            } else {
                return;
            }
        };

        let body = SpanBody {
            id: Some(batch_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            name: Some("Tools".to_string()),
            start_time: Some(batch_start),
            end_time: Some(batch_end.clone()),
            parent_observation_id: Some(parent_id),
            input: None,
            output: None,
            status_message: None,
            metadata: None,
            level: None,
            version: None,
            environment: None,
            session_id: Some(self.session.session_id.clone()),
        };
        let event = IngestionEvent::SpanCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: batch_end,
            body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: tools batch span 入队失败（背压丢弃）");
        }
    }

    /// 对话轮次开始：创建 agent-run Observation（根 observation）
    pub fn on_trace_start(&mut self, input: &str) {
        let batcher = &self.session.batcher;
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // 创建 agent-run Observation（type=Agent，Langfuse 语义：agent decides on application flow, uses tools with LLM guidance）
        let body = ObservationBody {
            id: Some(self.agent_observation_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            r#type: ObservationType::Agent,
            name: Some("agent-run".to_string()),
            start_time: Some(start_time),
            end_time: None,
            completion_start_time: None,
            parent_observation_id: None,
            input: Some(serde_json::json!(input)),
            output: None,
            metadata: None,
            model: None,
            model_parameters: None,
            level: None,
            status_message: None,
            version: None,
            environment: None,
            session_id: Some(self.session.session_id.clone()),
        };
        let event = IngestionEvent::ObservationCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            body,
            metadata: None,
        };
        if let Err(e) = batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: agent-run observation 入队失败（背压丢弃）");
        }
    }

    /// LLM 调用开始：提交上一轮工具批次 Span，缓存本轮 input
    pub fn on_llm_start(&mut self, step: usize, messages: &[BaseMessage], tools: &[ToolDefinition]) {
        self.flush_tools_batch();
        let gen_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.generation_data
            .insert(step, (gen_id, messages.to_vec(), tools.to_vec(), start_time));
    }

    /// LLM 调用结束：同步创建 Generation 事件
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

        let messages_val = serde_json::to_value(&messages).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: messages 序列化失败");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let tools_val = serde_json::to_value(&tools).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: tools 序列化失败");
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

        let body = GenerationBody {
            id: Some(gen_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            name: Some(format!("Chat{}", provider)),
            input: Some(input_json),
            output: Some(serde_json::json!(output)),
            model: Some(model.to_string()),
            usage_details: langfuse_usage_details,
            parent_observation_id: Some(self.current_agent_id()),
            start_time: Some(start_time),
            end_time: Some(end_time.clone()),
            session_id: Some(self.session.session_id.clone()),
            ..Default::default()
        };
        let event = IngestionEvent::GenerationCreate {
            id: gen_id.clone(),
            timestamp: end_time,
            body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, gen_id = %gen_id, "langfuse: generation 入队失败（背压丢弃）");
        }
    }

    /// 工具调用开始
    pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
        let current_agent_id = self.current_agent_id();
        let (batch_id_ref, start_time_ref, _, pending_tools) = self.current_tools_context();
        if pending_tools.is_empty() {
            *batch_id_ref = Some(uuid::Uuid::now_v7().to_string());
            *start_time_ref = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
        let parent_span_id = batch_id_ref
            .clone()
            .unwrap_or_else(|| current_agent_id);

        let span_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        pending_tools.insert(tool_call_id.to_string(), PendingTool {
            span_id,
            name: name.to_string(),
            input: input.clone(),
            start_time,
            parent_span_id,
        });
    }

    /// 工具调用结束：同步创建 tool observation
    pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool) {
        let trace_id = self.trace_id.clone();
        let trace_id_for_log = self.trace_id.clone();
        let (_, _, end_time_ref, pending_tools) = self.current_tools_context();
        let Some(tool) = pending_tools.remove(tool_call_id) else {
            return;
        };
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let status_msg = if is_error { Some("error".to_string()) } else { None };
        let tool_name = tool.name.clone();
        let span_id = tool.span_id;
        let tool_name_for_body = tool.name.clone();
        let tool_input = tool.input;
        let tool_start_time = tool.start_time;
        let tool_parent_id = tool.parent_span_id;

        let body = ObservationBody {
            id: Some(span_id),
            trace_id: Some(trace_id),
            r#type: ObservationType::Tool,
            name: Some(tool_name_for_body),
            input: Some(tool_input),
            output: Some(serde_json::json!(output)),
            start_time: Some(tool_start_time),
            end_time: Some(end_time.clone()),
            completion_start_time: None,
            parent_observation_id: Some(tool_parent_id),
            metadata: None,
            model: None,
            model_parameters: None,
            level: None,
            status_message: status_msg,
            version: None,
            environment: None,
            session_id: None,
        };
        let event = IngestionEvent::ObservationCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: end_time.clone(),
            body,
            metadata: None,
        };
        // 在可变借用结束后再访问 self.session
        drop(end_time_ref); // 显式结束借用
        drop(pending_tools);
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %trace_id_for_log, tool = %tool_name, "langfuse: tool observation 入队失败（背压丢弃）");
        }

        // 重新获取可变借用
        let (_, _, end_time_ref, _) = self.current_tools_context();
        *end_time_ref = Some(end_time);
    }

    /// 对话轮次结束：更新 agent-run Observation 输出和结束时间，并强制 flush。
    pub fn on_trace_end(&mut self, error_output: Option<&str>) -> tokio::task::JoinHandle<()> {
        self.flush_tools_batch();

        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let agent_observation_id = self.agent_observation_id.clone();
        let output = if let Some(err) = error_output {
            err.to_string()
        } else {
            std::mem::take(&mut self.final_answer)
        };

        tokio::spawn(async move {
            let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

            // 更新 agent-run Observation 的 output 和 end_time
            let obs_body = ObservationBody {
                id: Some(agent_observation_id.clone()),
                trace_id: Some(trace_id.clone()),
                r#type: ObservationType::Agent,
                name: Some("agent-run".to_string()),
                output: Some(serde_json::json!(output)),
                end_time: Some(end_time.clone()),
                ..Default::default()
            };
            let obs_event = IngestionEvent::ObservationUpdate {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: end_time,
                body: obs_body,
                metadata: None,
            };
            if let Err(e) = batcher.add(obs_event).await {
                tracing::warn!(error = %e, trace_id = %trace_id, obs_id = %agent_observation_id, "langfuse: agent-run observation 更新失败");
            }
            if let Err(e) = batcher.flush().await {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: batcher flush 失败");
            }
        })
    }

    /// SubAgent 开始：创建独立的 agent-run Observation（与主 agent 共享 trace_id）
    pub fn on_subagent_start(&mut self, agent_id: &str, task_preview: &str) {
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let observation_id = uuid::Uuid::now_v7().to_string();
        let parent_observation_id = self.current_agent_id();

        // 创建 SubAgent 的 Observation（type=Agent）
        let body = ObservationBody {
            id: Some(observation_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            r#type: ObservationType::Agent,
            name: Some(format!("subagent:{}", agent_id)),
            start_time: Some(start_time.clone()),
            end_time: None,
            completion_start_time: None,
            parent_observation_id: Some(parent_observation_id),
            input: Some(serde_json::json!(task_preview)),
            output: None,
            metadata: None,
            model: None,
            model_parameters: None,
            level: None,
            status_message: None,
            version: None,
            environment: None,
            session_id: Some(self.session.session_id.clone()),
        };
        let event = IngestionEvent::ObservationCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: start_time.clone(),
            body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, subagent = %agent_id, "langfuse: subagent observation 入队失败（背压丢弃）");
        }

        // 将 subagent 上下文压入栈
        self.subagent_stack.push(SubAgentContext {
            observation_id,
            agent_id: agent_id.to_string(),
            start_time,
            tools_batch_span_id: None,
            tools_batch_start_time: None,
            tools_batch_end_time: None,
            pending_tools: HashMap::new(),
        });
    }

    /// SubAgent 结束：更新 Observation 并弹出栈
    pub fn on_subagent_end(&mut self, result: &str, is_error: bool) {
        let (subagent_id, subagent_name) = match self.subagent_stack.pop() {
            Some(ctx) => (ctx.observation_id, ctx.agent_id),
            None => {
                tracing::warn!("langfuse: on_subagent_end 调用时 subagent_stack 为空，忽略");
                return;
            }
        };

        // 先 flush subagent 下的 tools batch（如果有）
        // 注意：需要在弹出栈之前调用 flush_tools_batch，它会自动使用当前（即将弹出的）subagent 上下文
        self.flush_tools_batch();

        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let status_message = if is_error { Some("error".to_string()) } else { None };

        // 更新 SubAgent Observation
        let obs_body = ObservationBody {
            id: Some(subagent_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            r#type: ObservationType::Agent,
            name: Some(format!("subagent:{}", subagent_name)),
            output: Some(serde_json::json!(result)),
            end_time: Some(end_time.clone()),
            status_message,
            ..Default::default()
        };
        let obs_event = IngestionEvent::ObservationUpdate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: end_time,
            body: obs_body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(obs_event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, subagent = %subagent_name, "langfuse: subagent observation 更新失败（背压丢弃）");
        }
    }
}
