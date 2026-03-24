use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
use super::MessageAdapter;

/// Anthropic Messages API 格式的消息适配器
pub struct AnthropicAdapter;

impl AnthropicAdapter {
    fn block_to_anthropic(block: &ContentBlock) -> Option<Value> {
        match block {
            ContentBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
            ContentBlock::Image { source } => match source {
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
            },
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
                    .filter_map(Self::block_to_anthropic)
                    .collect();
                Some(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content_val,
                    "is_error": is_error
                }))
            }
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
                    .filter_map(Self::block_to_anthropic)
                    .collect();
                Value::Array(parts)
            }
            MessageContent::Raw(values) => Value::Array(values.clone()),
        }
    }

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
                    let id = b["id"].as_str().unwrap_or("").to_string();
                    let name = b["name"].as_str().unwrap_or("").to_string();
                    let input = b["input"].clone();
                    blocks.push(ContentBlock::tool_use(&id, &name, input.clone()));
                    tool_calls.push(ToolCallRequest::new(id, name, input));
                }
                _ => {
                    blocks.push(ContentBlock::Unknown);
                }
            }
        }
        (blocks, tool_calls)
    }
}

impl MessageAdapter for AnthropicAdapter {
    /// BaseMessage[] → Anthropic messages JSON 数组
    ///
    /// - System 消息提取为第一条（系统角色），实际上 Anthropic 的 system 字段需要单独提取，
    ///   但本适配器将其作为 system role 消息插入（与 OpenAI 格式兼容），
    ///   使用 `from_base_messages_with_system` 可同时获得 system 字符串
    /// - Tool 消息合并到前一条 user 消息的 content blocks
    fn from_base_messages(messages: &[BaseMessage]) -> Value {
        let (msgs, _system) = Self::to_anthropic_with_system(messages);
        Value::Array(msgs)
    }

    /// Anthropic 原生 message JSON → BaseMessage
    fn to_base_message(value: &Value) -> Result<BaseMessage> {
        let role = value["role"].as_str().ok_or_else(|| anyhow!("缺少 role 字段"))?;
        match role {
            "user" => {
                let content = parse_anthropic_content(&value["content"]);
                Ok(BaseMessage::human(content))
            }
            "assistant" => {
                match &value["content"] {
                    Value::String(s) => Ok(BaseMessage::ai(s.clone())),
                    Value::Array(blocks) => {
                        let (content_blocks, tool_calls) =
                            Self::parse_content_blocks(blocks);
                        if tool_calls.is_empty() {
                            if content_blocks.len() == 1 {
                                if let ContentBlock::Text { text } = &content_blocks[0] {
                                    return Ok(BaseMessage::ai(text.clone()));
                                }
                            }
                            Ok(BaseMessage::ai(MessageContent::Blocks(content_blocks)))
                        } else {
                            Ok(BaseMessage::ai_with_tool_calls(
                                MessageContent::Blocks(content_blocks),
                                tool_calls,
                            ))
                        }
                    }
                    _ => Ok(BaseMessage::ai("")),
                }
            }
            "system" => {
                let content = parse_anthropic_content(&value["content"]);
                Ok(BaseMessage::system(content))
            }
            other => Err(anyhow!("未知 role: {other}")),
        }
    }
}

impl AnthropicAdapter {
    /// 返回 (messages_array, system_text)
    /// system_text 可用于 Anthropic API 的顶层 system 字段
    pub fn to_anthropic_with_system(messages: &[BaseMessage]) -> (Vec<Value>, Option<String>) {
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

                    let should_append = result
                        .last()
                        .map(|last| last["role"] == "user" && last["content"].is_array())
                        .unwrap_or(false);

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
}

fn parse_anthropic_content(content: &Value) -> MessageContent {
    match content {
        Value::String(s) => MessageContent::text(s.clone()),
        Value::Array(blocks) => {
            let parsed: Vec<ContentBlock> = blocks
                .iter()
                .filter_map(|b| {
                    match b["type"].as_str() {
                        Some("text") => {
                            Some(ContentBlock::text(b["text"].as_str().unwrap_or("")))
                        }
                        Some("tool_result") => {
                            // tool_result 在 user 消息中，保留为 Unknown
                            Some(ContentBlock::Unknown)
                        }
                        _ => None,
                    }
                })
                .collect();
            if parsed.is_empty() {
                MessageContent::text("")
            } else {
                MessageContent::Blocks(parsed)
            }
        }
        Value::Null => MessageContent::text(""),
        _ => MessageContent::text(content.to_string()),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_base_messages_basic() {
        let msgs = vec![
            BaseMessage::human("Hello"),
            BaseMessage::ai("Hi"),
        ];
        let val = AnthropicAdapter::from_base_messages(&msgs);
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "user");
        assert_eq!(arr[1]["role"], "assistant");
    }

    #[test]
    fn test_from_base_messages_tool_use_merged() {
        let msgs = vec![
            BaseMessage::ai_with_tool_calls(
                "",
                vec![ToolCallRequest::new("tc1", "bash", json!({"command": "ls"}))],
            ),
            BaseMessage::tool_result("tc1", "file.txt"),
        ];
        let val = AnthropicAdapter::from_base_messages(&msgs);
        let arr = val.as_array().unwrap();
        // tool result 应合并到 user 消息
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "assistant");
        // 第二条 - tool result 合并为 user
        assert_eq!(arr[1]["role"], "user");
        let content = arr[1]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
    }

