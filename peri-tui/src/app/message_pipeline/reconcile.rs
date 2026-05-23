use peri_agent::messages::BaseMessage;

use crate::app::tool_display;
use crate::ui::message_view::{
    aggregate_tool_groups, tool_color, ContentBlockView, MessageViewModel,
};
use crate::ui::theme;

pub use crate::ui::message_view::aggregate_batch_groups;

use super::MessagePipeline;

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

/// 合并冻结的 SubAgentGroup VM 到 reconcile 重建后的新 VMs 中，防止 Done 后 SubAgent 显示退化。
///
/// `frozen_vms` 是 SubAgentEnd 时构建的完整 SubAgentGroup VM（含 recent_messages、final_result 等），
/// 按 `agent_id` 精确匹配替换新 VMs 中的 SubAgentGroup 占位符。
/// 同一 agent_id（重试场景）取 frozen_vms 中最后一次出现的。
pub(crate) fn merge_frozen_subagents(
    frozen_vms: &[MessageViewModel],
    new_vms: &mut [MessageViewModel],
) {
    if frozen_vms.is_empty() {
        return;
    }

    let frozen_by_id: std::collections::HashMap<&str, &MessageViewModel> = frozen_vms
        .iter()
        .filter_map(|vm| {
            if let MessageViewModel::SubAgentGroup { agent_id, .. } = vm {
                Some((agent_id.as_str(), vm))
            } else {
                None
            }
        })
        .collect();

    for vm in new_vms.iter_mut() {
        if let MessageViewModel::SubAgentGroup { agent_id, .. } = vm {
            if let Some(frozen) = frozen_by_id.get(agent_id.as_str()) {
                *vm = (*frozen).clone();
            }
        }
    }
}

impl MessagePipeline {
    /// 构建 RebuildAll action（用于外部 agent_ops 显式触发重建）。
    /// 由调用者提供 prefix_len（round_start_vm_idx），pipeline 内部不维护 VM 索引。
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
    pub(crate) fn build_tail_vms(&self) -> Vec<MessageViewModel> {
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
        if self.has_streaming_content() {
            tail_vms.push(self.build_streaming_bubble());
        }

        // 追加 pending tool blocks（ToolStart 后、下一个 StateSnapshot 前的工具）
        for tc in &self.current_ai_tool_calls {
            if let Some(pending) = self.pending_tools.get(&tc.id) {
                if pending.name != "Agent" {
                    tail_vms.push(self.build_tool_start_vm(&tc.id, &pending.name, &pending.input));
                }
            }
        }

        // 追加已完成但尚未进入 completed 的工具结果
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
            merge_frozen_subagents(&self.frozen_subagent_vms, &mut tail_vms);
            for sub in &self.subagent_stack {
                if sub.finalized_vm.is_none() {
                    tail_vms.push(MessageViewModel::SubAgentGroup {
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
                        batch_agents: Vec::new(),
                    });
                }
            }
        } else {
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
                        batch_agents: Vec::new(),
                    }
                };
                tail_vms.push(vm);
            }
        }

        aggregate_tool_groups(&mut tail_vms);

        if !self.has_streaming_content() && self.current_ai_tool_calls.is_empty() {
            aggregate_batch_groups(&mut tail_vms);
        }

        add_thinking_tail_snapshot(&mut tail_vms);

        tail_vms
    }
}

/// 提取文本的最后 `n` 行（按换行符切分，单行不截断）。
/// 返回换行分隔的字符串。
pub(crate) fn extract_tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// 扫描 tail_vms 的最后一个 AssistantBubble，
/// 若满足条件（无 Text block + 最后一个 block 是 Reasoning）则设置 tail_lines。
pub(crate) fn add_thinking_tail_snapshot(tail_vms: &mut [MessageViewModel]) {
    for vm in tail_vms.iter_mut().rev() {
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            let has_text = blocks
                .iter()
                .any(|b| matches!(b, ContentBlockView::Text { raw, .. } if !raw.trim().is_empty()));
            if has_text {
                return;
            }
            if let Some(ContentBlockView::Reasoning {
                text, tail_lines, ..
            }) = blocks.last_mut()
            {
                let tail = extract_tail_lines(text, 3);
                if !tail.is_empty() {
                    *tail_lines = Some(tail);
                }
            }
            return;
        }
    }
}
