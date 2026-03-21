use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::error::{AgentError, AgentResult};
use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
use crate::llm::types::{LlmRequest, LlmResponse, StopReason};
use crate::tools::BaseTool;
use super::BaseModel;

/// ChatAnthropic - Anthropic Messages API 实现
pub struct ChatAnthropic {
    pub api_key: String,
    pub model: String,
    pub extended_thinking: bool,
    pub thinking_budget: u32,
    /// 是否开启 Prompt Caching（anthropic-beta: prompt-caching-2024-07-31），默认开启
    pub enable_cache: bool,
    /// 自定义 base URL（代理场景），不含末尾 /
    pub base_url: Option<String>,
    client: reqwest::Client,
}

impl ChatAnthropic {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            extended_thinking: false,
            thinking_budget: 10000,
            enable_cache: true,
            base_url: None,
            client: reqwest::Client::new(),
        }
    }

    /// 设置自定义 base URL（用于代理或兼容 API）
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        self.base_url = if url.is_empty() { None } else { Some(url) };
        self
    }

    /// 开启 Extended Thinking（claude-3-7-sonnet 及以上）
    pub fn with_extended_thinking(mut self, budget_tokens: u32) -> Self {
        self.extended_thinking = true;
        self.thinking_budget = budget_tokens;
        self
    }

    /// 关闭 Prompt Caching
    pub fn without_cache(mut self) -> Self {
        self.enable_cache = false;
        self
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
        let mut s = Self::new(api_key, model);
        if let Ok(url) = std::env::var("ANTHROPIC_BASE_URL") {
            s = s.with_base_url(url);
        }
        Some(s)
    }

    // ─── ContentBlock → Anthropic content part ────────────────────────────────

    fn block_to_anthropic(block: &ContentBlock) -> Option<Value> {
        match block {
            ContentBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
            ContentBlock::Image { source } => {
                match source {
                    ImageSource::Base64 { media_type, data } => Some(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data
                        }
                    })),
                    ImageSource::Url { url } => Some(json!({
                        "type": "image",
                        "source": { "type": "url", "url": url }
                    })),
                }
            }
            ContentBlock::Document { source, title } => {
                let src = serde_json::to_value(source).unwrap_or_default();
                let mut obj = json!({ "type": "document", "source": src });
                if let Some(t) = title {
                    obj["title"] = json!(t);
                }
                Some(obj)
            }
            ContentBlock::ToolUse { id, name, input } => Some(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            })),
            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                let content_val: Vec<Value> = content
                    .iter()
                    .filter_map(|b| Self::block_to_anthropic(b))
                    .collect();
                Some(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content_val,
                    "is_error": is_error
                }))
            }
            // thinking block 在 assistant 消息中由 Anthropic 生成，发送时透传
            ContentBlock::Reasoning { text, signature } => {
                let mut obj = json!({ "type": "thinking", "thinking": text });
                if let Some(sig) = signature {
                    obj["signature"] = json!(sig);
                }
                Some(obj)
            }
            ContentBlock::Unknown => None,
        }
    }

    fn content_to_anthropic(content: &MessageContent) -> Value {
        match content {
            MessageContent::Text(s) => json!(s),
            MessageContent::Blocks(blocks) => {
                let parts: Vec<Value> = blocks
                    .iter()
                    .filter_map(|b| Self::block_to_anthropic(b))
                    .collect();
                Value::Array(parts)
            }
            MessageContent::Raw(values) => Value::Array(values.clone()),
        }
    }

    /// 将 BaseMessage 列表转为 Anthropic messages 格式
    ///
    /// - System 消息提取到顶层 system 字段
    /// - Tool 消息合并为 user content blocks
    fn messages_to_anthropic(messages: &[BaseMessage]) -> (Vec<Value>, Option<String>) {
        let mut system_parts: Vec<String> = Vec::new();
        let mut result: Vec<Value> = Vec::new();

        for msg in messages {
            match msg {
                BaseMessage::System { content } => {
                    let text = content.text_content();
                    if !text.trim().is_empty() {
                        system_parts.push(text);
                    }
                }
                BaseMessage::Human { content } => {
                    result.push(json!({
                        "role": "user",
                        "content": Self::content_to_anthropic(content)
                    }));
                }
                BaseMessage::Ai { content, tool_calls } => {
                    if tool_calls.is_empty() {
                        result.push(json!({
                            "role": "assistant",
                            "content": Self::content_to_anthropic(content)
                        }));
                    } else {
                        // 若 content 已经是 Blocks（含 ToolUse），直接序列化
                        // 否则构造 text + tool_use blocks
                        let content_val = match content {
                            MessageContent::Blocks(_) | MessageContent::Raw(_) => {
                                Self::content_to_anthropic(content)
                            }
                            MessageContent::Text(t) => {
                                let mut blocks: Vec<Value> = Vec::new();
                                if !t.is_empty() {
                                    blocks.push(json!({ "type": "text", "text": t }));
                                }
                                for tc in tool_calls {
                                    blocks.push(json!({
                                        "type": "tool_use",
                                        "id": tc.id,
                                        "name": tc.name,
                                        "input": tc.arguments
                                    }));
                                }
                                Value::Array(blocks)
                            }
                        };
                        result.push(json!({ "role": "assistant", "content": content_val }));
                    }
                }
                BaseMessage::Tool { tool_call_id, content, is_error } => {
                    let tool_result_block = json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": Self::content_to_anthropic(content),
                        "is_error": is_error
                    });

                    let should_append = result.last().map(|last| {
                        last["role"] == "user" && last["content"].is_array()
                    }).unwrap_or(false);

                    if should_append {
                        if let Some(last) = result.last_mut() {
                            last["content"].as_array_mut().unwrap().push(tool_result_block);
                        }
                    } else {
                        result.push(json!({
                            "role": "user",
                            "content": [tool_result_block]
                        }));
                    }
                }
            }
        }

        let system_text = if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n\n"))
        };
        (result, system_text)
    }

    /// 对 messages 列表中最后一条消息的最后一个 content block 追加 cache_control
    ///
    /// Anthropic Prompt Caching 要求在需要缓存的边界位置加 `cache_control: { type: "ephemeral" }`。
    fn apply_cache_to_messages(messages: &mut Vec<Value>) {
        if let Some(last_msg) = messages.last_mut() {
            if let Some(content) = last_msg.get_mut("content") {
                match content {
                    Value::Array(blocks) => {
                        if let Some(last_block) = blocks.last_mut() {
                            // 跳过空 text block
                            let is_empty_text = last_block["type"].as_str() == Some("text")
                                && last_block["text"].as_str().map(|t| t.trim().is_empty()).unwrap_or(false);
                            if !is_empty_text {
                                last_block["cache_control"] = json!({ "type": "ephemeral" });
                            }
                        }
                    }
                    Value::String(s) if !s.trim().is_empty() => {
                        // 将纯文本 content 升级为 blocks，以便加 cache_control
                        let text = s.clone();
                        *content = json!([{
                            "type": "text",
                            "text": text,
                            "cache_control": { "type": "ephemeral" }
                        }]);
                    }
                    _ => {}
                }
            }
        }
    }

    // ─── 响应 content blocks → BaseMessage ───────────────────────────────────

    fn parse_content_blocks(raw_blocks: &[Value]) -> (Vec<ContentBlock>, Vec<ToolCallRequest>) {
        let mut blocks: Vec<ContentBlock> = Vec::new();
        let mut tool_calls: Vec<ToolCallRequest> = Vec::new();

        for b in raw_blocks {
            match b["type"].as_str() {
                Some("text") => {
                    if let Some(text) = b["text"].as_str() {
                        blocks.push(ContentBlock::text(text));
                    }
                }
                Some("thinking") => {
                    let text = b["thinking"].as_str().unwrap_or("").to_string();
                    let signature = b["signature"].as_str().map(|s| s.to_string());
                    if let Some(sig) = signature {
                        blocks.push(ContentBlock::reasoning_with_signature(text, sig));
                    } else {
                        blocks.push(ContentBlock::reasoning(text));
                    }
                }
                Some("tool_use") => {
                    if let (Some(id), Some(name)) =
                        (b["id"].as_str(), b["name"].as_str())
                    {
                        let input = b["input"].clone();
                        blocks.push(ContentBlock::tool_use(id, name, input.clone()));
                        tool_calls.push(ToolCallRequest::new(id, name, input));
                    }
                }
                _ => {} // 忽略未知 block
            }
        }

        (blocks, tool_calls)
    }
}

