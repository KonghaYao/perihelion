use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::error::{AgentError, AgentResult};
use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
use crate::llm::types::{LlmRequest, LlmResponse, StopReason};
use crate::tools::BaseTool;
use super::BaseModel;

/// ChatOpenAI - OpenAI 兼容 API 的 LLM 实现
pub struct ChatOpenAI {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// o1/o3 系列推理强度："low" | "medium" | "high"
    /// 设置后请求体加 `reasoning_effort` 字段，同时移除 temperature
    pub reasoning_effort: Option<String>,
    client: reqwest::Client,
}

impl ChatOpenAI {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: model.into(),
            reasoning_effort: None,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// 开启 reasoning effort（o1/o3 系列）
    /// `effort`: "low" | "medium" | "high"
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        let base_url = std::env::var("OPENAI_API_BASE")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let model = std::env::var("OPENAI_MODEL")
            .unwrap_or_else(|_| "gpt-4o".to_string());
        Some(Self::new(api_key, model).with_base_url(base_url))
    }

    // ─── MessageContent → OpenAI content ──────────────────────────────────────

    /// 将 MessageContent 序列化为 OpenAI content 字段
    ///
    /// - `Text(s)` → 字符串
    /// - `Blocks(v)` → array of content parts
    /// - `Raw(v)` → 透传
    fn content_to_openai(content: &MessageContent) -> Value {
        match content {
            MessageContent::Text(s) => json!(s),
            MessageContent::Blocks(blocks) => {
                let parts: Vec<Value> = blocks
                    .iter()
                    .filter_map(|b| Self::block_to_openai_part(b))
                    .collect();
                if parts.is_empty() {
                    json!("")
                } else {
                    Value::Array(parts)
                }
            }
            MessageContent::Raw(values) => Value::Array(values.clone()),
        }
    }

    fn block_to_openai_part(block: &ContentBlock) -> Option<Value> {
        match block {
            ContentBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
            ContentBlock::Image { source } => {
                let image_url = match source {
                    ImageSource::Url { url } => json!({ "url": url }),
                    ImageSource::Base64 { media_type, data } => {
                        json!({ "url": format!("data:{media_type};base64,{data}") })
                    }
                };
                Some(json!({ "type": "image_url", "image_url": image_url }))
            }
            // ToolUse / ToolResult 在 assistant / tool 角色消息中处理，此处跳过
            ContentBlock::ToolUse { .. } | ContentBlock::ToolResult { .. } => None,
            // OpenAI 的 reasoning_effort 模式不在 content 里暴露，跳过
            ContentBlock::Reasoning { .. } => None,
            // Document / Unknown 透传为 raw JSON（OpenAI 可能不支持，但透传保持兼容）
            ContentBlock::Document { source, title } => {
                let src = serde_json::to_value(source).unwrap_or_default();
                Some(json!({ "type": "document", "source": src, "title": title }))
            }
            ContentBlock::Unknown => None,
        }
    }

    fn messages_to_json(messages: &[BaseMessage]) -> Vec<Value> {
        // 先收集所有 System 消息，合并为一条放到首位
        let system_parts: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                if let BaseMessage::System { content } = m {
                    let t = content.text_content();
                    if !t.trim().is_empty() { Some(t) } else { None }
                } else {
                    None
                }
            })
            .collect();

        let mut result: Vec<Value> = Vec::new();

        if !system_parts.is_empty() {
            result.push(json!({ "role": "system", "content": system_parts.join("\n\n") }));
        }

        for m in messages {
            match m {
                BaseMessage::System { .. } => {} // 已在上方合并，跳过
                BaseMessage::Human { content } => {
                    result.push(json!({ "role": "user", "content": Self::content_to_openai(content) }));
                }
                BaseMessage::Ai { content, tool_calls } => {
                    if tool_calls.is_empty() {
                        result.push(json!({ "role": "assistant", "content": Self::content_to_openai(content) }));
                    } else {
                        let tcs: Vec<Value> = tool_calls
                            .iter()
                            .map(|tc| json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string()
                                }
                            }))
                            .collect();
                        result.push(json!({
                            "role": "assistant",
                            "content": Self::content_to_openai(content),
                            "tool_calls": tcs
                        }));
                    }
                }
                BaseMessage::Tool { tool_call_id, content, .. } => {
                    result.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": Self::content_to_openai(content)
                    }));
                }
            }
        }

        result
    }

    // ─── 响应 → BaseMessage ───────────────────────────────────────────────────

    /// 将 OpenAI 响应解析为 BaseMessage（含 reasoning block）
    ///
    /// 支持 `o1/o3/deepseek-r1` 格式：
    /// - `message.reasoning_content` → `ContentBlock::Reasoning`
    /// - `message.content` → `ContentBlock::Text`
    fn parse_assistant_message(
        assistant_msg: &Value,
        stop_reason: &StopReason,
    ) -> BaseMessage {
        let content_str = assistant_msg["content"].as_str().unwrap_or("").to_string();

        // 收集 content blocks
        let mut blocks: Vec<ContentBlock> = Vec::new();

        // reasoning_content（deepseek-r1、某些 OpenAI o 系列）
        if let Some(reasoning) = assistant_msg["reasoning_content"].as_str() {
            if !reasoning.is_empty() {
                blocks.push(ContentBlock::reasoning(reasoning));
            }
        }

        // 主文本
        if !content_str.is_empty() {
            blocks.push(ContentBlock::text(content_str.clone()));
        }

        if *stop_reason == StopReason::ToolUse {
            // tool_calls 也提取为 ToolUse blocks + ToolCallRequest
            let tool_calls: Vec<ToolCallRequest> = assistant_msg["tool_calls"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|tc| {
                    let id = tc["id"].as_str()?;
                    let name = tc["function"]["name"].as_str()?;
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let arguments = serde_json::from_str::<Value>(args_str)
                        .unwrap_or(Value::String(args_str.to_string()));
                    blocks.push(ContentBlock::tool_use(id, name, arguments.clone()));
                    Some(ToolCallRequest::new(id, name, arguments))
                })
                .collect();

            let content = if blocks.len() == 1 && blocks[0].as_text().is_some() {
                // 没有 reasoning，只有文本 → 保持简单 Text
                MessageContent::text(content_str)
            } else if blocks.is_empty() {
                MessageContent::default()
            } else {
                MessageContent::Blocks(blocks)
            };

            BaseMessage::ai_with_tool_calls(content, tool_calls)
        } else if blocks.len() == 1 && blocks[0].as_text().is_some() {
            // 普通文本回复，保持简单形式
            BaseMessage::ai(content_str)
        } else if blocks.is_empty() {
            BaseMessage::ai("")
        } else {
            // 含 reasoning block（或其他 block）→ Blocks 形式
            BaseMessage::ai(MessageContent::Blocks(blocks))
        }
    }
}

