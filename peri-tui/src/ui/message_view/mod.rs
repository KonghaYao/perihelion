use std::hash::{Hash, Hasher};

use crate::ui::theme;
use peri_agent::messages::{BaseMessage, ContentBlock};
use ratatui::style::Color;
use ratatui::text::Text;

use super::markdown::parse_markdown_default;

mod aggregate;
mod tools;
mod utils;

pub use aggregate::{aggregate_batch_groups, aggregate_tail_tool_groups, aggregate_tool_groups};
pub(crate) use tools::parse_subagent_tool_count;
pub use tools::{tool_color, AgentSummary, ToolCategory, ToolEntry};
pub(crate) use utils::{instance_hash, parse_bg_hash};

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
        tool_call_id: String,
        display_name: String,
        args_display: Option<String>,
        content: String,
        is_error: bool,
        collapsed: bool,
        color: Color,
    },
    /// 系统消息
    SystemNote { content: String },
    /// 缓存率过低警告（黄色纯文本，无前缀符号）
    CacheWarning { content: String },
    /// 只读工具调用聚合组（read/search/glob 折叠显示）
    ToolCallGroup {
        category: ToolCategory,
        tools: Vec<ToolEntry>,
        collapsed: bool,
    },
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
        /// SubAgent 执行是否以错误结束
        is_error: bool,
        /// 是否为后台 agent
        is_background: bool,
        /// Agent 实例的短显示标识符（6 位十六进制）
        bg_hash: Option<String>,
        /// 批次汇总信息：空 = 单 agent，非空 = 批次汇总模式
        batch_agents: Vec<AgentSummary>,
    },
}

/// 语义级相等比较，用于判断 Done 时是否需要 RebuildAll。
///
/// 忽略 UI-only 字段（rendered、is_streaming、collapsed、color 等），
/// 只比较影响显示内容的字段。
impl PartialEq for MessageViewModel {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                MessageViewModel::UserBubble { content: a, .. },
                MessageViewModel::UserBubble { content: b, .. },
            ) => a == b,
            (
                MessageViewModel::AssistantBubble { blocks: a, .. },
                MessageViewModel::AssistantBubble { blocks: b, .. },
            ) => a == b,
            (
                MessageViewModel::ToolBlock {
                    tool_name: a_name,
                    tool_call_id: a_tc,
                    args_display: a_args,
                    content: a_content,
                    is_error: a_err,
                    ..
                },
                MessageViewModel::ToolBlock {
                    tool_name: b_name,
                    tool_call_id: b_tc,
                    args_display: b_args,
                    content: b_content,
                    is_error: b_err,
                    ..
                },
            ) => {
                a_name == b_name
                    && a_tc == b_tc
                    && a_args == b_args
                    && a_content == b_content
                    && a_err == b_err
            }
            (
                MessageViewModel::SystemNote { content: a },
                MessageViewModel::SystemNote { content: b },
            ) => a == b,
            (
                MessageViewModel::CacheWarning { content: a },
                MessageViewModel::CacheWarning { content: b },
            ) => a == b,
            (
                MessageViewModel::ToolCallGroup {
                    category: a,
                    tools: a_tools,
                    ..
                },
                MessageViewModel::ToolCallGroup {
                    category: b,
                    tools: b_tools,
                    ..
                },
            ) => a == b && a_tools == b_tools,
            (
                MessageViewModel::SubAgentGroup {
                    agent_id: a_id,
                    task_preview: a_preview,
                    total_steps: a_steps,
                    recent_messages: a_msgs,
                    final_result: a_result,
                    is_error: a_err,
                    is_background: a_bg,
                    bg_hash: a_hash,
                    batch_agents: a_batch,
                    ..
                },
                MessageViewModel::SubAgentGroup {
                    agent_id: b_id,
                    task_preview: b_preview,
                    total_steps: b_steps,
                    recent_messages: b_msgs,
                    final_result: b_result,
                    is_error: b_err,
                    is_background: b_bg,
                    bg_hash: b_hash,
                    batch_agents: b_batch,
                    ..
                },
            ) => {
                a_id == b_id
                    && a_preview == b_preview
                    && a_steps == b_steps
                    && a_msgs == b_msgs
                    && a_result == b_result
                    && a_err == b_err
                    && a_bg == b_bg
                    && a_hash == b_hash
                    && a_batch == b_batch
            }
            _ => false,
        }
    }
}

