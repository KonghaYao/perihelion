use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
use super::MessageAdapter;

/// OpenAI 兼容格式的消息适配器
pub struct OpenAiAdapter;

impl OpenAiAdapter {
    fn content_to_openai(content: &MessageContent) -> Value {
        match content {
            MessageContent::Text(s) => json!(s),
            MessageContent::Blocks(blocks) => {
                let parts: Vec<Value> = blocks
                    .iter()
                    .filter_map(Self::block_to_openai_part)
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
            ContentBlock::ToolUse { .. } | ContentBlock::ToolResult { .. } => None,
            ContentBlock::Reasoning { .. } => None,
            ContentBlock::Document { source, title } => {
                let src = serde_json::to_value(source).unwrap_or_default();
                Some(json!({ "type": "document", "source": src, "title": title }))
            }
            ContentBlock::Unknown => None,
        }
    }
}

impl MessageAdapter for OpenAiAdapter {
    /// BaseMessage[] → OpenAI messages JSON 数组
    ///
    /// - System 消息合并为第一条 system 角色消息
    /// - Tool 消息用 role: "tool" + tool_call_id 格式
    fn from_base_messages(messages: &[BaseMessage]) -> Value {
        let mut system_parts: Vec<String> = Vec::new();
        let mut result: Vec<Value> = Vec::new();

        for m in messages {
            match m {
                BaseMessage::System { content } => {
                    let t = content.text_content();
                    if !t.trim().is_empty() {
                        system_parts.push(t);
                    }
                }
                BaseMessage::Human { content } => {
                    result.push(json!({
                        "role": "user",
                        "content": Self::content_to_openai(content)
                    }));
                }
                BaseMessage::Ai { content, tool_calls } => {
                    if tool_calls.is_empty() {
                        result.push(json!({
                            "role": "assistant",
                            "content": Self::content_to_openai(content)
                        }));
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

        if !system_parts.is_empty() {
            result.insert(0, json!({ "role": "system", "content": system_parts.join("\n\n") }));
        }

        Value::Array(result)
    }

    /// OpenAI 原生 message JSON → BaseMessage
    fn to_base_message(value: &Value) -> Result<BaseMessage> {
        let role = value["role"].as_str().ok_or_else(|| anyhow!("缺少 role 字段"))?;
        match role {
            "user" => {
                let content = parse_openai_content(&value["content"]);
                Ok(BaseMessage::human(content))
            }
            "assistant" => {
                let content_str = value["content"].as_str().unwrap_or("").to_string();
                let mut blocks: Vec<ContentBlock> = Vec::new();

                // reasoning_content（deepseek-r1 等）
                if let Some(reasoning) = value["reasoning_content"].as_str() {
                    if !reasoning.is_empty() {
                        blocks.push(ContentBlock::reasoning(reasoning));
                    }
                }
                if !content_str.is_empty() {
                    blocks.push(ContentBlock::text(content_str.clone()));
                }

                let tool_calls_arr = value["tool_calls"].as_array();
                if let Some(tcs_raw) = tool_calls_arr {
                    let tool_calls: Vec<ToolCallRequest> = tcs_raw
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
                        MessageContent::text(content_str)
                    } else if blocks.is_empty() {
                        MessageContent::default()
                    } else {
                        MessageContent::Blocks(blocks)
                    };
                    Ok(BaseMessage::ai_with_tool_calls(content, tool_calls))
                } else {
                    // 普通文本回复
                    Ok(BaseMessage::ai(content_str))
                }
            }
            "system" => {
                let content = parse_openai_content(&value["content"]);
                Ok(BaseMessage::system(content))
            }
            "tool" => {
                let tool_call_id = value["tool_call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("tool 消息缺少 tool_call_id"))?;
                let content = parse_openai_content(&value["content"]);
                Ok(BaseMessage::tool_result(tool_call_id, content))
            }
            other => Err(anyhow!("未知 role: {other}")),
        }
    }
}

/// 将 OpenAI content 字段（string 或 array）解析为 MessageContent
fn parse_openai_content(content: &Value) -> MessageContent {
    match content {
        Value::String(s) => MessageContent::text(s.clone()),
        Value::Array(parts) => {
            let blocks: Vec<ContentBlock> = parts
                .iter()
                .filter_map(|part| {
                    match part["type"].as_str() {
                        Some("text") => {
                            let text = part["text"].as_str().unwrap_or("").to_string();
                            Some(ContentBlock::text(text))
                        }
                        Some("image_url") => {
                            let url = part["image_url"]["url"].as_str().unwrap_or("").to_string();
                            Some(ContentBlock::image_url(url))
                        }
                        _ => None,
                    }
                })
                .collect();
            if blocks.is_empty() {
                MessageContent::text("")
            } else {
                MessageContent::Blocks(blocks)
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
    fn test_from_base_messages_human_ai() {
        let msgs = vec![
            BaseMessage::human("Hello"),
            BaseMessage::ai("Hi"),
        ];
        let val = OpenAiAdapter::from_base_messages(&msgs);
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "user");
        assert_eq!(arr[1]["role"], "assistant");
    }

    #[test]
    fn test_from_base_messages_system_prepended() {
        let msgs = vec![
            BaseMessage::system("You are helpful"),
            BaseMessage::human("Hello"),
        ];
        let val = OpenAiAdapter::from_base_messages(&msgs);
        let arr = val.as_array().unwrap();
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[0]["content"], "You are helpful");
    }

    #[test]
    fn test_from_base_messages_tool() {
        let msgs = vec![
            BaseMessage::ai_with_tool_calls(
                "",
                vec![ToolCallRequest::new("tc1", "bash", json!({"command": "ls"}))],
            ),
            BaseMessage::tool_result("tc1", "file.txt"),
        ];
        let val = OpenAiAdapter::from_base_messages(&msgs);
        let arr = val.as_array().unwrap();
        assert_eq!(arr[0]["role"], "assistant");
        assert!(arr[0]["tool_calls"].is_array());
        assert_eq!(arr[1]["role"], "tool");
        assert_eq!(arr[1]["tool_call_id"], "tc1");
    }

    #[test]
    fn test_to_base_message_roundtrip() {
        let original = BaseMessage::human("Test message");
        let val = OpenAiAdapter::from_base_messages(&[original]);
        let arr = val.as_array().unwrap();
        let restored = OpenAiAdapter::to_base_message(&arr[0]).unwrap();
        assert_eq!(restored.content(), "Test message");
    }

    #[test]
    fn test_to_base_message_tool() {
        let val = json!({
            "role": "tool",
            "tool_call_id": "tc1",
            "content": "result"
        });
        let msg = OpenAiAdapter::to_base_message(&val).unwrap();
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            assert_eq!(tool_call_id, "tc1");
        } else {
            panic!("期望 Tool 消息");
        }
    }

    /// 双写一致性 roundtrip：从 OpenAI API 响应解析后，
    /// content blocks 中的 ToolUse 与 tool_calls 字段始终同步
    #[test]
    fn test_tool_calls_dual_write_roundtrip() {
        // 模拟 OpenAI API 返回包含工具调用的 assistant 消息
        let api_response = json!({
            "role": "assistant",
            "content": "I'll run bash",
            "tool_calls": [{
                "id": "tc1",
                "type": "function",
                "function": {
                    "name": "bash",
                    "arguments": "{\"command\":\"ls\"}"
                }
            }]
        });

        let msg = OpenAiAdapter::to_base_message(&api_response).unwrap();

        // tool_calls 字段应正确提取
        assert!(msg.has_tool_calls());
        assert_eq!(msg.tool_calls().len(), 1);
        assert_eq!(msg.tool_calls()[0].id, "tc1");
        assert_eq!(msg.tool_calls()[0].name, "bash");

        // content blocks 中也应有 ToolUse（双写一致）
        let has_tool_use = msg
            .content_blocks()
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
        assert!(has_tool_use, "content blocks 中应有 ToolUse block");

        // 序列化回 OpenAI 格式后，tool_calls 字段应存在（OpenAI 用 tool_calls 字段，不在 content 里）
        let re_serialized = OpenAiAdapter::from_base_messages(&[msg]);
        let arr = re_serialized.as_array().unwrap();
        // system prompt prepended if any, here just one assistant msg
        let assistant = arr.iter().find(|m| m["role"] == "assistant").unwrap();
        assert!(assistant["tool_calls"].is_array());
        assert_eq!(assistant["tool_calls"][0]["id"], "tc1");
        // OpenAI content 不含 ToolUse（已过滤），只保留 text
        let content_has_tool_use = assistant["content"]
            .as_array()
            .map(|arr| arr.iter().any(|b| b["type"] == "tool_use"))
            .unwrap_or(false);
        assert!(!content_has_tool_use, "OpenAI content 中不应出现 ToolUse block");
    }
}