#[async_trait]
impl BaseModel for ChatOpenAI {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let chat_url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let tools_json: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            }))
            .collect();

        let mut messages = Self::messages_to_json(&request.messages);

        if let Some(base_system) = &request.system {
            if let Some(first) = messages.first_mut() {
                if first["role"] == "system" {
                    // 消息列表中已有 System（来自中间件，如 agent.md），追加基础提示词
                    let existing = first["content"].as_str().unwrap_or("").to_string();
                    first["content"] = json!(format!("{}\n\n{}", existing, base_system));
                } else {
                    messages.insert(0, json!({ "role": "system", "content": base_system }));
                }
            } else {
                messages.insert(0, json!({ "role": "system", "content": base_system }));
            }
        }

        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": false
        });

        if !tools_json.is_empty() {
            body["tools"] = Value::Array(tools_json);
            body["tool_choice"] = json!("auto");
        }

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }

        if let Some(ref effort) = self.reasoning_effort {
            // o1/o3 reasoning effort 模式：加 reasoning_effort，不设 temperature
            body["reasoning_effort"] = json!(effort);
        } else if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }

        let resp = self
            .client
            .post(&chat_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let status = resp.status();
        let resp_json: Value = resp
            .json()
            .await
            .map_err(|e| AgentError::LlmError(format!("解析响应失败: {e}")))?;

        if !status.is_success() {
            let msg = resp_json["error"]["message"]
                .as_str()
                .unwrap_or("未知错误")
                .to_string();
            return Err(AgentError::LlmError(format!("API 错误 {status}: {msg}")));
        }

        let choice = &resp_json["choices"][0];
        let finish_reason = choice["finish_reason"].as_str().unwrap_or("stop");
        let stop_reason = StopReason::from_openai(finish_reason);
        let assistant_msg = &choice["message"];

        let message = Self::parse_assistant_message(assistant_msg, &stop_reason);

        Ok(LlmResponse { message, stop_reason })
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl ReactLLM for ChatOpenAI {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
    ) -> AgentResult<Reasoning> {
        let tool_defs = tools.iter().map(|t| t.definition()).collect();
        let request = LlmRequest::new(messages.to_vec()).with_tools(tool_defs);

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
