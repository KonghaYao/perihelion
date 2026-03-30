use ratatui::style::Color;
use ratatui::text::Text;
use crate::ui::theme;
use rust_create_agent::messages::{BaseMessage, ContentBlock};

use super::markdown::parse_markdown;

/// 渲染层的视图模型，从 BaseMessage/AgentEvent 转换而来
#[derive(Debug, Clone)]
pub enum MessageViewModel {
    /// 用户输入
    UserBubble {
        #[allow(dead_code)]
        content: String,
        rendered: Text<'static>,
    },
    /// AI 回复（支持流式追加）
    AssistantBubble {
        blocks: Vec<ContentBlockView>,
        is_streaming: bool,
        /// 折叠状态：true 表示完全隐藏，false 表示展开显示
        collapsed: bool,
    },
    /// 工具调用结果
    ToolBlock {
        #[allow(dead_code)]
        tool_name: String,
        display_name: String,
        args_display: Option<String>,
        content: String,
        is_error: bool,
        collapsed: bool,
        color: Color,
    },
    /// 系统消息
    SystemNote { content: String },
    /// SubAgent 执行块（可折叠，含滑动窗口消息）
    SubAgentGroup {
        agent_id: String,
        task_preview: String,
        /// 总步数（工具调用 + AI 回复），不受滑动窗口截断影响
        total_steps: usize,
        /// 滑动窗口，最多 4 条最近消息
        recent_messages: Vec<MessageViewModel>,
        /// 子 agent 执行中为 true
        is_running: bool,
        /// 默认展开，完成后用户可折叠
        collapsed: bool,
        /// SubAgentEnd 携带的结果摘要（工具返回值）
        final_result: Option<String>,
    },
}

/// ContentBlock 的视图化表示
#[derive(Debug, Clone)]
pub enum ContentBlockView {
    /// 文本内容（含 markdown 解析缓存）
    Text {
        raw: String,
        rendered: Text<'static>,
        dirty: bool,
    },
    /// 推理/思考过程（仅显示字数摘要）
    Reasoning { char_count: usize },
    /// 工具使用请求（AI 发起的调用请求）
    ToolUse { name: String },
}

