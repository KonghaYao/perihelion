//! 统一消息渲染管线 (Unified Message Rendering Pipeline)
//!
//! 核心设计：所有 `MessageViewModel` 的产生都经过单一转换函数
//! `messages_to_view_models(base_messages, cwd)`。
//!
//! # 两条路径
//!
//! ```text
//!   流式事件 ──→ 增量更新 BaseMessage[] ──→ reconcile ──→ MessageViewModel[]
//!   历史恢复 ──→ BaseMessage[]            ──→ 直接转换  ──→ MessageViewModel[]
//!                                    ↑
//!                      同一个 messages_to_view_models()
//! ```
//!
//! # 流式 UX 优化
//!
//! `AssistantChunk` 使用 `AppendChunk` 直接操作渲染层（避免每字符重做 markdown），
//! 但在 "finalize 边界"（ToolStart / ToolEnd / Done）会 reconcile 最后的
//! AssistantBubble，确保最终状态与 restore 路径完全一致。

use std::collections::HashMap;

use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent, ToolCallRequest};

use crate::app::tool_display;
use crate::ui::message_view::{
    MessageViewModel, ContentBlockView, ToolCategory, aggregate_tool_groups,
    tool_color,
};
use crate::ui::theme;
use crate::app::events::AgentEvent;

// ─── 管线事件 ────────────────────────────────────────────────────────────────

/// 管线处理事件后的输出动作
#[derive(Debug)]
pub enum PipelineAction {
    /// 无 UI 变化
    None,
    /// 新增消息
    AddMessage(MessageViewModel),
    /// 追加 chunk 到最后一条 AssistantBubble（流式优化）
    AppendChunk(String),
    /// 更新最后一条消息（SubAgentGroup / ToolBlock 内容更新）
    UpdateLast(MessageViewModel),
    /// 流式结束
    StreamingDone,
    /// 移除最后一条消息
    RemoveLast,
    /// 移除末尾 N 条消息
    RemoveLastN(usize),
    /// 全量重建（工具聚合变更等）
    RebuildAll(Vec<MessageViewModel>),
}

// ─── 管线内部状态 ────────────────────────────────────────────────────────────

/// 已开始但未结束的工具调用
struct PendingTool {
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
}

/// 活跃 SubAgent 执行状态
struct SubAgentState {
    agent_id: String,
    task_preview: String,
    total_steps: usize,
    /// 流式期间的内部消息（不持久化）
    recent_messages: Vec<MessageViewModel>,
    is_running: bool,
    /// 流式期间子 agent 产生的 BaseMessage
    inner_messages: Vec<BaseMessage>,
}

// ─── MessagePipeline ─────────────────────────────────────────────────────────

/// 统一消息渲染管线。
///
/// 维护规范 `BaseMessage[]` 状态，通过单一转换函数 `messages_to_view_models()`
/// 产生 `MessageViewModel`。流式和恢复共享同一个转换路径。
pub struct MessagePipeline {
    cwd: String,
    /// 已完成的 BaseMessages（规范状态，可用于持久化）
    completed: Vec<BaseMessage>,
    /// 当前正在流式构建的 AI 文本
    current_ai_text: String,
    /// 当前正在流式构建的 AI 推理内容
    current_ai_reasoning: String,
    /// 当前 AI 消息中的 tool_calls（由 ToolStart 事件积累）
    current_ai_tool_calls: Vec<ToolCallRequest>,
    /// 当前 AI 消息是否已 finalize（ToolStart 到达后 finalize）
    current_ai_finalized: bool,
    /// 已开始但未结束的工具调用
    pending_tools: HashMap<String, PendingTool>,
    /// SubAgent 栈
    subagent_stack: Vec<SubAgentState>,
}

