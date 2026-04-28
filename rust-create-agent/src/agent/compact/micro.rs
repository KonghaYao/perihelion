use crate::agent::compact::config::CompactConfig;
use crate::agent::compact::invariant::{adjust_index_to_preserve_invariants, group_messages_by_round};
use crate::messages::{BaseMessage, ContentBlock, MessageContent};

fn find_tool_name_for_tool_result(messages: &[BaseMessage], tool_call_id: &str) -> Option<String> {
    for msg in messages.iter().rev() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                if tc.id == tool_call_id {
                    return Some(tc.name.clone());
                }
            }
        }
    }
    None
}

fn compact_tool_result_content(content: &mut MessageContent, config: &CompactConfig) -> bool {
    let original_text = content.text_content();
    if original_text.is_empty() {
        return false;
    }

    let blocks = content.content_blocks();

    let has_image_or_doc = blocks.iter().any(|b| {
        matches!(b, ContentBlock::Image { .. } | ContentBlock::Document { .. })
    });

    if !has_image_or_doc {
        return false;
    }

    let mut modified = false;
    let new_blocks: Vec<ContentBlock> = blocks
        .into_iter()
        .map(|b| match &b {
            ContentBlock::Image { source } => {
                let size_chars = match source {
                    crate::messages::ImageSource::Base64 { data, .. } => data.len(),
                    crate::messages::ImageSource::Url { url } => url.len(),
                };
                let token_est = size_chars / 4;
                modified = true;
                if token_est > config.re_inject_max_tokens_per_file as usize {
                    ContentBlock::text(format!("[compacted: image ~{} tokens]", token_est))
                } else {
                    ContentBlock::text("[image]")
                }
            }
            ContentBlock::Document { source, .. } => {
                let size_chars = match source {
                    crate::messages::DocumentSource::Base64 { data, .. } => data.len(),
                    crate::messages::DocumentSource::Text { text } => text.len(),
                    crate::messages::DocumentSource::Url { url } => url.len(),
                };
                let token_est = size_chars / 4;
                modified = true;
                if token_est > config.re_inject_max_tokens_per_file as usize {
                    ContentBlock::text(format!("[compacted: document ~{} tokens]", token_est))
                } else {
                    ContentBlock::text("[document]")
                }
            }
            _ => b,
        })
        .collect();

    if modified {
        *content = MessageContent::blocks(new_blocks);
    }
    modified
}