    #[test]
    fn test_system_extracted() {
        let msgs = vec![
            BaseMessage::system("You are helpful"),
            BaseMessage::human("Hello"),
        ];
        let (msgs_val, system) = AnthropicAdapter::to_anthropic_with_system(&msgs);
        assert_eq!(system.as_deref(), Some("You are helpful"));
        // system 消息不进入 messages 数组
        assert_eq!(msgs_val.len(), 1);
        assert_eq!(msgs_val[0]["role"], "user");
    }

    #[test]
    fn test_to_base_message_assistant_with_tool_use() {
        let val = json!({
            "role": "assistant",
            "content": [
                { "type": "text", "text": "I'll run bash" },
                { "type": "tool_use", "id": "tc1", "name": "bash", "input": {"command": "ls"} }
            ]
        });
        let msg = AnthropicAdapter::to_base_message(&val).unwrap();
        assert!(msg.has_tool_calls());
        assert_eq!(msg.tool_calls()[0].name, "bash");
    }

    #[test]
    fn test_to_base_message_roundtrip() {
        let original = BaseMessage::human("Test");
        let val = AnthropicAdapter::from_base_messages(&[original]);
        let arr = val.as_array().unwrap();
        let restored = AnthropicAdapter::to_base_message(&arr[0]).unwrap();
        assert_eq!(restored.content(), "Test");
    }

    /// 双写一致性 roundtrip：Ai 消息经过序列化→API→反序列化后，
    /// content blocks 中的 ToolUse 与 tool_calls 字段始终保持同步
    #[test]
    fn test_tool_calls_dual_write_roundtrip() {
        // 构造包含工具调用的 AI 消息（模拟 LLM 响应解析后的内部状态）
        let original = BaseMessage::ai_from_blocks(vec![
            ContentBlock::text("I'll run bash"),
            ContentBlock::tool_use("tc1", "bash", json!({"command": "ls"})),
        ]);
        assert!(original.has_tool_calls());
        assert_eq!(original.tool_calls().len(), 1);

        // 序列化为 Anthropic API 格式
        let api_json = AnthropicAdapter::from_base_messages(&[original]);
        let arr = api_json.as_array().unwrap();
        let assistant_msg = &arr[0];
        assert_eq!(assistant_msg["role"], "assistant");

        // API 格式应包含 tool_use block
        let blocks = assistant_msg["content"].as_array().unwrap();
        let has_tool_use = blocks.iter().any(|b| b["type"] == "tool_use");
        assert!(has_tool_use, "序列化后 content 应包含 tool_use block");

        // 反序列化回 BaseMessage，双写应仍然一致
        let restored = AnthropicAdapter::to_base_message(assistant_msg).unwrap();
        assert!(restored.has_tool_calls(), "反序列化后 tool_calls 应保留");
        assert_eq!(restored.tool_calls().len(), 1);
        assert_eq!(restored.tool_calls()[0].id, "tc1");
        assert_eq!(restored.tool_calls()[0].name, "bash");

        // content blocks 中也应有 ToolUse（双写一致性验证）
        let content_has_tool_use = restored
            .content_blocks()
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
        assert!(content_has_tool_use, "content blocks 中应有 ToolUse block");
    }

    /// Text 类型内容 + tool_calls 的序列化：应从 tool_calls 重建 ToolUse blocks
    #[test]
    fn test_text_content_with_tool_calls_serializes_correctly() {
        let msg = BaseMessage::ai_with_tool_calls(
            "I'll run bash",
            vec![ToolCallRequest::new("tc2", "bash", json!({"command": "pwd"}))],
        );
        let api_json = AnthropicAdapter::from_base_messages(&[msg]);
        let arr = api_json.as_array().unwrap();
        let blocks = arr[0]["content"].as_array().unwrap();

        let text_block = blocks.iter().find(|b| b["type"] == "text");
        let tool_block = blocks.iter().find(|b| b["type"] == "tool_use");
        assert!(text_block.is_some(), "应包含 text block");
        assert!(tool_block.is_some(), "应从 tool_calls 重建 tool_use block");
        assert_eq!(tool_block.unwrap()["id"], "tc2");
    }
}