impl MessagePipeline {
    pub fn new(cwd: String) -> Self {
        Self {
            cwd,
            completed: Vec::new(),
            current_ai_text: String::new(),
            current_ai_reasoning: String::new(),
            current_ai_tool_calls: Vec::new(),
            current_ai_finalized: false,
            pending_tools: HashMap::new(),
            subagent_stack: Vec::new(),
        }
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// 统一事件处理入口：将 AgentEvent 转换为 PipelineAction 列表。
    /// agent_ops 通过此方法委托所有消息状态管理逻辑。
    pub fn handle_event(&mut self, event: AgentEvent) -> Vec<PipelineAction> {
        match event {
            AgentEvent::AssistantChunk(chunk) => {
                if chunk.is_empty() {
                    // 空 chunk：不创建新 bubble，仅追加到已有 bubble
                    vec![PipelineAction::None]
                } else if self.in_subagent() {
                    self.subagent_push_chunk(&chunk);
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    self.push_chunk(&chunk);
                    vec![PipelineAction::AppendChunk(chunk)]
                }
            }
            AgentEvent::ToolStart { tool_call_id, name, display: _, args: _, input } => {
                if self.in_subagent() {
                    self.subagent_tool_start(&name, input);
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    vec![self.tool_start(&tool_call_id, &name, input)]
                }
            }
            AgentEvent::ToolEnd { tool_call_id, name, output, is_error } => {
                if self.in_subagent() {
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    vec![self.tool_end(&tool_call_id, &name, &output, is_error)]
                }
            }
            AgentEvent::SubAgentStart { agent_id, task_preview } => {
                let input = serde_json::json!({"agent_id": &agent_id, "task": &task_preview});
                vec![self.tool_start(&format!("subagent_{}", agent_id), "launch_agent", input)]
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                vec![self.tool_end("subagent_end", "launch_agent", &result, is_error)]
            }
            AgentEvent::Done => {
                self.done();
                vec![PipelineAction::StreamingDone]
            }
            AgentEvent::Interrupted => {
                self.interrupt();
                vec![PipelineAction::None]
            }
            AgentEvent::StateSnapshot(msgs) => {
                self.set_completed(msgs);
                vec![PipelineAction::None]
            }
            // 以下事件由 agent_ops 直接处理，Pipeline 返回 None
            AgentEvent::Error(_)
            | AgentEvent::InteractionRequest { .. }
            | AgentEvent::TodoUpdate(_)
            | AgentEvent::CompactDone { .. }
            | AgentEvent::CompactError(_)
            | AgentEvent::TokenUsageUpdate { .. }
            | AgentEvent::LlmRetrying { .. } => {
                vec![PipelineAction::None]
            }
        }
    }

    // ─── 流式事件输入 ─────────────────────────────────────────────────────

    /// 追加流式文本 chunk
    pub fn push_chunk(&mut self, chunk: &str) {
        self.current_ai_text.push_str(chunk);
    }

    /// 追加推理 chunk
    pub fn push_reasoning(&mut self, text: &str) {
        self.current_ai_reasoning.push_str(text);
    }

    /// 工具调用开始
    ///
    /// 返回 `PipelineAction` 告知调用方需要什么 UI 操作。
    pub fn tool_start(
        &mut self,
        tool_call_id: &str,
        name: &str,
        input: serde_json::Value,
    ) -> PipelineAction {
        // 首次 ToolStart → finalize 当前 AI 消息到 completed
        self.finalize_current_ai();

        // 记录 tool_call
        self.current_ai_tool_calls.push(ToolCallRequest::new(
            tool_call_id,
            name,
            input.clone(),
        ));

        // 构建 ToolBlock VM（从 BaseMessage 路径，保持一致）
        if name == "launch_agent" {
            let agent_id = input["agent_id"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let task_preview: String = input["task"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(40)
                .collect();
            // 开始新的 SubAgentGroup
            self.subagent_stack.push(SubAgentState {
                agent_id: agent_id.clone(),
                task_preview: task_preview.clone(),
                total_steps: 0,
                recent_messages: Vec::new(),
                is_running: true,
                inner_messages: Vec::new(),
            });
            self.pending_tools.insert(
                tool_call_id.to_string(),
                PendingTool {
                    tool_call_id: tool_call_id.to_string(),
                    name: name.to_string(),
                    input,
                },
            );
            return PipelineAction::AddMessage(MessageViewModel::subagent_group(
                agent_id, task_preview,
            ));
        }

        // 构建与 from_base_message 一致的 ToolBlock
        let vm = self.build_tool_start_vm(name, &input);
        self.pending_tools.insert(
            tool_call_id.to_string(),
            PendingTool {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                input,
            },
        );
        PipelineAction::AddMessage(vm)
    }

    /// 工具调用结束
    pub fn tool_end(
        &mut self,
        tool_call_id: &str,
        name: &str,
        output: &str,
        is_error: bool,
    ) -> PipelineAction {
        // 创建 BaseMessage::Tool 并加入 completed
        let tool_msg = BaseMessage::Tool {
            id: rust_create_agent::messages::MessageId::new(),
            tool_call_id: tool_call_id.to_string(),
            content: MessageContent::text(output),
            is_error,
        };
        self.completed.push(tool_msg);
        self.pending_tools.remove(tool_call_id);

        // launch_agent ToolEnd → SubAgentEnd
        if name == "launch_agent" {
            if let Some(sub) = self.subagent_stack.last_mut() {
                sub.is_running = false;
                // Store result as BaseMessage for persistence
                self.completed.push(BaseMessage::Tool {
                    id: rust_create_agent::messages::MessageId::new(),
                    tool_call_id: tool_call_id.to_string(),
                    content: MessageContent::text(output),
                    is_error,
                });
                let vm = MessageViewModel::SubAgentGroup {
                    agent_id: sub.agent_id.clone(),
                    task_preview: sub.task_preview.clone(),
                    total_steps: sub.total_steps,
                    recent_messages: std::mem::take(&mut sub.recent_messages),
                    is_running: false,
                    collapsed: false,
                    final_result: Some(output.to_string()),
                };
                return PipelineAction::UpdateLast(vm);
            }
            return PipelineAction::None;
        }

        // ask_user ToolEnd → 更新 ToolBlock 显示用户回答
        if name == "ask_user" {
            let args = tool_display::format_tool_args(
                "ask_user",
                &serde_json::Value::Null,
                None,
            );
            let vm = MessageViewModel::ToolBlock {
                tool_name: "ask_user".to_string(),
                display_name: tool_display::format_tool_name("ask_user"),
                args_display: args,
                content: output.to_string(),
                is_error,
                collapsed: true,
                color: tool_color("ask_user"),
            };
            return PipelineAction::UpdateLast(vm);
        }

        // 普通工具错误 → 更新 ToolBlock 为错误状态
        if is_error {
            let args = tool_display::format_tool_args(
                name,
                &serde_json::Value::Null,
                Some(&self.cwd),
            );
            let vm = MessageViewModel::ToolBlock {
                tool_name: name.to_string(),
                display_name: tool_display::format_tool_name(name),
                args_display: args,
                content: output.to_string(),
                is_error: true,
                collapsed: true,
                color: theme::ERROR,
            };
            return PipelineAction::UpdateLast(vm);
        }

        // 只读工具成功 → 可能需要聚合
        if ToolCategory::from_tool_name(name).is_some() {
            // 返回 None，由调用方决定是否聚合
            // 调用方会检查最后一个 VM 是否是 ToolCallGroup
            return PipelineAction::None;
        }

        // 非只读工具成功 → 更新 ToolBlock 内容
        PipelineAction::None
    }

    /// SubAgent 内部工具调用（路由进 SubAgentGroup）
    pub fn subagent_tool_start(&mut self, name: &str, input: serde_json::Value) {
        if let Some(sub) = self.subagent_stack.last_mut() {
            let display = tool_display::format_tool_name(name);
            let args = tool_display::format_tool_args(name, &input, Some(&self.cwd));
            let vm = MessageViewModel::tool_block(name.to_string(), display, args, false);
            sub.total_steps += 1;
            if sub.recent_messages.len() >= 4 {
                sub.recent_messages.remove(0);
            }
            sub.recent_messages.push(vm);
        }
    }

    /// SubAgent 内部 chunk
    pub fn subagent_push_chunk(&mut self, chunk: &str) {
        if let Some(sub) = self.subagent_stack.last_mut() {
            match sub.recent_messages.last_mut() {
                Some(m) if m.is_assistant() => m.append_chunk(chunk),
                _ => {
                    if sub.recent_messages.len() >= 4 {
                        sub.recent_messages.remove(0);
                    } else {
                        sub.total_steps += 1;
                    }
                    let mut bubble = MessageViewModel::assistant();
                    bubble.append_chunk(chunk);
                    sub.recent_messages.push(bubble);
                }
            }
        }
    }

    /// 标记当前 AI 轮次结束
    pub fn done(&mut self) {
        self.finalize_current_ai();
        // 重置流式状态以准备下一轮
        self.current_ai_finalized = false;
        // 清理已完成的 SubAgent
        self.subagent_stack.retain(|s| s.is_running);
    }

    /// 中断：finalize 当前状态
    pub fn interrupt(&mut self) {
        self.finalize_current_ai();
        self.current_ai_finalized = false;
    }

    /// 清空所有状态
    pub fn clear(&mut self) {
        self.completed.clear();
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.subagent_stack.clear();
    }

    /// 当前 AI 消息是否有可见内容
    pub fn has_streaming_content(&self) -> bool {
        !self.current_ai_text.trim().is_empty() || !self.current_ai_reasoning.is_empty()
    }

    /// 当前 AI 消息是否有待处理的 tool_calls
    pub fn has_pending_tool_calls(&self) -> bool {
        !self.current_ai_tool_calls.is_empty()
    }

    /// 是否在 SubAgent 执行中
    pub fn in_subagent(&self) -> bool {
        self.subagent_stack.last().map_or(false, |s| s.is_running)
    }

    /// 构建当前流式 AssistantBubble（用于 AppendChunk 优化）
    pub fn build_streaming_bubble(&self) -> MessageViewModel {
        MessageViewModel::AssistantBubble {
            blocks: Vec::new(), // 由 append_chunk 填充
            is_streaming: true,
            collapsed: false,
        }
    }

    /// 构建 SubAgentGroup 更新 VM
    pub fn build_subagent_update(&self) -> Option<MessageViewModel> {
        self.subagent_stack.last().map(|sub| MessageViewModel::SubAgentGroup {
            agent_id: sub.agent_id.clone(),
            task_preview: sub.task_preview.clone(),
            total_steps: sub.total_steps,
            recent_messages: sub.recent_messages.clone(),
            is_running: sub.is_running,
            collapsed: false,
            final_result: None,
        })
    }

    /// 获取已完成的 BaseMessages（用于持久化）
    pub fn completed_messages(&self) -> &[BaseMessage] {
        &self.completed
    }

    /// 追加增量 BaseMessages（StateSnapshot 是增量消息），并清除流式状态防止重复
    pub fn set_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.completed.extend(msgs);
        // 清除流式缓冲：completed 已包含完整消息，finalize_current_ai 不应再产出重复
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
    }

    /// 从外部加载全量 BaseMessages（用于历史恢复后覆盖），并清除所有状态
    pub fn restore_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.completed = msgs;
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
    }

    // ─── 核心转换函数 ─────────────────────────────────────────────────────

    /// 从规范 BaseMessage[] 构建完整的 MessageViewModel[]。
    ///
    /// **这是唯一的转换入口**——流式 reconcile 和历史恢复都调用此函数。
    pub fn messages_to_view_models(msgs: &[BaseMessage], cwd: &str) -> Vec<MessageViewModel> {
        let mut vms: Vec<MessageViewModel> = Vec::with_capacity(msgs.len());
        let mut prev_ai_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

        for msg in msgs {
            // 维护前一条 Ai 消息的 tool_calls，用于 Tool 消息获取工具名和参数
            if let BaseMessage::Ai { tool_calls, .. } = msg {
                prev_ai_tool_calls = tool_calls
                    .iter()
                    .map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                    .collect();
            }

            let vm = MessageViewModel::from_base_message_with_cwd(msg, &prev_ai_tool_calls, Some(cwd));

            // 跳过没有可见文本内容的 AssistantBubble（纯 ToolUse 或空文本 + ToolUse）
            if let MessageViewModel::AssistantBubble { blocks, .. } = &vm {
                let has_visible = blocks.iter().any(|b| match b {
                    ContentBlockView::Text { raw, .. } => !raw.trim().is_empty(),
                    ContentBlockView::Reasoning { char_count } => *char_count > 0,
                    ContentBlockView::ToolUse { .. } => false,
                });
                if !has_visible {
                    continue;
                }
            }

            vms.push(vm);
        }

        // 聚合相邻的只读工具调用为 ToolCallGroup
        aggregate_tool_groups(&mut vms);
        vms
    }

    /// Reconcile：从当前 completed 状态重建完整的 view_models。
    ///
    /// 在 "finalize 边界"（ToolStart / Done）调用，确保流式最终状态
    /// 与 restore 路径 `messages_to_view_models()` 完全一致。
    pub fn reconcile(&self) -> Vec<MessageViewModel> {
        Self::messages_to_view_models(&self.completed, &self.cwd)
    }

    // ─── 内部方法 ─────────────────────────────────────────────────────────

    /// Finalize 当前 AI 消息：将流式状态转为 BaseMessage 加入 completed
    fn finalize_current_ai(&mut self) {
        if self.current_ai_finalized {
            return;
        }
        let has_content = !self.current_ai_text.trim().is_empty()
            || !self.current_ai_reasoning.is_empty()
            || !self.current_ai_tool_calls.is_empty();

        if !has_content {
            return;
        }

        let mut blocks: Vec<ContentBlock> = Vec::new();

        if !self.current_ai_reasoning.is_empty() {
            blocks.push(ContentBlock::reasoning(&self.current_ai_reasoning));
        }

        if !self.current_ai_text.is_empty() {
            blocks.push(ContentBlock::text(&self.current_ai_text));
        }

        // 将 tool_calls 转为 ToolUse blocks（与 LLM 响应格式一致）
        for tc in &self.current_ai_tool_calls {
            blocks.push(ContentBlock::tool_use(&tc.id, &tc.name, tc.arguments.clone()));
        }

        let content = MessageContent::Blocks(blocks);

        let ai_msg = BaseMessage::Ai {
            id: rust_create_agent::messages::MessageId::new(),
            content,
            tool_calls: std::mem::take(&mut self.current_ai_tool_calls),
        };

        self.completed.push(ai_msg);
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_finalized = true;
    }

    /// 构建 ToolStart 的 ToolBlock VM（与 from_base_message_with_cwd 的 Tool 路径一致）
    fn build_tool_start_vm(&self, name: &str, input: &serde_json::Value) -> MessageViewModel {
        let display_name = tool_display::format_tool_name(name);
        let args_display = tool_display::format_tool_args(name, input, Some(&self.cwd));
        MessageViewModel::ToolBlock {
            tool_name: name.to_string(),
            display_name,
            args_display,
            content: String::new(), // 流式时内容为空，ToolEnd 时更新
            is_error: false,
            collapsed: true,
            color: tool_color(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent, ToolCallRequest};
    use serde_json::json;

    fn normalize_vms(vms: Vec<MessageViewModel>) -> Vec<String> {
        vms.iter().map(|vm| format!("{:?}", vm)).collect()
    }

    /// 测试：流式路径和恢复路径对简单文本回复产生一致的输出
    #[test]
    fn test_streaming_vs_restore_text_only() {
        let cwd = "/Users/test/project";

        // 恢复路径
        let msgs = vec![
            BaseMessage::human("hello"),
            BaseMessage::ai("world"),
        ];
        let restore_vms = MessagePipeline::messages_to_view_models(&msgs, cwd);

        // 流式路径：模拟事件序列
        let mut pipeline = MessagePipeline::new(cwd.to_string());
        pipeline.push_chunk("world");
        pipeline.done();
        let stream_vms = pipeline.reconcile();

        // 比较非系统消息
        assert_eq!(restore_vms.len(), 2);
        assert_eq!(stream_vms.len(), 1); // 流式路径没有用户消息（由 handle_agent_event 添加）
    }

    /// 测试：工具调用的 cwd 一致性（核心修复验证）
    #[test]
    fn test_tool_args_cwd_consistency() {
        let cwd = "/Users/test/project";

        // 模拟恢复路径：Tool 消息从 BaseMessage 转换
        // Ai 消息带文本 + tool_calls，确保不会被过滤
        let msgs = vec![
            BaseMessage::human("read file"),
            BaseMessage::ai_with_tool_calls(
                MessageContent::text("I'll read the file"),
                vec![ToolCallRequest::new("tc1", "read_file", json!({"file_path": "/Users/test/project/src/main.rs"}))],
            ),
            BaseMessage::Tool {
                id: rust_create_agent::messages::MessageId::new(),
                tool_call_id: "tc1".to_string(),
                content: MessageContent::text("file content here"),
                is_error: false,
            },
        ];
        let restore_vms = MessagePipeline::messages_to_view_models(&msgs, cwd);

        // 找到 ToolBlock 或 ToolCallGroup
        let tool_vm = restore_vms.iter().find(|vm| {
            matches!(vm, MessageViewModel::ToolBlock { .. }) || matches!(vm, MessageViewModel::ToolCallGroup { .. })
        });
        assert!(tool_vm.is_some(), "应有 ToolBlock/ToolCallGroup，实际 VMs: {:?}", restore_vms);

        if let Some(MessageViewModel::ToolBlock { args_display, .. }) = tool_vm {
            // 应该显示相对路径而非绝对路径
            assert!(args_display.is_some(), "args_display 应有值");
            let args = args_display.as_ref().unwrap();
            assert!(
                args.contains("src/main.rs"),
                "应显示相对路径，实际: {}",
                args
            );
            assert!(
                !args.contains("/Users/test/project"),
                "不应包含 cwd 前缀，实际: {}",
                args
            );
        }
    }

    /// 测试：恢复路径的 cwd=None 仍能正常工作（向后兼容）
    #[test]
    fn test_restore_without_cwd() {
        let msgs = vec![
            BaseMessage::human("hello"),
            BaseMessage::ai("hi"),
        ];
        // cwd=None → fallback 行为
        let vms = MessagePipeline::messages_to_view_models(&msgs, "");
        assert_eq!(vms.len(), 2);
    }

    /// 测试：流式 pipeline 的 finalize 正确产生 BaseMessage
    #[test]
    fn test_pipeline_finalize_ai_message() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        pipeline.push_reasoning("thinking...");
        pipeline.push_chunk("Hello world");
        pipeline.done();

        let completed = pipeline.completed_messages();
        assert_eq!(completed.len(), 1);

        if let BaseMessage::Ai { content, .. } = &completed[0] {
            let blocks = content.content_blocks();
            assert_eq!(blocks.len(), 2); // Reasoning + Text
        } else {
            panic!("应为 Ai 消息");
        }
    }

    /// 测试：流式 pipeline 的 tool_start/tool_end 正确产生 BaseMessage
    #[test]
    fn test_pipeline_tool_lifecycle() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        pipeline.push_chunk("I'll read a file");
        let _action = pipeline.tool_start("tc1", "read_file", json!({"file_path": "/tmp/test.txt"}));
        let _action = pipeline.tool_end("tc1", "read_file", "content here", false);
        pipeline.done();

        let completed = pipeline.completed_messages();
        // 应有: Ai(text) + Tool(result)
        assert!(completed.len() >= 2, "应有至少 2 条消息，实际: {}", completed.len());
    }

    /// 测试：from_base_message_with_cwd 与 from_base_message 向后兼容
    #[test]
    fn test_from_base_message_backward_compat() {
        let msg = BaseMessage::ai("hello");
        let vm1 = MessageViewModel::from_base_message(&msg, &[]);
        let vm2 = MessageViewModel::from_base_message_with_cwd(&msg, &[], None);
        // 两者应产生相同结果
        assert_eq!(format!("{:?}", vm1), format!("{:?}", vm2));
    }

    // ─── handle_event 测试 ─────────────────────────────────────────────────

    /// 测试：handle_event AssistantChunk 产生 AppendChunk
    #[test]
    fn test_handle_event_assistant_chunk() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        let actions = pipeline.handle_event(AgentEvent::AssistantChunk("hello".into()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::AppendChunk(ref c) if c == "hello"));
    }