#[async_trait]
impl BaseModel for ChatAnthropic {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let chat_url = match &self.base_url {
            Some(base) => format!("{}/v1/messages", base.trim_end_matches('/')),
            None => "https://api.anthropic.com/v1/messages".to_string(),
        };

        let tools_json: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            }))
            .collect();

        let (mut messages, system_from_msgs) = Self::messages_to_anthropic(&request.messages);
        // 合并：消息列表中的 System（来自中间件，如 agent.md）在前，
        // request.system（BaseModelReactLLM 设置的基础提示词）在后
        let system = match (system_from_msgs, request.system) {
            (Some(from_msgs), Some(base)) => Some(format!("{}\n\n{}", from_msgs, base)),
            (Some(from_msgs), None) => Some(from_msgs),
            (None, base) => base,
        };
        let max_tokens = request.max_tokens.unwrap_or(4096);

        // 开启缓存时：对最后一条消息的最后一个 block 加 cache_control
        if self.enable_cache {
            Self::apply_cache_to_messages(&mut messages);
        }

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "messages": messages
        });

        if self.enable_cache {
            // system 升级为 blocks 数组格式以支持 cache_control
            if let Some(ref sys_text) = system {
                body["system"] = json!([{
                    "type": "text",
                    "text": sys_text,
                    "cache_control": { "type": "ephemeral" }
                }]);
            }
        } else if let Some(sys) = &system {
            body["system"] = json!(sys);
        }

        if !tools_json.is_empty() {
            body["tools"] = Value::Array(tools_json);
        }

        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }

        // Extended Thinking 配置
        if self.extended_thinking {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": self.thinking_budget
            });
        }

        let mut req = self
            .client
            .post(chat_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        // Prompt Caching 需要 beta header
        if self.enable_cache {
            req = req.header("anthropic-beta", "prompt-caching-2024-07-31");
        }

        let resp = req
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let status = resp.status();
        let resp_text = resp
            .text()
            .await
            .map_err(|e| AgentError::LlmError(format!("读取响应体失败: {e}")))?;
        let resp_json: Value = serde_json::from_str(&resp_text)
            .map_err(|e| AgentError::LlmError(format!("解析响应失败: {e}\n原始响应({status}): {resp_text}")))?;

        if !status.is_success() {
            let msg = resp_json["error"]["message"]
                .as_str()
                .unwrap_or("未知错误")
                .to_string();
            return Err(AgentError::LlmError(format!("API 错误 {status}: {msg}")));
        }

        let stop_reason = StopReason::from_anthropic(
            resp_json["stop_reason"].as_str().unwrap_or("end_turn"),
        );

        let raw_blocks = resp_json["content"]
            .as_array()
            .ok_or_else(|| AgentError::LlmError("响应缺少 content 字段".to_string()))?;

        let (blocks, tool_calls) = Self::parse_content_blocks(raw_blocks);

        // 决定 content 形式
        // - 只有单个纯文本且无工具调用 → 简单 Text（向后兼容）
        // - 含 thinking / tool_use / 多 block → Blocks
        let message = if !tool_calls.is_empty() {
            let content = if blocks.len() == 1 && blocks[0].as_text().is_some() {
                MessageContent::text(blocks[0].as_text().unwrap())
            } else {
                MessageContent::Blocks(blocks)
            };
            BaseMessage::ai_with_tool_calls(content, tool_calls)
        } else if blocks.len() == 1 && blocks[0].as_text().is_some() {
            BaseMessage::ai(blocks[0].as_text().unwrap())
        } else if blocks.is_empty() {
            BaseMessage::ai("")
        } else {
            // 含 thinking block 或多 block
            BaseMessage::ai(MessageContent::Blocks(blocks))
        };

        Ok(LlmResponse { message, stop_reason })
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl ReactLLM for ChatAnthropic {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
    ) -> AgentResult<Reasoning> {
        let tool_defs = tools.iter().map(|t| t.definition()).collect();
        let request = LlmRequest::new(messages.to_vec()).with_tools(tool_defs);

        // system 消息由 messages_to_anthropic 从消息列表提取，无需单独处理

        let response = self.invoke(request).await?;

        if response.stop_reason == StopReason::ToolUse {
            let blocks = response.message.content_blocks();
            let thought = blocks
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("");

            let calls: Vec<ToolCall> = blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolUse { id, name, input } = b {
                        Some(ToolCall::new(id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            if !calls.is_empty() {
                let mut r = Reasoning::with_tools(thought, calls);
                r.source_message = Some(response.message);
                return Ok(r);
            }

            let calls: Vec<ToolCall> = response
                .message
                .tool_calls()
                .iter()
                .map(|tc| ToolCall::new(tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                .collect();
            let mut r = Reasoning::with_tools(thought, calls);
            r.source_message = Some(response.message);
            Ok(r)
        } else {
            let text = response.message.content();
            let mut r = Reasoning::with_answer("", text);
            r.source_message = Some(response.message);
            Ok(r)
        }
    }
}
