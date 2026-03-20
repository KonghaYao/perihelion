use serde::{Deserialize, Serialize};

use super::content::{ContentBlock, MessageContent};

// ─── ToolCallRequest ──────────────────────────────────────────────────────────

/// 工具调用请求（对应 OpenAI tool_calls / Anthropic tool_use blocks）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl ToolCallRequest {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

// ─── BaseMessage ──────────────────────────────────────────────────────────────

/// BaseMessage - 统一消息类型，对齐 LangChain BaseMessage
///
/// `content` 字段为 `MessageContent`，支持：
/// - 纯文本字符串
/// - 标准 ContentBlock 列表（多模态、推理内容等）
/// - Provider 原生格式（透传）
///
/// `content_blocks()` 方法懒解析，对齐 LangChain JS 的 `contentBlocks` 属性。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum BaseMessage {
    #[serde(rename = "user")]
    Human { content: MessageContent },

    #[serde(rename = "assistant")]
    Ai {
        content: MessageContent,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCallRequest>,
    },

    #[serde(rename = "system")]
    System { content: MessageContent },

    #[serde(rename = "tool")]
    Tool {
        tool_call_id: String,
        content: MessageContent,
        #[serde(default)]
        is_error: bool,
    },
}

impl BaseMessage {
    // ── 构造器 ────────────────────────────────────────────────────────────────

    pub fn human(content: impl Into<MessageContent>) -> Self {
        Self::Human { content: content.into() }
    }

    pub fn ai(content: impl Into<MessageContent>) -> Self {
        Self::Ai {
            content: content.into(),
            tool_calls: Vec::new(),
        }
    }

    pub fn ai_with_tool_calls(
        content: impl Into<MessageContent>,
        tool_calls: Vec<ToolCallRequest>,
    ) -> Self {
        Self::Ai {
            content: content.into(),
            tool_calls,
        }
    }

    /// 构造带 ContentBlock 列表的 AI 消息（含工具调用 block）
    ///
    /// `blocks` 中的 `ToolUse` block 会被同步提取到 `tool_calls`，保持一致性。
    pub fn ai_from_blocks(blocks: Vec<ContentBlock>) -> Self {
        let tool_calls: Vec<ToolCallRequest> = blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, name, input } = b {
                    Some(ToolCallRequest::new(id.clone(), name.clone(), input.clone()))
                } else {
                    None
                }
            })
            .collect();
        Self::Ai {
            content: MessageContent::Blocks(blocks),
            tool_calls,
        }
    }

    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self::System { content: content.into() }
    }

    pub fn tool_result(id: impl Into<String>, content: impl Into<MessageContent>) -> Self {
        Self::Tool {
            tool_call_id: id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    pub fn tool_error(id: impl Into<String>, error: impl Into<MessageContent>) -> Self {
        Self::Tool {
            tool_call_id: id.into(),
            content: error.into(),
            is_error: true,
        }
    }

    // ── 访问器 ────────────────────────────────────────────────────────────────

    /// 获取消息 `MessageContent` 引用
    pub fn message_content(&self) -> &MessageContent {
        match self {
            Self::Human { content } => content,
            Self::Ai { content, .. } => content,
            Self::System { content } => content,
            Self::Tool { content, .. } => content,
        }
    }

    /// 获取纯文本内容（拼接所有 text block）
    pub fn content(&self) -> String {
        self.message_content().text_content()
    }

    /// 懒解析为标准 ContentBlock 列表
    ///
    /// 对齐 LangChain JS 的 `message.contentBlocks` 属性。
    pub fn content_blocks(&self) -> Vec<ContentBlock> {
        self.message_content().content_blocks()
    }

    /// 是否包含工具调用
    pub fn has_tool_calls(&self) -> bool {
        match self {
            Self::Ai { tool_calls, .. } => !tool_calls.is_empty(),
            _ => false,
        }
    }

    /// 获取工具调用列表（仅 Ai 变体有效）
    pub fn tool_calls(&self) -> &[ToolCallRequest] {
        match self {
            Self::Ai { tool_calls, .. } => tool_calls,
            _ => &[],
        }
    }

    /// 是否为系统消息
    pub fn is_system(&self) -> bool {
        matches!(self, Self::System { .. })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_from_blocks_extracts_tool_calls() {
        let blocks = vec![
            ContentBlock::text("I'll use a tool"),
            ContentBlock::tool_use("id1", "bash", serde_json::json!({"command": "ls"})),
        ];
        let msg = BaseMessage::ai_from_blocks(blocks);
        assert!(msg.has_tool_calls());
        assert_eq!(msg.tool_calls().len(), 1);
        assert_eq!(msg.tool_calls()[0].name, "bash");
    }

    #[test]
    fn test_base_message_content_blocks_lazy_parse() {
        let msg = BaseMessage::ai(MessageContent::Blocks(vec![
            ContentBlock::reasoning("thinking..."),
            ContentBlock::text("answer"),
        ]));
        let blocks = msg.content_blocks();
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], ContentBlock::Reasoning { .. }));
        assert_eq!(blocks[1].as_text(), Some("answer"));
    }

    #[test]
    fn test_human_message_multimodal() {
        let msg = BaseMessage::human(MessageContent::Blocks(vec![
            ContentBlock::text("What's in this image?"),
            ContentBlock::image_url("https://example.com/image.jpg"),
        ]));
        let blocks = msg.content_blocks();
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[1], ContentBlock::Image { .. }));
    }
}