    /// 测试：handle_event 空 chunk 不产生 AppendChunk
    #[test]
    fn test_handle_event_empty_chunk() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        let actions = pipeline.handle_event(AgentEvent::AssistantChunk(String::new()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::None));
    }

    /// 测试：handle_event ToolStart + ToolEnd + Done 产生完整生命周期
    #[test]
    fn test_handle_event_tool_lifecycle() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        // ToolStart
        let actions = pipeline.handle_event(AgentEvent::ToolStart {
            tool_call_id: "tc1".into(),
            name: "read_file".into(),
            display: "ReadFile".into(),
            args: "src/main.rs".into(),
            input: serde_json::json!({"file_path": "/tmp/src/main.rs"}),
        });
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::AddMessage(_)));
        // ToolEnd
        let actions = pipeline.handle_event(AgentEvent::ToolEnd {
            tool_call_id: "tc1".into(),
            name: "read_file".into(),
            output: "file content".into(),
            is_error: false,
        });
        assert_eq!(actions.len(), 1);
        // ToolEnd 对只读工具返回 None
        assert!(matches!(actions[0], PipelineAction::None));
        // Done → StreamingDone（不再 RebuildAll，流式路径已通过增量操作维护 view_messages）
        let actions = pipeline.handle_event(AgentEvent::Done);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::StreamingDone));
    }

    /// 测试：handle_event StateSnapshot 更新 completed
    #[test]
    fn test_handle_event_state_snapshot() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("world")];
        let actions = pipeline.handle_event(AgentEvent::StateSnapshot(msgs.clone()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::None));
        assert_eq!(pipeline.completed_messages().len(), 2);
    }
}