/// Hash 包含所有影响渲染输出的字段（内容 + UI 状态如 collapsed/is_streaming/is_running）。
/// `rendered` 和 `color` 不参与 hash（rendered 依赖宽度缓存，color 可从 tool_name 推导）。
impl Hash for MessageViewModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MessageViewModel::UserBubble { content, .. } => {
                0u8.hash(state);
                content.hash(state);
            }
            MessageViewModel::AssistantBubble {
                blocks,
                is_streaming,
                collapsed,
                ..
            } => {
                1u8.hash(state);
                blocks.hash(state);
                is_streaming.hash(state);
                collapsed.hash(state);
            }
            MessageViewModel::ToolBlock {
                tool_name,
                tool_call_id,
                display_name,
                args_display,
                content,
                is_error,
                collapsed,
                ..
            } => {
                2u8.hash(state);
                tool_name.hash(state);
                tool_call_id.hash(state);
                display_name.hash(state);
                args_display.hash(state);
                content.hash(state);
                is_error.hash(state);
                collapsed.hash(state);
            }
            MessageViewModel::SystemNote { content } => {
                3u8.hash(state);
                content.hash(state);
            }
            MessageViewModel::CacheWarning { content } => {
                4u8.hash(state);
                content.hash(state);
            }
            MessageViewModel::ToolCallGroup {
                category,
                tools,
                collapsed,
            } => {
                5u8.hash(state);
                category.hash(state);
                tools.hash(state);
                collapsed.hash(state);
            }
            MessageViewModel::SubAgentGroup {
                agent_id,
                task_preview,
                total_steps,
                recent_messages,
                is_running,
                collapsed,
                final_result,
                is_error,
                is_background,
                bg_hash,
                batch_agents,
            } => {
                6u8.hash(state);
                agent_id.hash(state);
                task_preview.hash(state);
                total_steps.hash(state);
                recent_messages.hash(state);
                is_running.hash(state);
                collapsed.hash(state);
                final_result.hash(state);
                is_error.hash(state);
                is_background.hash(state);
                bg_hash.hash(state);
                batch_agents.hash(state);
            }
        }
    }
}

/// ContentBlock 的视图化表示
#[derive(Debug, Clone)]
pub enum ContentBlockView {
    /// 文本内容（含 markdown 解析缓存）
    Text {
        raw: String,
        rendered: Text<'static>,
        dirty: bool,
        /// 已渲染到 `raw` 的字节偏移（增量解析用）
        rendered_prefix_len: usize,
        /// `rendered` 中对应前缀的行数（避免重解析计数）
        rendered_prefix_lines: usize,
    },
    /// 推理/思考过程（仅显示字数摘要，尾部预览可选）
    Reasoning {
        char_count: usize,
        /// 原始推理全文（仅用于提取尾部预览，不参与哈希/比较）
        text: String,
        /// 尾部行预览：符合条件时由后处理设置。
        /// 值为最后 3 行原始文本（不含 �� 前缀）。
        /// None = 不显示尾部预览
        tail_lines: Option<String>,
    },
    /// 工具使用请求（AI 发起的调用请求）
    ToolUse { name: String },
}

/// 只比较有意义的字段（忽略 `rendered`，因为 markdown 解析可能因宽度不同而异）
impl PartialEq for ContentBlockView {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ContentBlockView::Text {
                    raw: a_raw,
                    dirty: a_dirty,
                    ..
                },
                ContentBlockView::Text {
                    raw: b_raw,
                    dirty: b_dirty,
                    ..
                },
            ) => a_raw == b_raw && a_dirty == b_dirty,
            (
                ContentBlockView::Reasoning { char_count: a, .. },
                ContentBlockView::Reasoning { char_count: b, .. },
            ) => a == b,
            (ContentBlockView::ToolUse { name: a }, ContentBlockView::ToolUse { name: b }) => {
                a == b
            }
            _ => false,
        }
    }
}

