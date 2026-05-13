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
use std::time::{Duration, Instant};

use rust_create_agent::messages::{BaseMessage, ToolCallRequest};

use crate::app::events::AgentEvent;
use crate::app::tool_display;
use crate::ui::markdown::parse_markdown_default;
use crate::ui::message_view::{
    aggregate_tool_groups, tool_color, ContentBlockView, MessageViewModel,
};
use crate::ui::theme;

/// 合并冻结的 SubAgentGroup VM 到 reconcile 重建后的新 VMs 中，防止 Done 后 SubAgent 显示退化。
///
/// `frozen_vms` 是 SubAgentEnd 时构建的完整 SubAgentGroup VM（含 recent_messages、final_result 等），
/// 按出现顺序与新 VMs 中的 SubAgentGroup 占位符按位置匹配替换。
fn merge_frozen_subagents(frozen_vms: &[MessageViewModel], new_vms: &mut [MessageViewModel]) {
    if frozen_vms.is_empty() {
        return;
    }

    // 按位置匹配：新 VMs 中第 N 个 SubAgentGroup 对应 frozen_vms 中第 N 个
    let mut frozen_idx = 0;
    for vm in new_vms.iter_mut() {
        if matches!(vm, MessageViewModel::SubAgentGroup { .. }) && frozen_idx < frozen_vms.len() {
            *vm = frozen_vms[frozen_idx].clone();
            frozen_idx += 1;
        }
    }
}

// ─── 管线事件 ────────────────────────────────────────────────────────────────

/// 管线处理事件后的输出动作
#[derive(Debug)]
pub enum PipelineAction {
    /// 无 UI 变化
    None,
    /// 新增消息（外部通知 + 用户消息）
    AddMessage(MessageViewModel),
    /// 尾部重建（prefix_len 标记不变前缀长度，tail_vms 存储重建尾部）
    RebuildAll {
        prefix_len: usize,
        tail_vms: Vec<MessageViewModel>,
    },
}

// ─── 管线内部状态 ────────────────────────────────────────────────────────────

/// 已开始但未结束的工具调用
struct PendingTool {
    #[allow(dead_code)]
    tool_call_id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    input: serde_json::Value,
}

/// ToolEnd 后、StateSnapshot 前的工具结果（用于在 reconcile gap 期间显示）
struct CompletedTool {
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
    output: String,
    is_error: bool,
}

/// 从后台任务结果字符串中解析 task_id 短格式（前 8 位）。
///
/// 输入格式: `"Background task bg-{uuid} started..."`
/// 输出: `Some("{前8位}")` 或 `None`（解析失败时优雅降级）
fn parse_bg_hash(result: &str) -> Option<String> {
    result
        .strip_prefix("Background task bg-")
        .and_then(|rest| rest.split(' ').next())
        .map(|uuid| uuid.chars().take(8).collect())
}

