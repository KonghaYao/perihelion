use serde::{Deserialize, Serialize};

/// 消息唯一标识符 — UUID v7（时间有序，跨进程安全）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(uuid::Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

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
    Human { id: MessageId, content: MessageContent },

    #[serde(rename = "assistant")]
    Ai {
        id: MessageId,
        content: MessageContent,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCallRequest>,
    },

    #[serde(rename = "system")]
    System { id: MessageId, content: MessageContent },

    #[serde(rename = "tool")]
    Tool {
        id: MessageId,
        tool_call_id: String,
        content: MessageContent,
        #[serde(default)]
        is_error: bool,
    },
}

impl BaseMessage {
    // ── 构造器 ────────────────────────────────────────────────────────────────

    pub fn human(content: impl Into<MessageContent>) -> Self {
        Self::Human { id: MessageId::new(), content: content.into() }
    }

    pub fn ai(content: impl Into<MessageContent>) -> Self {
        Self::Ai {
            id: MessageId::new(),
            content: content.into(),
            tool_calls: Vec::new(),
        }
    }

    pub fn ai_with_tool_calls(
        content: impl Into<MessageContent>,
        tool_calls: Vec<ToolCallRequest>,
    ) -> Self {
        Self::Ai {
            id: MessageId::new(),
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
            id: MessageId::new(),
            content: MessageContent::Blocks(blocks),
            tool_calls,
        }
    }

    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self::System { id: MessageId::new(), content: content.into() }
    }

    pub fn tool_result(id: impl Into<String>, content: impl Into<MessageContent>) -> Self {
        Self::Tool {
            id: MessageId::new(),
            tool_call_id: id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    pub fn tool_error(id: impl Into<String>, error: impl Into<MessageContent>) -> Self {
        Self::Tool {
            id: MessageId::new(),
            tool_call_id: id.into(),
            content: error.into(),
            is_error: true,
        }
    }

    // ── 访问器 ────────────────────────────────────────────────────────────────

    /// 获取消息 ID
    pub fn id(&self) -> MessageId {
        match self {
            Self::Human { id, .. } => *id,
            Self::Ai { id, .. } => *id,
            Self::System { id, .. } => *id,
            Self::Tool { id, .. } => *id,
        }
    }

    /// 获取消息 `MessageContent` 引用
    pub fn message_content(&self) -> &MessageContent {
        match self {
            Self::Human { content, .. } => content,
            Self::Ai { content, .. } => content,
            Self::System { content, .. } => content,
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

    #[test]
    fn test_message_id_generated() {
        // 不同消息的 id 应不同
        let m1 = BaseMessage::human("hello");
        let m2 = BaseMessage::human("hello");
        assert_ne!(m1.id(), m2.id(), "两条消息 id 应不同");

        // 序列化/反序列化后 id 保持一致
        let json = serde_json::to_string(&m1).unwrap();
        let restored: BaseMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id(), m1.id(), "反序列化后 id 应保持不变");
    }

    #[test]
    fn test_tool_call_id_persistence() {
        // 模拟完整的工具调用流程：
        // 1. AI 消息包含 tool_calls（id=toolu_123）
        // 2. Tool 消息的 tool_call_id 也是 toolu_123
        use crate::messages::ContentBlock;
        let blocks = vec![
            ContentBlock::text("I'll read a file"),
            ContentBlock::tool_use("toolu_123", "read_file", serde_json::json!({"path": "test.txt"})),
        ];
        let ai_msg = BaseMessage::ai_from_blocks(blocks);

        // 验证 AI 消息包含 tool_calls
        let tcs = ai_msg.tool_calls();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "toolu_123");
        assert_eq!(tcs[0].name, "read_file");

        // 序列化
        let json = serde_json::to_string(&ai_msg).unwrap();

        // 反序列化
        let restored: BaseMessage = serde_json::from_str(&json).unwrap();

        // 验证 tool_calls 仍然存在
        let tcs = restored.tool_calls();
        assert_eq!(tcs.len(), 1, "反序列化后 tool_calls 应该保留");
        assert_eq!(tcs[0].id, "toolu_123");

        // 模拟 Tool 消息
        let tool_msg = BaseMessage::tool_result("toolu_123", "file content");
        let tool_json = serde_json::to_string(&tool_msg).unwrap();
        let restored_tool: BaseMessage = serde_json::from_str(&tool_json).unwrap();

        if let BaseMessage::Tool { tool_call_id, .. } = restored_tool {
            assert_eq!(tool_call_id, "toolu_123");
        } else {
            panic!("Tool 消息反序列化失败");
        }
    }
}