/// Hash 基于 PartialEq 的语义字段（忽略 `rendered` 缓存）。
/// 用于渲染线程的 hash diff：判断消息是否需要重新渲染。
impl Hash for ContentBlockView {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ContentBlockView::Text { raw, dirty, .. } => {
                0u8.hash(state);
                raw.hash(state);
                dirty.hash(state);
            }
            ContentBlockView::Reasoning {
                char_count,
                tail_lines,
                ..
            } => {
                1u8.hash(state);
                char_count.hash(state);
                tail_lines.hash(state);
            }
            ContentBlockView::ToolUse { name } => {
                2u8.hash(state);
                name.hash(state);
            }
        }
    }
}

impl MessageViewModel {
    /// 从 BaseMessage 转换为视图模型（向后兼容，cwd 为 None）
    ///
    /// `prev_ai_tool_calls` 用于为 Tool 消息提供工具名和参数（BaseMessage::Tool 只存储 tool_use_id）
    pub fn from_base_message(
        msg: &BaseMessage,
        prev_ai_tool_calls: &[(String, String, serde_json::Value)],
    ) -> Self {
        Self::from_base_message_with_cwd(msg, prev_ai_tool_calls, None)
    }

    /// 从 BaseMessage 转换为视图模型（带 cwd 上下文，统一管线入口）
    ///
    /// `cwd` 用于工具参数路径缩短，确保流式和恢复路径产生一致的显示。
    /// 这是统一管线的核心转换函数——`MessagePipeline::messages_to_view_models()` 调用此方法。
    pub fn from_base_message_with_cwd(
        msg: &BaseMessage,
        prev_ai_tool_calls: &[(String, String, serde_json::Value)],
        cwd: Option<&str>,
    ) -> Self {
        match msg {
            BaseMessage::Human { content, .. } => {
                let raw = content.text_content();
                let rendered = parse_markdown_default(&raw);
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
                        ContentBlock::Text { text } => {
                            let rendered = parse_markdown_default(&text);
                            let rendered_prefix_lines = rendered.lines.len();
                            ContentBlockView::Text {
                                raw: text.clone(),
                                rendered,
                                dirty: false,
                                rendered_prefix_len: text.len(),
                                rendered_prefix_lines,
                            }
                        }
                        ContentBlock::Reasoning { text, .. } => ContentBlockView::Reasoning {
                            char_count: text.chars().count(),
                            text: text.clone(),
                            tail_lines: None,
                        },
                        ContentBlock::ToolUse { name, .. } => ContentBlockView::ToolUse { name },
                        ContentBlock::Image { .. } => ContentBlockView::Text {
                            raw: "[Image]".to_string(),
                            rendered: Text::raw("[Image]"),
                            dirty: false,
                            rendered_prefix_len: 7,
                            rendered_prefix_lines: 1,
                        },
                        ContentBlock::Document { title, .. } => {
                            let label = title.as_deref().unwrap_or("Document");
                            let raw = format!("[Document: {}]", label);
                            let len = raw.len();
                            ContentBlockView::Text {
                                raw,
                                rendered: Text::raw(format!("[Document: {}]", label)),
                                dirty: false,
                                rendered_prefix_len: len,
                                rendered_prefix_lines: 1,
                            }
                        }
                        ContentBlock::Unknown(v) => {
                            let type_name =
                                v.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
                            let raw = format!("[{}]", type_name);
                            let len = raw.len();
                            ContentBlockView::Text {
                                raw,
                                rendered: Text::raw(format!("[{}]", type_name)),
                                dirty: false,
                                rendered_prefix_len: len,
                                rendered_prefix_lines: 1,
                            }
                        }
                        // ToolResult 在 Ai 消息中不常见，静默跳过
                        _ => ContentBlockView::Text {
                            raw: String::new(),
                            rendered: Text::raw(""),
                            dirty: false,
                            rendered_prefix_len: 0,
                            rendered_prefix_lines: 0,
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
                // Agent 工具恢复为 SubAgentGroup（完成状态，展开显示 final_result）
                if tool_name == "Agent" {
                    let agent_id = input
                        .get("subagent_type")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or("fork")
                        .to_string();
                    let task_preview = input["prompt"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(40)
                        .collect::<String>();
                    // 检测是否为后台 agent（从 result 字符串检测 "Background task" 前缀）
                    let is_background = raw_content.starts_with("Background task");
                    // 解析 bg_hash（后台任务从 result 解析，前台从 tool_call_id 生成）
                    let bg_hash = if is_background {
                        parse_bg_hash(&raw_content)
                    } else {
                        Some(instance_hash(tool_call_id))
                    };
                    return MessageViewModel::SubAgentGroup {
                        agent_id,
                        task_preview,
                        total_steps: parse_subagent_tool_count(&raw_content),
                        recent_messages: Vec::new(), // 子 agent 内部消息不持久化
                        is_running: false,
                        collapsed: false, // 展开显示 final_result
                        final_result: Some(raw_content),
                        is_error: *is_error,
                        is_background,
                        bg_hash,
                        batch_agents: Vec::new(),
                    };
                }
                // 使用统一格式化函数生成 display_name 和 args_display
                // cwd 参数确保流式和恢复路径产生一致的路径显示
                let display_name = crate::app::tool_display::format_tool_name(&tool_name);
                let args_display =
                    crate::app::tool_display::format_tool_args(&tool_name, &input, cwd);
                let color = if *is_error {
                    theme::ERROR
                } else {
                    tool_color(&tool_name)
                };
                MessageViewModel::ToolBlock {
                    tool_name,
                    tool_call_id: tool_call_id.clone(),
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
        if let MessageViewModel::AssistantBubble {
            blocks, collapsed, ..
        } = self
        {
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
                rendered_prefix_len: 0,
                rendered_prefix_lines: 0,
            });
        }
    }

    /// 切换折叠状态（对 ToolBlock、AssistantBubble、SubAgentGroup、ToolCallGroup 生效）
    #[allow(dead_code)]
    pub fn toggle_collapse(&mut self) {
        match self {
            MessageViewModel::ToolBlock { collapsed, .. }
            | MessageViewModel::AssistantBubble { collapsed, .. }
            | MessageViewModel::SubAgentGroup { collapsed, .. }
            | MessageViewModel::ToolCallGroup { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            _ => {}
        }
    }

    /// 判断是否为 AssistantBubble
    pub fn is_assistant(&self) -> bool {
        matches!(self, MessageViewModel::AssistantBubble { .. })
    }

    /// 判断是否为"仅含推理内容"的 AssistantBubble（渲染时不可见）
    /// 用于在工具分组合并时跳过中间的空 thinking bubble
    pub fn is_reasoning_only(&self) -> bool {
        match self {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                blocks.is_empty()
                    || blocks.iter().all(|b| match b {
                        ContentBlockView::Reasoning { .. } => true,
                        ContentBlockView::Text { raw, .. } => raw.trim().is_empty(),
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    /// 创建用户消息
    pub fn user(content: String) -> Self {
        let rendered = parse_markdown_default(&content);
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
    pub fn tool_block(
        tool_name: String,
        display: String,
        args: Option<String>,
        is_error: bool,
    ) -> Self {
        Self::tool_block_with_id(String::new(), tool_name, display, args, is_error)
    }

    /// 创建带 tool_call_id 的工具消息（SubAgent 内部并行工具调用精确匹配）
    pub fn tool_block_with_id(
        tool_call_id: String,
        tool_name: String,
        display: String,
        args: Option<String>,
        is_error: bool,
    ) -> Self {
        let color = if is_error {
            theme::ERROR
        } else {
            tool_color(&tool_name)
        };
        MessageViewModel::ToolBlock {
            tool_call_id,
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

    /// 创建缓存率警告消息（黄色纯文本，无前缀符号）
    pub fn cache_warning(content: String) -> Self {
        MessageViewModel::CacheWarning { content }
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
            is_error: false,
            is_background: false,
            bg_hash: None,
            batch_agents: Vec::new(),
        }
    }

    /// 判断是否为 SubAgentGroup
    pub fn is_subagent_group(&self) -> bool {
        matches!(self, MessageViewModel::SubAgentGroup { .. })
    }
}

/// 从 SubAgent 返回结果中解析工具调用次数。
///
/// `format_subagent_result()` 输出格式：`[Sub-agent executed N tool calls: ...]`
/// 或中文版 `[子 agent 执行了 N 个工具调用: ...]`。
#[cfg(test)]
#[path = "message_view_test.rs"]
mod tests;