impl MessageViewModel {
    /// 从 BaseMessage 转换为视图模型
    ///
    /// `prev_ai_tool_calls` 用于为 Tool 消息提供工具名和参数（BaseMessage::Tool 只存储 tool_use_id）
    pub fn from_base_message(msg: &BaseMessage, prev_ai_tool_calls: &[(String, String, serde_json::Value)]) -> Self {
        match msg {
            BaseMessage::Human { content, .. } => {
                let raw = content.text_content();
                let rendered = parse_markdown(&raw);
                MessageViewModel::UserBubble {
                    content: raw,
                    rendered,
                }
            }
            BaseMessage::Ai {
                content,
                tool_calls,
                ..
            } => {
                // 先处理 content 中的 blocks
                let mut blocks: Vec<ContentBlockView> = content
                    .content_blocks()
                    .into_iter()
                    .map(|block| match block {
                        ContentBlock::Text { text } => ContentBlockView::Text {
                            raw: text.clone(),
                            rendered: parse_markdown(&text),
                            dirty: false,
                        },
                        ContentBlock::Reasoning { text, .. } => ContentBlockView::Reasoning {
                            char_count: text.chars().count(),
                        },
                        ContentBlock::ToolUse {
                            name, ..
                        } => {
                            ContentBlockView::ToolUse {
                                name,
                            }
                        }
                        _ => ContentBlockView::Text {
                            raw: String::new(),
                            rendered: Text::raw(""),
                            dirty: false,
                        },
                    })
                    .collect();

                // 补充 tool_calls 字段中的工具调用（当 content 中没有对应的 ToolUse block 时）
                // 避免重复：如果 content_blocks 中已有同名 ToolUse，跳过
                let existing_tool_names: std::collections::HashSet<String> = blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlockView::ToolUse { name } = b {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                for tc in tool_calls {
                    if !existing_tool_names.contains(&tc.name) {
                        blocks.push(ContentBlockView::ToolUse {
                            name: tc.name.clone(),
                        });
                    }
                }

                MessageViewModel::AssistantBubble {
                    blocks,
                    is_streaming: false,
                    collapsed: false,
                }
            }
            BaseMessage::Tool {
                tool_call_id,
                content,
                is_error,
                ..
            } => {
                // 从前一条 Ai 消息的 tool_calls 中查找工具名和参数
                let (tool_name, input) = prev_ai_tool_calls
                    .iter()
                    .find(|(id, _, _)| id == tool_call_id)
                    .map(|(_, name, input)| (name.clone(), input.clone()))
                    .unwrap_or_else(|| (tool_call_id.clone(), serde_json::Value::Null));
                let raw_content = content.text_content();
                // launch_agent 工具恢复为 SubAgentGroup（完成状态，折叠）
                if tool_name == "launch_agent" {
                    let agent_id = input["agent_id"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string();
                    let task_preview = input["task"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(40)
                        .collect::<String>();
                    return MessageViewModel::SubAgentGroup {
                        agent_id,
                        task_preview,
                        total_steps: 0, // 历史恢复时无法得知总步数
                        recent_messages: Vec::new(), // 子 agent 内部消息不持久化
                        is_running: false,
                        collapsed: true,
                        final_result: Some(raw_content),
                    };
                }
                // 使用统一格式化函数生成 display_name（与实时流式一致）
                let display_name = crate::app::tool_display::format_tool_name(&tool_name);
                let args_display = crate::app::tool_display::format_tool_args(&tool_name, &input, None);
                let color = if *is_error {
                    theme::ERROR
                } else {
                    tool_color(&tool_name)
                };
                MessageViewModel::ToolBlock {
                    tool_name,
                    display_name,
                    args_display,
                    content: raw_content,
                    is_error: *is_error,
                    collapsed: true,
                    color,
                }
            }
            BaseMessage::System { content, .. } => MessageViewModel::SystemNote {
                content: content.text_content(),
            },
        }
    }

    /// 追加流式文本 chunk
    pub fn append_chunk(&mut self, chunk: &str) {
        if let MessageViewModel::AssistantBubble { blocks, collapsed, .. } = self {
            // 如果有内容追加，自动展开
            if *collapsed && !chunk.is_empty() {
                *collapsed = false;
            }
            if let Some(ContentBlockView::Text { raw, dirty, .. }) = blocks.last_mut() {
                raw.push_str(chunk);
                *dirty = true;
                return;
            }
            // 没有 Text block，创建新的
            let mut raw = String::new();
            raw.push_str(chunk);
            blocks.push(ContentBlockView::Text {
                raw,
                rendered: Text::raw(""),
                dirty: true,
            });
        }
    }

    /// 切换折叠状态（对 ToolBlock、AssistantBubble、SubAgentGroup 生效）
    #[allow(dead_code)]
    pub fn toggle_collapse(&mut self) {
        match self {
            MessageViewModel::ToolBlock { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            MessageViewModel::AssistantBubble { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            MessageViewModel::SubAgentGroup { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            _ => {}
        }
    }

    /// 判断是否为 AssistantBubble
    pub fn is_assistant(&self) -> bool {
        matches!(self, MessageViewModel::AssistantBubble { .. })
    }

    /// 创建用户消息
    pub fn user(content: String) -> Self {
        let rendered = parse_markdown(&content);
        MessageViewModel::UserBubble { content, rendered }
    }

    /// 创建助手消息
    pub fn assistant() -> Self {
        MessageViewModel::AssistantBubble {
            blocks: Vec::new(),
            is_streaming: true,
            collapsed: false,
        }
    }

    /// 创建工具消息
    pub fn tool_block(tool_name: String, display: String, args: Option<String>, is_error: bool) -> Self {
        let color = if is_error {
            theme::ERROR
        } else {
            tool_color(&tool_name)
        };
        MessageViewModel::ToolBlock {
            tool_name,
            display_name: display,
            args_display: args,
            content: String::new(),
            is_error,
            collapsed: true,
            color,
        }
    }

    /// 创建系统消息
    pub fn system(content: String) -> Self {
        MessageViewModel::SystemNote { content }
    }

    /// 创建 SubAgentGroup（初始状态：运行中、展开、0 步）
    pub fn subagent_group(agent_id: String, task_preview: String) -> Self {
        MessageViewModel::SubAgentGroup {
            agent_id,
            task_preview,
            total_steps: 0,
            recent_messages: Vec::new(),
            is_running: true,
            collapsed: false,
            final_result: None,
        }
    }

    /// 判断是否为 SubAgentGroup
    pub fn is_subagent_group(&self) -> bool {
        matches!(self, MessageViewModel::SubAgentGroup { .. })
    }

}

/// 按工具名分配颜色
pub fn tool_color(name: &str) -> Color {
    match name {
        "bash" => theme::WARNING,
        "write_file" | "edit_file" | "folder_operations"
        | "delete_file" | "delete_folder" | "rm" | "rm_rf" => theme::WARNING,
        "read_file" | "glob_files" | "search_files_rg"
        | "launch_agent" | "ask_user_question" | "todo_write" => theme::MUTED,
        _ if name.contains("error") => theme::ERROR,
        _ => theme::MUTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::messages::{MessageContent, ToolCallRequest};
    use serde_json::json;

    /// 测试：AI 消息只有 tool_calls（无 content）时，应正确渲染工具调用
    #[test]
    fn test_ai_message_with_only_tool_calls_renders_tool_use() {
        // 模拟：AI 消息只包含 tool_calls，content 为空
        let msg = BaseMessage::ai_with_tool_calls(
            MessageContent::text(""),
            vec![
                ToolCallRequest::new("toolu_001", "bash", json!({"command": "ls"})),
                ToolCallRequest::new("toolu_002", "read_file", json!({"path": "test.txt"})),
            ],
        );

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                // 应该有 2 个 ToolUse block
                let tool_uses: Vec<_> = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                    .collect();
                assert_eq!(tool_uses.len(), 2, "应该有 2 个 ToolUse block");

                // 验证工具名称
                let names: Vec<&str> = blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlockView::ToolUse { name } = b {
                            Some(name.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(names.contains(&"bash"), "应包含 bash 工具");
                assert!(names.contains(&"read_file"), "应包含 read_file 工具");
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    /// 测试：AI 消息同时有文本和 tool_calls 时，两者都应渲染
    #[test]
    fn test_ai_message_with_text_and_tool_calls_renders_both() {
        let msg = BaseMessage::ai_with_tool_calls(
            MessageContent::text("I'll run a command"),
            vec![ToolCallRequest::new("toolu_001", "bash", json!({"command": "ls"}))],
        );

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                // 应该有 1 个 Text block 和 1 个 ToolUse block
                let text_count = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::Text { .. }))
                    .count();
                let tool_count = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                    .count();

                assert_eq!(text_count, 1, "应该有 1 个 Text block");
                assert_eq!(tool_count, 1, "应该有 1 个 ToolUse block");
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    /// 测试：content 中已有 ToolUse block 时，不重复添加 tool_calls
    #[test]
    fn test_no_duplicate_tool_use_from_tool_calls() {
        use rust_create_agent::messages::ContentBlock;

        // content 中包含 ToolUse block，同时 tool_calls 也有相同的
        let blocks = vec![
            ContentBlock::text("I'll run bash"),
            ContentBlock::tool_use("toolu_001", "bash", json!({"command": "ls"})),
        ];
        let msg = BaseMessage::ai_from_blocks(blocks);

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                // 应该只有 1 个 ToolUse block（不重复）
                let tool_count = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                    .count();
                assert_eq!(tool_count, 1, "不应该重复添加 ToolUse block");
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    /// 测试：纯文本 AI 消息正常渲染
    #[test]
    fn test_ai_message_with_only_text_renders_text() {
        let msg = BaseMessage::ai("Hello, how can I help?");

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                assert_eq!(blocks.len(), 1, "应该有 1 个 block");
                assert!(
                    matches!(blocks[0], ContentBlockView::Text { .. }),
                    "应该是 Text block"
                );
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }
}