pub fn micro_compact_enhanced(config: &CompactConfig, messages: &mut [BaseMessage]) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let rounds = group_messages_by_round(messages);
    let total_rounds = rounds.len();
    let stale_threshold = config.micro_compact_stale_steps;
    let stale_round_limit = total_rounds.saturating_sub(stale_threshold);

    let mut round_index = vec![0usize; messages.len()];
    for (ri, round) in rounds.iter().enumerate() {
        for mi in round.start..round.end {
            if mi < messages.len() {
                round_index[mi] = ri;
            }
        }
    }

    let mut compactable_indices: Vec<usize> = Vec::new();
    for (i, msg) in messages.iter().enumerate() {
        if let BaseMessage::Tool {
            tool_call_id,
            is_error,
            ..
        } = msg
        {
            if *is_error {
                continue;
            }
            if round_index[i] >= stale_round_limit {
                continue;
            }
            let tool_name = find_tool_name_for_tool_result(messages, tool_call_id);
            match tool_name {
                Some(ref name) if config.micro_compactable_tools.contains(name) => {}
                _ => continue,
            }
            compactable_indices.push(i);
        }
    }

    if compactable_indices.is_empty() {
        let mut image_cleared = 0;
        for i in 0..messages.len() {
            if round_index[i] >= stale_round_limit {
                continue;
            }
            if let BaseMessage::Tool {
                content,
                is_error,
                ..
            } = &mut messages[i]
            {
                if *is_error {
                    continue;
                }
                if compact_tool_result_content(content, config) {
                    image_cleared += 1;
                }
            }
        }
        return image_cleared;
    }

    let compact_start = *compactable_indices.first().unwrap();
    let compact_end = *compactable_indices.last().unwrap() + 1;
    let (adj_start, adj_end) =
        adjust_index_to_preserve_invariants(messages, compact_start, compact_end);

    let mut cleared = 0;
    for i in adj_start..adj_end {
        if round_index[i] >= stale_round_limit {
            continue;
        }
        let (tc_id, is_err) = match &messages[i] {
            BaseMessage::Tool { tool_call_id, is_error, .. } => (tool_call_id.clone(), *is_error),
            _ => continue,
        };
        if is_err {
            continue;
        }
        let tool_name = find_tool_name_for_tool_result(messages, &tc_id);
        let in_whitelist = match tool_name {
            Some(ref name) => config.micro_compactable_tools.contains(name),
            None => false,
        };
        if !in_whitelist {
            continue;
        }

        if let BaseMessage::Tool { content, .. } = &mut messages[i] {
            let original_text = content.text_content();
            // 跳过已压缩的消息，避免重复处理（覆盖 chars/image/document 三种格式）
            if original_text.starts_with("[compacted:") {
                continue;
            }
            let original_len = original_text.len();
            let was_modified = compact_tool_result_content(content, config);

            if !was_modified {
                *content = MessageContent::text(format!("[compacted: {} chars]", original_len));
            }
            cleared += 1;
        }
    }

    cleared
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> CompactConfig {
        CompactConfig::default()
    }

    fn ai_with_tool(id: &str, name: &str) -> BaseMessage {
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("using tool"),
            vec![ToolCallRequest::new(id, name, json!({}))],
        )
    }

    fn tool_result(tc_id: &str, text: &str) -> BaseMessage {
        BaseMessage::tool_result(tc_id, text)
    }

    fn tool_result_with_image(tc_id: &str, text: &str) -> BaseMessage {
        BaseMessage::tool_result(
            tc_id,
            MessageContent::blocks(vec![
                ContentBlock::text(text),
                ContentBlock::image_base64("image/png", "iVBOR...base64data"),
            ]),
        )
    }

    fn tool_result_with_large_image(tc_id: &str) -> BaseMessage {
        let large_b64 = "A".repeat(100_000);
        BaseMessage::tool_result(
            tc_id,
            MessageContent::blocks(vec![
                ContentBlock::text("output"),
                ContentBlock::image_base64("image/png", &large_b64),
            ]),
        )
    }

    use crate::messages::ToolCallRequest;

    // Whitelist tests

    #[test]
    fn test_whitelist_only_compactable_tools() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", &long_text),
            BaseMessage::human("q"),
            ai_with_tool("tc2", "ask_user_question"),
            tool_result("tc2", &long_text),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 1;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 1);
        assert_eq!(msgs[1].content(), "[compacted: 600 chars]");
        assert_ne!(msgs[4].content(), "[compacted: 600 chars]");
    }

    #[test]
    fn test_whitelist_custom_list() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", &long_text),
            ai_with_tool("tc2", "read_file"),
            tool_result("tc2", &long_text),
        ];
        let mut config = CompactConfig {
            micro_compactable_tools: vec!["read_file".to_string()],
            micro_compact_stale_steps: 0,
            ..Default::default()
        };
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 1);
        assert_ne!(msgs[1].content(), "[compacted: 600 chars]");
        assert_eq!(msgs[3].content(), "[compacted: 600 chars]");
    }

    #[test]
    fn test_whitelist_unknown_tool_preserved() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "custom_tool"),
            tool_result("tc1", &long_text),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 0);
    }

    // Stale steps tests

    #[test]
    fn test_stale_steps_keep_recent() {
        let long_text = "x".repeat(600);
        let mut msgs: Vec<BaseMessage> = Vec::new();
        for i in 0..7 {
            let tc_id = format!("tc{}", i);
            msgs.push(ai_with_tool(&tc_id, "bash"));
            msgs.push(tool_result(&tc_id, &long_text));
        }
        let mut config = test_config();
        config.micro_compact_stale_steps = 5;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 2);
    }

    #[test]
    fn test_stale_steps_zero_compact_all() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", &long_text),
            ai_with_tool("tc2", "bash"),
            tool_result("tc2", &long_text),
            ai_with_tool("tc3", "bash"),
            tool_result("tc3", &long_text),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 3);
    }

    #[test]
    fn test_stale_steps_large_keep_all() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", &long_text),
            ai_with_tool("tc2", "bash"),
            tool_result("tc2", &long_text),
            ai_with_tool("tc3", "bash"),
            tool_result("tc3", &long_text),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 100;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 0);
    }

    // Image/document tests

    #[test]
    fn test_image_replaced_with_placeholder() {
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result_with_image("tc1", "text"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 1);
        let content = msgs[1].content();
        assert!(content.contains("[image]"), "got: {}", content);
    }

    #[test]
    fn test_large_image_compacted_with_token_info() {
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result_with_large_image("tc1"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 1);
        let content = msgs[1].content();
        assert!(content.contains("compacted: image"), "got: {}", content);
        assert!(content.contains("25000 tokens"), "got: {}", content);
    }

    #[test]
    fn test_image_in_recent_step_preserved() {
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result_with_image("tc1", "text"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 5;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 0);
    }

    // Invariant preservation tests

    #[test]
    fn test_invariant_preserves_tool_pair() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            BaseMessage::human("q"),
            BaseMessage::ai_with_tool_calls(
                MessageContent::text("using tools"),
                vec![
                    ToolCallRequest::new("tc1", "bash", json!({})),
                    ToolCallRequest::new("tc2", "bash", json!({})),
                ],
            ),
            tool_result("tc1", &long_text),
            tool_result("tc2", &long_text),
            ai_plain("done"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 1;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 2);
    }

    #[test]
    fn test_invariant_preserves_ai_parent() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", &long_text),
            BaseMessage::human("q"),
            ai_plain("done"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 1;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 1);
        assert!(msgs[0].has_tool_calls());
    }

    // Edge cases

    #[test]
    fn test_empty_messages() {
        let mut msgs: Vec<BaseMessage> = vec![];
        let cleared = micro_compact_enhanced(&test_config(), &mut msgs);
        assert_eq!(cleared, 0);
    }

    #[test]
    fn test_no_tool_messages() {
        let mut msgs = vec![
            BaseMessage::human("q"),
            ai_plain("a"),
            BaseMessage::human("q2"),
            ai_plain("a2"),
        ];
        let cleared = micro_compact_enhanced(&test_config(), &mut msgs);
        assert_eq!(cleared, 0);
    }

    #[test]
    fn test_error_tool_result_preserved() {
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            BaseMessage::tool_error("tc1", "error message"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 0);
    }

    #[test]
    fn test_already_compacted_skipped() {
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", "[compacted: 600 chars]"),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 0, "已压缩的消息应被跳过");
        assert_eq!(msgs[1].content(), "[compacted: 600 chars]", "已压缩消息内容不变");
    }

    #[test]
    fn test_orphan_tool_result_preserved() {
        let mut msgs = vec![tool_result("orphan_id", &"x".repeat(600))];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 0);
    }

    #[test]
    fn test_mixed_compactable_and_protected() {
        let long_text = "x".repeat(600);
        let mut msgs = vec![
            ai_with_tool("tc1", "bash"),
            tool_result("tc1", &long_text),
            ai_with_tool("tc2", "ask_user_question"),
            tool_result("tc2", &long_text),
            ai_with_tool("tc3", "bash"),
            tool_result("tc3", &long_text),
        ];
        let mut config = test_config();
        config.micro_compact_stale_steps = 0;
        let cleared = micro_compact_enhanced(&config, &mut msgs);
        assert_eq!(cleared, 2);
        assert_eq!(msgs[1].content(), "[compacted: 600 chars]");
        assert_ne!(msgs[3].content(), "[compacted: 600 chars]");
        assert_eq!(msgs[5].content(), "[compacted: 600 chars]");
    }

    fn ai_plain(text: &str) -> BaseMessage {
        BaseMessage::ai(text)
    }
}
