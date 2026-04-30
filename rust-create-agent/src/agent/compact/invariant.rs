use crate::messages::BaseMessage;

#[derive(Debug, Clone)]
pub struct MessageRound {
    pub start: usize,
    pub end: usize,
    pub tool_call_ids: Vec<String>,
}

pub fn group_messages_by_round(messages: &[BaseMessage]) -> Vec<MessageRound> {
    let mut rounds = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        let round_start = i;
        if matches!(&messages[i], BaseMessage::Ai { tool_calls, .. } if !tool_calls.is_empty()) {
            let tool_call_ids: Vec<String> = messages[i]
                .tool_calls()
                .iter()
                .map(|tc| tc.id.clone())
                .collect();
            let tc_count = tool_call_ids.len();
            let mut end = i + 1;
            let mut matched = 0;
            while end < messages.len() && matched < tc_count {
                if let BaseMessage::Tool { tool_call_id, .. } = &messages[end] {
                    if tool_call_ids.contains(tool_call_id) {
                        matched += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
                end += 1;
            }
            rounds.push(MessageRound {
                start: round_start,
                end,
                tool_call_ids,
            });
            i = end;
        } else {
            rounds.push(MessageRound {
                start: round_start,
                end: i + 1,
                tool_call_ids: Vec::new(),
            });
            i += 1;
        }
    }
    rounds
}

fn find_tool_pair_boundary(messages: &[BaseMessage], index: usize) -> (usize, usize) {
    let tool_call_id = match &messages[index] {
        BaseMessage::Tool { tool_call_id, .. } => tool_call_id.clone(),
        _ => return (index, index + 1),
    };

    let ai_index = messages[..index]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, msg)| {
            if msg.has_tool_calls() {
                let ids: Vec<&str> = msg.tool_calls().iter().map(|tc| tc.id.as_str()).collect();
                if ids.contains(&tool_call_id.as_str()) {
                    return Some(i);
                }
            }
            None
        })
        .unwrap_or(index);

    let all_tc_ids: Vec<String> = messages[ai_index]
        .tool_calls()
        .iter()
        .map(|tc| tc.id.clone())
        .collect();

    let mut end = ai_index + 1;
    while end < messages.len() {
        if let BaseMessage::Tool {
            tool_call_id: tc_id,
            ..
        } = &messages[end]
        {
            if all_tc_ids.iter().any(|id| id == tc_id) {
                end += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    (ai_index, end)
}

pub fn adjust_index_to_preserve_invariants(
    messages: &[BaseMessage],
    start: usize,
    end: usize,
) -> (usize, usize) {
    if messages.is_empty() || start >= end || start >= messages.len() {
        return (start.min(messages.len()), end.min(messages.len()));
    }

    let mut adjusted_start = start;
    let mut adjusted_end = end.min(messages.len());

    let (pair_start, _pair_end) = find_tool_pair_boundary(messages, adjusted_start);
    if pair_start < adjusted_start {
        adjusted_start = pair_start;
    }

    if adjusted_end < messages.len() {
        let (pair_start, pair_end) = find_tool_pair_boundary(messages, adjusted_end);
        if pair_start < adjusted_end && pair_end > adjusted_end {
            adjusted_end = pair_end;
        }
    }

    // 使用 while 循环替代 for 循环，确保边界扩展后新加入的 Tool 消息也被检查
    let mut i = adjusted_start;
    while i < adjusted_end {
        if matches!(&messages[i], BaseMessage::Tool { .. }) {
            let (ps, pe) = find_tool_pair_boundary(messages, i);
            if ps < adjusted_start {
                adjusted_start = ps;
            }
            if pe > adjusted_end {
                adjusted_end = pe.min(messages.len());
            }
        }
        i += 1;
    }

    (adjusted_start, adjusted_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{MessageContent, ToolCallRequest};
    use serde_json::json;

    fn ai_with_tools(ids: &[&str]) -> BaseMessage {
        let tcs: Vec<ToolCallRequest> = ids
            .iter()
            .map(|&id| ToolCallRequest::new(id, "bash", json!({"command": "echo"})))
            .collect();
        BaseMessage::ai_with_tool_calls(MessageContent::text("using tools"), tcs)
    }

    fn ai_plain(text: &str) -> BaseMessage {
        BaseMessage::ai(text)
    }

    fn tool_msg(tc_id: &str, text: &str) -> BaseMessage {
        BaseMessage::tool_result(tc_id, text)
    }

    // group_messages_by_round tests

    #[test]
    fn test_group_empty() {
        let rounds = group_messages_by_round(&[]);
        assert!(rounds.is_empty());
    }

    #[test]
    fn test_group_plain_ai_only() {
        let msgs = vec![ai_plain("a"), ai_plain("b"), ai_plain("c")];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 3);
        assert_eq!(rounds[0].start, 0);
        assert_eq!(rounds[0].end, 1);
        assert_eq!(rounds[1].start, 1);
        assert_eq!(rounds[1].end, 2);
        assert_eq!(rounds[2].start, 2);
        assert_eq!(rounds[2].end, 3);
    }

    #[test]
    fn test_group_human_ai_alternating() {
        let msgs = vec![
            BaseMessage::human("q1"),
            ai_plain("a1"),
            BaseMessage::human("q2"),
            ai_plain("a2"),
        ];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 4);
    }

    #[test]
    fn test_group_single_tool_pair() {
        let msgs = vec![ai_with_tools(&["tc1"]), tool_msg("tc1", "output")];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].start, 0);
        assert_eq!(rounds[0].end, 2);
        assert_eq!(rounds[0].tool_call_ids, vec!["tc1".to_string()]);
    }

    #[test]
    fn test_group_multiple_tools_one_ai() {
        let msgs = vec![
            ai_with_tools(&["tc1", "tc2"]),
            tool_msg("tc1", "out1"),
            tool_msg("tc2", "out2"),
        ];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].end, 3);
        assert_eq!(
            rounds[0].tool_call_ids,
            vec!["tc1".to_string(), "tc2".to_string()]
        );
    }

    #[test]
    fn test_group_mixed_rounds() {
        let msgs = vec![
            BaseMessage::human("q"),
            ai_with_tools(&["tc1"]),
            tool_msg("tc1", "out"),
            ai_plain("thinking"),
            ai_with_tools(&["tc2"]),
            tool_msg("tc2", "out2"),
        ];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 4);
        assert!(rounds[0].tool_call_ids.is_empty());
        assert_eq!(rounds[1].tool_call_ids, vec!["tc1"]);
        assert!(rounds[2].tool_call_ids.is_empty());
        assert_eq!(rounds[3].tool_call_ids, vec!["tc2"]);
    }

    #[test]
    fn test_group_orphan_tool_message() {
        let msgs = vec![BaseMessage::tool_result("orphan", "orphan output")];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 1);
        assert!(rounds[0].tool_call_ids.is_empty());
    }

    #[test]
    fn test_group_interleaved_human_in_tool_pair() {
        let msgs = vec![
            ai_with_tools(&["tc1", "tc2"]),
            tool_msg("tc1", "out1"),
            BaseMessage::human("interrupt"),
            tool_msg("tc2", "out2"),
        ];
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 3);
        assert_eq!(rounds[0].end, 2);
        assert!(rounds[1].tool_call_ids.is_empty());
    }

    // find_tool_pair_boundary tests

    #[test]
    fn test_find_boundary_tool_message() {
        let msgs = vec![
            ai_with_tools(&["tc1", "tc2"]),
            tool_msg("tc1", "out1"),
            tool_msg("tc2", "out2"),
        ];
        let (start, end) = find_tool_pair_boundary(&msgs, 1);
        assert_eq!(start, 0);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_find_boundary_ai_message() {
        let msgs = vec![ai_with_tools(&["tc1"]), tool_msg("tc1", "out")];
        let (start, end) = find_tool_pair_boundary(&msgs, 0);
        assert_eq!(start, 0);
        assert_eq!(end, 1);
    }

    #[test]
    fn test_find_boundary_human_message() {
        let msgs = vec![BaseMessage::human("q"), ai_plain("a"), tool_msg("x", "out")];
        let (start, end) = find_tool_pair_boundary(&msgs, 0);
        assert_eq!(start, 0);
        assert_eq!(end, 1);
    }

    // adjust_index_to_preserve_invariants tests

    #[test]
    fn test_adjust_no_tool_calls() {
        let msgs = vec![
            BaseMessage::human("q"),
            ai_plain("a"),
            BaseMessage::human("q2"),
            ai_plain("a2"),
        ];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 1, 3);
        assert_eq!(s, 1);
        assert_eq!(e, 3);
    }

    #[test]
    fn test_adjust_start_splits_pair() {
        let msgs = vec![
            ai_with_tools(&["tc1"]),
            tool_msg("tc1", "out"),
            BaseMessage::human("q"),
            ai_plain("a"),
        ];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 1, 4);
        assert_eq!(s, 0);
        assert_eq!(e, 4);
    }

    #[test]
    fn test_adjust_end_splits_pair() {
        let msgs = vec![
            BaseMessage::human("q"),
            ai_with_tools(&["tc1"]),
            tool_msg("tc1", "out"),
            BaseMessage::human("q2"),
        ];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 0, 2);
        assert_eq!(s, 0);
        assert_eq!(e, 3);
    }

    #[test]
    fn test_adjust_both_boundaries_split() {
        let msgs = vec![
            ai_with_tools(&["tc1"]),
            tool_msg("tc1", "out"),
            ai_with_tools(&["tc2"]),
            tool_msg("tc2", "out"),
        ];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 1, 3);
        assert_eq!(s, 0);
        assert_eq!(e, 4);
    }

    #[test]
    fn test_adjust_multiple_tools_partial() {
        let msgs = vec![
            ai_with_tools(&["tc1", "tc2"]),
            tool_msg("tc1", "out1"),
            tool_msg("tc2", "out2"),
            BaseMessage::human("q"),
        ];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 0, 2);
        assert_eq!(s, 0);
        assert_eq!(e, 3);
    }

    #[test]
    fn test_adjust_already_aligned() {
        let msgs = vec![
            BaseMessage::human("q"),
            ai_with_tools(&["tc1"]),
            tool_msg("tc1", "out"),
            BaseMessage::human("q2"),
        ];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 0, 3);
        assert_eq!(s, 0);
        assert_eq!(e, 3);
    }

    #[test]
    fn test_adjust_empty_messages() {
        let (s, e) = adjust_index_to_preserve_invariants(&[], 0, 0);
        assert_eq!(s, 0);
        assert_eq!(e, 0);
    }

    #[test]
    fn test_adjust_full_range() {
        let msgs = vec![BaseMessage::human("q"), ai_plain("a")];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 0, 2);
        assert_eq!(s, 0);
        assert_eq!(e, 2);
    }

    #[test]
    fn test_adjust_start_at_end() {
        let msgs = vec![BaseMessage::human("q")];
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 1, 1);
        assert_eq!(s, 1);
        assert_eq!(e, 1);
    }

    /// 验证 while 循环正确处理边界扩展后的新 Tool 消息
    #[test]
    fn test_adjust_transitive_expansion() {
        let msgs = vec![
            BaseMessage::human("q"),
            // 第一组工具调用
            ai_with_tools(&["tc1"]),
            tool_msg("tc1", "out1"),
            // 第二组工具调用
            ai_with_tools(&["tc2"]),
            tool_msg("tc2", "out2"),
            // 第三组工具调用
            ai_with_tools(&["tc3"]),
            tool_msg("tc3", "out3"),
            BaseMessage::human("q2"),
        ];
        // 初始范围只包含 tc2 的 Tool 结果(索引 4)
        // 应扩展到包含 tc2 完整组，不应影响 tc1 或 tc3
        let (s, e) = adjust_index_to_preserve_invariants(&msgs, 4, 5);
        assert_eq!(s, 3, "应包含 tc2 的 Ai 消息");
        assert_eq!(e, 5, "应包含 tc2 的 Tool 结果");
    }
}