/// 活跃 SubAgent 执行状态
struct SubAgentState {
    agent_id: String,
    task_preview: String,
    total_steps: usize,
    /// 流式期间的内部消息（不持久化）
    recent_messages: Vec<MessageViewModel>,
    is_running: bool,
    /// SubAgentEnd 时固化的完整 VM（含 recent_messages、final_result 等）
    finalized_vm: Option<MessageViewModel>,
    /// 是否为后台 agent
    is_background: bool,
    /// 后台任务的短 ID（task_id 前 8 位）
    bg_hash: Option<String>,
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
    /// ToolEnd 后、StateSnapshot 前的工具结果（在 reconcile gap 期间显示）
    completed_tools: Vec<CompletedTool>,
    /// SubAgent 栈
    subagent_stack: Vec<SubAgentState>,
    /// 冻结的 SubAgentGroup VMs（SubAgentEnd 时构建，done() 时收集）
    frozen_subagent_vms: Vec<MessageViewModel>,
    // ── 节流状态 ──
    /// 是否有待发射的节流 RebuildAll（有流式 chunk 积累但尚未发射）
    throttle_armed: bool,
    /// 上次节流发射的时间
    throttle_last_fire: Option<Instant>,
    // ── 轮次追踪 ──
    /// 本轮开始时 completed 的长度（用于区分首轮 StateSnapshot 前/后）
    completed_len_at_round_start: usize,
    /// 本轮是否收到过 StateSnapshot
    has_snapshot_this_round: bool,
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
            completed_tools: Vec::new(),
            subagent_stack: Vec::new(),
            frozen_subagent_vms: Vec::new(),
            throttle_armed: false,
            throttle_last_fire: None,
            completed_len_at_round_start: 0,
            has_snapshot_this_round: false,
        }
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// 统一事件处理入口：将 AgentEvent 转换为 PipelineAction 列表。
    /// 所有事件只更新 pipeline 内部状态，返回 None。
    /// RebuildAll 由 agent_ops 通过 `check_throttle()` 或 `build_rebuild_all()` 显式触发。
    pub fn handle_event(&mut self, event: AgentEvent) -> Vec<PipelineAction> {
        match event {
            AgentEvent::AssistantChunk(chunk) => {
                if !chunk.is_empty() {
                    if self.in_subagent() {
                        self.subagent_push_chunk(&chunk);
                    } else {
                        self.push_chunk(&chunk);
                    }
                    self.throttle_armed = true;
                }
                vec![PipelineAction::None]
            }
            AgentEvent::AiReasoning(text) => {
                if self.in_subagent() {
                    // SubAgent 内部推理：更新 subagent 状态，arm throttle
                    if let Some(_sub) = self.subagent_stack.last_mut() {
                        // 推理内容不直接显示，但需要 arm throttle 以刷新 SubAgentGroup
                    }
                    self.throttle_armed = true;
                } else {
                    self.push_reasoning(&text);
                    self.throttle_armed = true;
                }
                vec![PipelineAction::None]
            }
            AgentEvent::ToolStart {
                tool_call_id,
                name,
                display: _,
                args: _,
                input,
            } => {
                // 仅解除 throttle，不在此处触发 RebuildAll。
                // agent_ops 中的 request_rebuild() 会以正确的 prefix_len
                // (= round_start_vm_idx) 触发重建，同时包含流式文本和工具调用。
                // 之前此处使用 prefix_len: 0 会导致 view_messages 被全部替换，
                // 随后 request_rebuild() 用旧的 round_start_vm_idx 做 drain 时 panic。
                self.throttle_armed = false;

                if self.in_subagent() {
                    self.subagent_tool_start(&tool_call_id, &name, input);
                } else {
                    self.tool_start_internal(&tool_call_id, &name, input, false);
                }

                vec![PipelineAction::None]
            }
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
            } => {
                self.throttle_armed = false;
                if self.in_subagent() {
                    // 更新 recent_messages 中对应 ToolBlock 的内容
                    if let Some(sub) = self.subagent_stack.last_mut() {
                        for vm in sub.recent_messages.iter_mut().rev() {
                            if let MessageViewModel::ToolBlock {
                                tool_call_id: tc_id,
                                content,
                                is_error: err,
                                ..
                            } = vm
                            {
                                if tc_id == &tool_call_id {
                                    *content = output.clone();
                                    *err = is_error;
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    self.tool_end_internal(&tool_call_id, &name, &output, is_error);
                }
                vec![PipelineAction::None]
            }
            AgentEvent::SubAgentStart {
                agent_id,
                task_preview,
                is_background,
            } => {
                let input =
                    serde_json::json!({"subagent_type": &agent_id, "prompt": &task_preview});
                let tc_id = format!("subagent_{}", agent_id);
                self.tool_start_internal(&tc_id, "Agent", input, is_background);
                vec![PipelineAction::None]
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                let tc_id = self
                    .subagent_stack
                    .last()
                    .map(|s| format!("subagent_{}", s.agent_id))
                    .unwrap_or_else(|| "subagent_end".to_string());
                self.tool_end_internal(&tc_id, "Agent", &result, is_error);
                vec![PipelineAction::None]
            }
            AgentEvent::Done => {
                if self.in_subagent() {
                    // Child agent done during tool execution — ignore to avoid
                    // finalizing parent state or corrupting the subagent_stack.
                    vec![PipelineAction::None]
                } else {
                    self.done();
                    vec![PipelineAction::None]
                }
            }
            AgentEvent::Interrupted => {
                if self.in_subagent() {
                    // Child agent interrupted — ignore; parent tool call will
                    // handle the result (including interruption) when it returns.
                    vec![PipelineAction::None]
                } else {
                    self.interrupt();
                    vec![PipelineAction::None]
                }
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
            | AgentEvent::LlmRetrying { .. }
            | AgentEvent::ContextWarning { .. }
            | AgentEvent::OAuthAuthorizationNeeded { .. }
            | AgentEvent::OAuthAuthorizationCompleted { .. }
            | AgentEvent::OAuthAuthorizationFailed { .. }
            | AgentEvent::BackgroundTaskCompleted { .. }
            | AgentEvent::McpActionCompleted { .. }
            | AgentEvent::PluginActionCompleted { .. }
            | AgentEvent::LspDiagnostics { .. } => {
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

    /// 工具调用开始（内部版本，只更新状态，不返回 PipelineAction）
    fn tool_start_internal(
        &mut self,
        tool_call_id: &str,
        name: &str,
        input: serde_json::Value,
        is_background: bool,
    ) {
        self.finalize_current_ai();
        self.current_ai_tool_calls
            .push(ToolCallRequest::new(tool_call_id, name, input.clone()));

        if name == "Agent" {
            let agent_id = input["subagent_type"]
                .as_str()
                .unwrap_or("Agent")
                .to_string();
            let task_preview: String = input["prompt"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(40)
                .collect();
            self.subagent_stack.push(SubAgentState {
                agent_id: agent_id.clone(),
                task_preview: task_preview.clone(),
                total_steps: 0,
                recent_messages: Vec::new(),
                is_running: true,
                finalized_vm: None,
                is_background,
                bg_hash: None,
            });
        }

        self.pending_tools.insert(
            tool_call_id.to_string(),
            PendingTool {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                input,
            },
        );
    }

    /// 工具调用结束（内部版本，只更新状态，不返回 PipelineAction）
    fn tool_end_internal(&mut self, tool_call_id: &str, name: &str, output: &str, is_error: bool) {
        let pending = self.pending_tools.remove(tool_call_id);
        let input = pending
            .as_ref()
            .map(|p| p.input.clone())
            .unwrap_or(serde_json::Value::Null);

        if name == "Agent" {
            if let Some(sub) = self.subagent_stack.last_mut() {
                if sub.is_background {
                    // 后台 agent 路径：不冻结，保持 is_running=true，解析 bg_hash
                    sub.bg_hash = parse_bg_hash(output);
                    // 保持 is_running=true，等待 BackgroundTaskCompleted 到达
                    // 显式确保 is_running=true（防止其他逻辑意外修改）
                    sub.is_running = true;
                } else {
                    // 前台 agent 路径：冻结 SubAgentGroup
                    sub.is_running = false;
                    let vm = MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id.clone(),
                        task_preview: sub.task_preview.clone(),
                        total_steps: sub.total_steps,
                        recent_messages: std::mem::take(&mut sub.recent_messages),
                        is_running: false,
                        collapsed: false,
                        final_result: Some(output.to_string()),
                        is_error,
                        is_background: false,
                        bg_hash: None,
                    };
                    sub.finalized_vm = Some(vm.clone());
                    // 立即冻结：RebuildAll 可能在下一个 StateSnapshot 前触发
                    self.frozen_subagent_vms.push(vm);
                }
            }
        } else {
            // 非 SubAgent 工具：保存到 completed_tools，在 StateSnapshot 到达前显示
            self.completed_tools.push(CompletedTool {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                input,
                output: output.to_string(),
                is_error,
            });
        }
    }

    /// SubAgent 内部工具调用（路由进 SubAgentGroup）
    pub fn subagent_tool_start(
        &mut self,
        tool_call_id: &str,
        name: &str,
        input: serde_json::Value,
    ) {
        if let Some(sub) = self.subagent_stack.last_mut() {
            let display = tool_display::format_tool_name(name);
            let args = tool_display::format_tool_args(name, &input, Some(&self.cwd));
            let vm = MessageViewModel::tool_block_with_id(
                tool_call_id.to_string(),
                name.to_string(),
                display,
                args,
                false,
            );
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
                    sub.total_steps += 1;
                    if sub.recent_messages.len() >= 4 {
                        sub.recent_messages.remove(0);
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
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.throttle_armed = false;
        self.throttle_last_fire = None;
        self.drain_subagent_stack();
    }

    /// 中断：finalize 当前状态并清理残留
    pub fn interrupt(&mut self) {
        self.finalize_current_ai();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.throttle_armed = false;
        self.throttle_last_fire = None;
        self.drain_subagent_stack();
    }

    /// 清理 subagent_stack：只推入**未**在 tool_end_internal 中 freeze 的残留条目。
    ///
    /// `tool_end_internal` 在 SubAgentEnd 时已将 finalized_vm 推入 frozen_subagent_vms，
    /// 这里只处理异常情况（SubAgent 未正常结束，如被 Interrupted/Error 打断时仍在运行）。
    /// 已 finalized 的条目不重复推入，避免 frozen 列表膨胀导致 merge_frozen_subagents 错位。
    fn drain_subagent_stack(&mut self) {
        for sub in self.subagent_stack.drain(..) {
            if sub.finalized_vm.is_none() && !sub.is_running {
                // 未 finalized 但已停止：异常残留，构建一个基本 VM 保留显示
                self.frozen_subagent_vms
                    .push(MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id,
                        task_preview: sub.task_preview,
                        total_steps: sub.total_steps,
                        recent_messages: sub.recent_messages,
                        is_running: false,
                        collapsed: false,
                        final_result: None,
                        is_error: false,
                        is_background: sub.is_background,
                        bg_hash: sub.bg_hash,
                    });
            }
            // 已 finalized（finalized_vm.is_some()）的不推入——tool_end_internal 已处理
            // 仍在运行（is_running=true）的不推入——background agent 仍在执行
        }
    }

    /// 清空所有状态
    pub fn clear(&mut self) {
        self.completed.clear();
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.subagent_stack.clear();
        self.frozen_subagent_vms.clear();
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
        // 后台 agent 不会阻塞父 agent 的 Done 事件
        self.subagent_stack
            .last()
            .is_some_and(|s| s.is_running && !s.is_background)
    }

    /// 诊断用：返回 frozen_subagent_vms 的数量
    pub fn frozen_subagent_vms_count(&self) -> usize {
        self.frozen_subagent_vms.len()
    }

    /// 构建当前流式 AssistantBubble（从 pipeline 流式缓冲区构建完整内容）
    pub fn build_streaming_bubble(&self) -> MessageViewModel {
        let mut blocks: Vec<ContentBlockView> = Vec::new();
        if !self.current_ai_reasoning.is_empty() {
            blocks.push(ContentBlockView::Reasoning {
                char_count: self.current_ai_reasoning.chars().count(),
            });
        }
        if !self.current_ai_text.trim().is_empty() {
            let rendered = parse_markdown_default(&self.current_ai_text);
            let rendered_prefix_lines = rendered.lines.len();
            blocks.push(ContentBlockView::Text {
                raw: self.current_ai_text.clone(),
                rendered,
                dirty: false,
                rendered_prefix_len: self.current_ai_text.len(),
                rendered_prefix_lines,
            });
        }
        // 追加当前 AI 消息中已完成的 tool_use blocks（不含 pending tools）
        for tc in &self.current_ai_tool_calls {
            if !self.pending_tools.contains_key(&tc.id) {
                blocks.push(ContentBlockView::ToolUse {
                    name: tc.name.clone(),
                });
            }
        }
        MessageViewModel::AssistantBubble {
            blocks,
            is_streaming: true,
            collapsed: false,
        }
    }

    // ── 轮次管理 ──────────────────────────────────────────────────────────────

    /// 标记新一轮对话开始。由 submit_message() 调用。
    pub fn begin_round(&mut self) {
        self.completed_len_at_round_start = self.completed.len();
        self.has_snapshot_this_round = false;
        self.throttle_armed = false;
        self.throttle_last_fire = None;
    }

    // ── 节流机制 ──────────────────────────────────────────────────────────────

    /// 检查节流计时器，若 100ms 已过则发射 RebuildAll。
    /// 由 poll_agent() 每帧调用。
    pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        if !self.throttle_armed {
            return None;
        }
        let now = Instant::now();
        let should_fire = match self.throttle_last_fire {
            None => true,
            Some(last) => now.duration_since(last) >= Duration::from_millis(100),
        };
        if should_fire {
            self.throttle_last_fire = Some(now);
            self.throttle_armed = false;
            return Some(self.build_rebuild_all(prefix_len));
        }
        None
    }

    // ── RebuildAll 构造 ───────────────────────────────────────────────────────

    /// 构造 RebuildAll action：从 pipeline 规范状态重建尾部 VMs。
    pub fn build_rebuild_all(&self, prefix_len: usize) -> PipelineAction {
        let tail_vms = self.build_tail_vms();
        PipelineAction::RebuildAll {
            prefix_len,
            tail_vms,
        }
    }

    /// 从 pipeline 规范状态构建尾部 VMs。
    ///
    /// 两种情况：
    /// - has_snapshot_this_round == true：从 completed[last_human..] reconcile + streaming + pending tools
    /// - has_snapshot_this_round == false（Case 1）：跳过 reconcile，只输出 streaming + pending tools
    fn build_tail_vms(&self) -> Vec<MessageViewModel> {
        let mut tail_vms = Vec::new();

        if self.has_snapshot_this_round {
            let start = self.completed_len_at_round_start.min(self.completed.len());
            let round_completed = &self.completed[start..];
            let last_human_offset = round_completed
                .iter()
                .rposition(|msg| matches!(msg, BaseMessage::Human { .. }))
                .map(|idx| idx + start)
                .unwrap_or(start);
            tail_vms =
                Self::messages_to_view_models(&self.completed[last_human_offset..], &self.cwd);
            let reconcile_subagent_count =
                tail_vms.iter().filter(|vm| vm.is_subagent_group()).count();
            tracing::debug!(
                has_snapshot = true,
                completed_len = self.completed.len(),
                start_offset = start,
                last_human_offset,
                reconcile_total = tail_vms.len(),
                reconcile_subagent_count,
                frozen_count = self.frozen_subagent_vms.len(),
                "[bg-diag] build_tail_vms reconcile"
            );
        }

        // 追加流式 AssistantBubble（当前 AI 正在输出的文本）
        // 必须在工具 blocks 之前：AI 先说文本，再调用工具
        if self.has_streaming_content() {
            tail_vms.push(self.build_streaming_bubble());
        }

        // 追加 pending tool blocks（ToolStart 后、下一个 StateSnapshot 前的工具）
        // 跳过 Agent 工具（由 subagent_stack 表示为 SubAgentGroup）
        for tc in &self.current_ai_tool_calls {
            if let Some(pending) = self.pending_tools.get(&tc.id) {
                if pending.name != "Agent" {
                    tail_vms.push(self.build_tool_start_vm(&tc.id, &pending.name, &pending.input));
                }
            }
        }

        // 追加已完成但尚未进入 completed 的工具结果（ToolEnd 后、StateSnapshot 前）
        for ct in &self.completed_tools {
            let display = tool_display::format_tool_name(&ct.name);
            let args = tool_display::format_tool_args(&ct.name, &ct.input, Some(&self.cwd));
            tail_vms.push(MessageViewModel::ToolBlock {
                tool_name: ct.name.clone(),
                tool_call_id: ct.tool_call_id.clone(),
                display_name: display,
                args_display: args,
                content: ct.output.clone(),
                is_error: ct.is_error,
                collapsed: true,
                color: if ct.is_error {
                    theme::ERROR
                } else {
                    tool_color(&ct.name)
                },
            });
        }

        // SubAgentGroup VMs
        if self.has_snapshot_this_round {
            // reconcile 已从 completed 生成 SubAgentGroup 占位符，用冻结版本替换
            // （冻结版本含 recent_messages、final_result 等 richer 信息）
            merge_frozen_subagents(&self.frozen_subagent_vms, &mut tail_vms);
        } else {
            // 无 snapshot 时 reconcile 不执行，直接从 subagent_stack 构建 SubAgentGroup
            for sub in &self.subagent_stack {
                let vm = if let Some(ref finalized) = sub.finalized_vm {
                    finalized.clone()
                } else {
                    MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id.clone(),
                        task_preview: sub.task_preview.clone(),
                        total_steps: sub.total_steps,
                        recent_messages: sub.recent_messages.clone(),
                        is_running: sub.is_running,
                        collapsed: false,
                        final_result: None,
                        is_error: false,
                        is_background: sub.is_background,
                        bg_hash: sub.bg_hash.clone(),
                    }
                };
                tail_vms.push(vm);
            }
        }

        // 聚合相邻只读工具
        aggregate_tool_groups(&mut tail_vms);

        tail_vms
    }

    /// 获取已完成的 BaseMessages（用于持久化）
    pub fn completed_messages(&self) -> &[BaseMessage] {
        &self.completed
    }

    /// 追加增量 BaseMessages（StateSnapshot 是增量消息），并清除流式状态防止重复
    pub fn set_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.completed.extend(msgs);
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
        self.has_snapshot_this_round = true;
        self.pending_tools.clear();
        self.completed_tools.clear();
    }

    /// 从外部加载全量 BaseMessages（用于历史恢复后覆盖），并清除所有状态
    pub fn restore_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.completed = msgs;
        self.completed_len_at_round_start = self.completed.len();
        self.has_snapshot_this_round = false;
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

            let vm =
                MessageViewModel::from_base_message_with_cwd(msg, &prev_ai_tool_calls, Some(cwd));

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

        // 不清空 current_ai_text/current_ai_reasoning：在 StateSnapshot 到达前，
        // build_tail_vms() 仍需要这些内容来显示 AI 已输出的文本。
        // set_completed() 到达时会清空它们。
        // 保留 tool_calls 信息给后续 reconcile 使用
        self.current_ai_finalized = true;
    }

    /// 构建 ToolStart 的 ToolBlock VM（与 from_base_message_with_cwd 的 Tool 路径一致）
    fn build_tool_start_vm(
        &self,
        tool_call_id: &str,
        name: &str,
        input: &serde_json::Value,
    ) -> MessageViewModel {
        let display_name = tool_display::format_tool_name(name);
        let args_display = tool_display::format_tool_args(name, input, Some(&self.cwd));
        MessageViewModel::ToolBlock {
            tool_name: name.to_string(),
            tool_call_id: tool_call_id.to_string(),
            display_name,
            args_display,
            content: String::new(),
            is_error: false,
            collapsed: true,
            color: tool_color(name),
        }
    }
}

#[cfg(test)]
#[path = "message_pipeline_test.rs"]
mod tests;
