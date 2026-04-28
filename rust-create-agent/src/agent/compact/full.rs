use crate::agent::compact::config::CompactConfig;
use crate::agent::compact::invariant::{group_messages_by_round, MessageRound};
use crate::error::{AgentError, AgentResult};
use crate::llm::types::LlmRequest;
use crate::llm::BaseModel;
use crate::messages::{BaseMessage, ContentBlock, MessageContent};
use tracing::warn;

/// 结构化摘要 system prompt
const SYSTEM_PROMPT: &str =
    "你是一个对话上下文压缩工具，擅长将长对话压缩为结构化摘要。";

/// 结构化摘要 user prompt 模板
const USER_PROMPT_TEMPLATE: &str = r#"请分析以下对话历史，按以下 9 个方面进行详细分析：

<analysis>
1. **Primary Request and Intent** — 用户的核心请求和意图
2. **Key Technical Concepts** — 涉及的关键技术概念和框架
3. **Files and Code Sections** — 操作过的文件路径和关键代码片段
4. **Errors and Fixes** — 遇到的错误及其修复方法
5. **Problem Solving** — 问题解决的思路和过程
6. **All User Messages** — 所有用户消息的摘要
7. **Pending Tasks** — 尚未完成的任务
8. **Current Work** — 当前正在进行的工作
9. **Optional Next Step** — 建议的下一步行动
</analysis>

<summary>
基于以上分析，生成精炼的结构化摘要。保留所有文件路径、错误信息和关键决策。使用 Markdown 格式。
</summary>"#;

/// Full Compact 执行结果
#[derive(Debug, Clone)]
pub struct FullCompactResult {
    pub summary: String,
    pub messages_used: usize,
}

/// 按字符数截断，超出时添加 "...(已截断)" 后缀
fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let end: String = s.chars().take(max).collect();
        format!("{}...(已截断)", end)
    } else {
        s.to_string()
    }
}

/// 将 content 中的 Image block 替换为 [image] 文本，然后截断到 max_chars 字符
fn replace_images_and_truncate(content: &MessageContent, max_chars: usize) -> String {
    let blocks = content.content_blocks();
    let parts: Vec<String> = blocks
        .iter()
        .map(|b| match b {
            ContentBlock::Image { .. } => "[image]".to_string(),
            _ => match b {
                ContentBlock::Text { text } => text.clone(),
                ContentBlock::ToolUse { name, input, .. } => {
                    format!("调用 {}({})", name, input)
                }
                ContentBlock::Reasoning { text, .. } => text.clone(),
                _ => format!("{:?}", b),
            },
        })
        .collect();
    let full = parts.join("\n");
    truncate_str(&full, max_chars)
}

/// 预处理消息：跳过 System、替换 Image block 为 [image]、截断每条消息
fn preprocess_messages(messages: &[BaseMessage], truncate_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for msg in messages {
        match msg {
            BaseMessage::System { .. } => {}
            BaseMessage::Human { .. } => {
                let content = replace_images_and_truncate(&msg.message_content(), truncate_chars);
                lines.push(format!("[用户] {}", content));
            }
            BaseMessage::Ai { tool_calls, .. } => {
                let text = replace_images_and_truncate(&msg.message_content(), truncate_chars);
                let tool_names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                let line = if tool_names.is_empty() {
                    format!("[助手] {}", text)
                } else {
                    format!("[助手] {}（调用了工具: {}）", text, tool_names.join(", "))
                };
                lines.push(line);
            }
            BaseMessage::Tool { tool_call_id, .. } => {
                let content = replace_images_and_truncate(&msg.message_content(), truncate_chars);
                lines.push(format!("[工具结果:{}] {}", tool_call_id, content));
            }
        }
    }
    lines
}

/// 移除 <analysis>...</analysis> 块
fn remove_analysis_blocks(text: &str) -> String {
    let mut result = text.to_string();
    loop {
        let start_tag = "<analysis>";
        let end_tag = "</analysis>";
        if let Some(start) = result.find(start_tag) {
            if let Some(end) = result[start..].find(end_tag) {
                let remove_end = start + end + end_tag.len();
                result = format!("{}{}", &result[..start], &result[remove_end..]);
            } else {
                result = result[..start].to_string();
                break;
            }
        } else {
            break;
        }
    }
    result
}

/// 提取 <summary>...</summary> 标签内的内容
fn extract_summary_content(text: &str) -> Option<String> {
    let start_tag = "<summary>";
    let end_tag = "</summary>";
    let start = text.find(start_tag)?;
    let content_start = start + start_tag.len();
    if let Some(end) = text[content_start..].find(end_tag) {
        Some(text[content_start..content_start + end].trim().to_string())
    } else {
        Some(text[content_start..].trim().to_string())
    }
}

/// 后处理 LLM 输出
fn postprocess_summary(raw: &str) -> String {
    let mut text = raw.to_string();

    text = remove_analysis_blocks(&text);

    if let Some(summary_content) = extract_summary_content(&text) {
        text = summary_content;
    }

    let prefix = "此会话从之前的对话延续。以下是之前对话的摘要。";

    text = text.trim().to_string();
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }

    format!("{}\n\n{}", prefix, text)
}

/// 判断错误是否为 PTL（Prompt Too Long）错误
fn is_ptl_error(error: &crate::error::AgentError) -> bool {
    let msg = error.to_string().to_lowercase();
    msg.contains("prompt_too_long")
        || msg.contains("context_length_exceeded")
        || msg.contains("max_context_window")
        || msg.contains("token limit")
        || msg.contains("too many tokens")
}

/// PTL 降级：从最旧的 round 开始删除指定数量的消息组
///
/// 始终保留开头的所有 System 消息（包含旧摘要等关键上下文），
/// 只从第一个非 System 消息开始截断。
fn truncate_for_ptl(
    messages: &[BaseMessage],
    rounds: &[MessageRound],
    drop_count: usize,
) -> Vec<BaseMessage> {
    if rounds.len() <= 1 {
        return messages.to_vec();
    }

    // 找到第一条非 System 消息的位置
    let first_non_system = messages
        .iter()
        .position(|m| !matches!(m, BaseMessage::System { .. }))
        .unwrap_or(messages.len());

    // 在 rounds 中找到第一个包含非 System 消息的 round 的起始索引
    let non_system_round_start = rounds
        .iter()
        .position(|r| r.start >= first_non_system)
        .unwrap_or(0);

    let droppable_rounds = rounds.len().saturating_sub(non_system_round_start);
    if droppable_rounds <= 1 {
        return messages.to_vec();
    }

    let actual_drop = drop_count.min(droppable_rounds - 1);
    let drop_end = rounds[non_system_round_start + actual_drop - 1].end;

    // 拼接：保留所有 System 消息 + 截断后的非 System 消息
    let mut result = messages[..first_non_system].to_vec();
    result.extend_from_slice(&messages[drop_end..]);
    result
}

/// 执行 Full Compact：预处理 -> LLM 摘要 -> 后处理，支持 PTL 降级重试
pub async fn full_compact(
    messages: &[BaseMessage],
    model: &dyn BaseModel,
    config: &CompactConfig,
    instructions: &str,
) -> AgentResult<FullCompactResult> {
    let non_system_count = messages
        .iter()
        .filter(|m| !matches!(m, BaseMessage::System { .. }))
        .count();

    if non_system_count == 0 {
        return Ok(FullCompactResult {
            summary: postprocess_summary("## 摘要\n（无有效对话历史）"),
            messages_used: messages.len(),
        });
    }

    let mut current_messages: Vec<BaseMessage> = messages.to_vec();
    let max_retries = config.ptl_max_retries as usize;

    for attempt in 0..=max_retries {
        let truncated = preprocess_messages(&current_messages, 2000);
        let conversation_text = truncated.join("\n");

        let mut user_content = format!(
            "以下是需要压缩的对话历史：\n<conversation>\n{}\n</conversation>\n\n{}",
            conversation_text, USER_PROMPT_TEMPLATE
        );

        if !instructions.trim().is_empty() {
            user_content.push_str(&format!("\n\n压缩时请特别注意：{}", instructions.trim()));
        }

        let request = LlmRequest::new(vec![BaseMessage::human(user_content)])
            .with_system(SYSTEM_PROMPT.to_string())
            .with_max_tokens(config.summary_max_tokens);

        match model.invoke(request).await {
            Ok(response) => {
                let raw_summary = response.message.content();
                let summary = postprocess_summary(&raw_summary);
                return Ok(FullCompactResult {
                    summary,
                    messages_used: current_messages.len(),
                });
            }
            Err(e) if is_ptl_error(&e) && attempt < max_retries => {
                warn!(
                    attempt = attempt + 1,
                    max_retries,
                    "Full Compact PTL 降级：prompt 过长，删除最旧消息组后重试"
                );

                let rounds = group_messages_by_round(&current_messages);
                let truncated_messages = truncate_for_ptl(&current_messages, &rounds, 1);
                // 截断无变化 → 无法继续降级，立即返回错误
                if truncated_messages.len() == current_messages.len() {
                    return Err(AgentError::Other(anyhow::anyhow!(
                        "Full Compact PTL 降级失败：消息已无法进一步缩减 ({})",
                        e
                    )));
                }
                current_messages = truncated_messages;
            }
            Err(e) => {
                return Err(AgentError::Other(anyhow::anyhow!(
                    "Full Compact 失败（PTL 降级重试 {} 次后仍失败）: {}",
                    attempt,
                    e
                )));
            }
        }
    }

    // 所有 attempt 在循环内均有 return（Ok 或 Err），此处不可达
    unreachable!("full_compact loop should always return within the loop body")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AgentError;
    use crate::llm::types::{LlmResponse, StopReason};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockBaseModel {
        response: String,
        fail_with_ptl: usize,
        call_count: AtomicUsize,
    }

    impl MockBaseModel {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                fail_with_ptl: 0,
                call_count: AtomicUsize::new(0),
            }
        }
        fn new_with_ptl_fail(response: &str, ptl_fails: usize) -> Self {
            Self {
                response: response.to_string(),
                fail_with_ptl: ptl_fails,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl BaseModel for MockBaseModel {
        async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_with_ptl {
                return Err(AgentError::LlmError(
                    "prompt_too_long: input tokens exceed context window".to_string(),
                ));
            }
            Ok(LlmResponse {
                message: BaseMessage::ai(self.response.clone()),
                stop_reason: StopReason::EndTurn,
                usage: None,
            })
        }
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    // preprocess_messages tests

    #[test]
    fn test_preprocess_skips_system() {
        let msgs = vec![
            BaseMessage::system("old summary"),
            BaseMessage::human("hello"),
            BaseMessage::ai("hi"),
        ];
        let result = preprocess_messages(&msgs, 2000);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_preprocess_truncates_long_text() {
        let long_text = "x".repeat(3000);
        let msgs = vec![BaseMessage::human(long_text)];
        let result = preprocess_messages(&msgs, 2000);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("...(已截断)"));
    }

    #[test]
    fn test_preprocess_replaces_image() {
        let msgs = vec![BaseMessage::human(MessageContent::blocks(vec![
            ContentBlock::text("see"),
            ContentBlock::image_base64("image/png", "data..."),
        ]))];
        let result = preprocess_messages(&msgs, 2000);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("[image]"));
        assert!(result[0].contains("see"));
    }

    #[test]
    fn test_preprocess_formats_tool_calls() {
        use crate::messages::ToolCallRequest;
        use serde_json::json;
        let msgs = vec![BaseMessage::ai_with_tool_calls(
            MessageContent::text("thinking"),
            vec![
                ToolCallRequest::new("tc1", "bash", json!({})),
                ToolCallRequest::new("tc2", "read_file", json!({})),
            ],
        )];
        let result = preprocess_messages(&msgs, 2000);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("（调用了工具: bash, read_file）"));
    }

    #[test]
    fn test_preprocess_formats_tool_result() {
        let msgs = vec![BaseMessage::tool_result("tc1", "output text")];
        let result = preprocess_messages(&msgs, 2000);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("[工具结果:tc1]"));
        assert!(result[0].contains("output text"));
    }

    #[test]
    fn test_preprocess_empty_messages() {
        let result: Vec<String> = preprocess_messages(&[], 2000);
        assert!(result.is_empty());
    }

    // postprocess_summary tests

    #[test]
    fn test_postprocess_removes_analysis() {
        let input = "<analysis>detailed analysis here</analysis>\n\n## 摘要\ncontent";
        let result = postprocess_summary(input);
        assert!(!result.contains("<analysis>"));
        assert!(!result.contains("</analysis>"));
        assert!(result.contains("此会话从之前的对话延续"));
    }

    #[test]
    fn test_postprocess_extracts_summary_tag() {
        let input = "<analysis>思考</analysis>\n<summary>\n## 核心摘要\n实际内容\n</summary>";
        let result = postprocess_summary(input);
        assert!(result.contains("## 核心摘要"));
        assert!(result.contains("实际内容"));
        assert!(!result.contains("<summary>"));
    }

    #[test]
    fn test_postprocess_no_tags() {
        let input = "## 摘要\n这是直接输出的摘要文本";
        let result = postprocess_summary(input);
        assert!(result.contains("此会话从之前的对话延续"));
        assert!(result.contains("这是直接输出的摘要文本"));
    }

    #[test]
    fn test_postprocess_cleans_blank_lines() {
        let input = "## 摘要\n\n\n\n内容\n\n\n\n结尾";
        let result = postprocess_summary(input);
        assert!(!result.contains("\n\n\n"));
    }

    #[test]
    fn test_postprocess_multiple_analysis_blocks() {
        let input = "<analysis>块1</analysis>中间文本<analysis>块2</analysis>剩余";
        let result = postprocess_summary(input);
        assert!(result.contains("中间文本"));
        assert!(result.contains("剩余"));
        assert!(!result.contains("块1"));
        assert!(!result.contains("块2"));
    }

    // truncate_for_ptl tests

    #[test]
    fn test_ptl_truncate_single_round() {
        let msgs = vec![BaseMessage::human("q"), BaseMessage::ai("a")];
        let rounds = group_messages_by_round(&msgs);
        // Human and Ai are separate rounds, but only 1 round can be dropped at most
        let result = truncate_for_ptl(&msgs, &rounds, 1);
        // With 2 rounds, dropping 1 should leave 1 round
        assert!(result.len() < msgs.len());
    }

    #[test]
    fn test_ptl_truncate_drops_oldest() {
        use crate::messages::ToolCallRequest;
        use serde_json::json;
        let msgs = vec![
            BaseMessage::human("q1"),
            BaseMessage::ai("a1"),
            BaseMessage::ai_with_tool_calls(
                MessageContent::text("using"),
                vec![ToolCallRequest::new("tc1", "bash", json!({}))],
            ),
            BaseMessage::tool_result("tc1", "output"),
            BaseMessage::human("q2"),
            BaseMessage::ai("a2"),
        ];
        let rounds = group_messages_by_round(&msgs);
        assert!(rounds.len() >= 4);
        let result = truncate_for_ptl(&msgs, &rounds, 1);
        assert!(result.len() < msgs.len());
        assert!(result[0].content().contains("a1") || result[0].content().contains("using"));
    }

    #[test]
    fn test_ptl_truncate_drops_multiple() {
        let msgs: Vec<BaseMessage> = (0..5)
            .flat_map(|i| {
                vec![BaseMessage::human(format!("q{}", i)), BaseMessage::ai(format!("a{}", i))]
            })
            .collect();
        let rounds = group_messages_by_round(&msgs);
        assert_eq!(rounds.len(), 10);
        let result = truncate_for_ptl(&msgs, &rounds, 3);
        assert!(result.len() < msgs.len());
    }

    #[test]
    fn test_ptl_truncate_preserves_at_least_one() {
        let msgs: Vec<BaseMessage> = (0..3)
            .flat_map(|i| {
                vec![BaseMessage::human(format!("q{}", i)), BaseMessage::ai(format!("a{}", i))]
            })
            .collect();
        let rounds = group_messages_by_round(&msgs);
        let result = truncate_for_ptl(&msgs, &rounds, 5);
        assert!(!result.is_empty());
    }

    // is_ptl_error tests

    #[test]
    fn test_is_ptl_error_variants() {
        for msg in &[
            "prompt_too_long",
            "context_length_exceeded",
            "max_context_window",
            "token limit exceeded",
            "too many tokens",
        ] {
            let err = AgentError::LlmError(msg.to_string());
            assert!(is_ptl_error(&err), "expected '{}' to be PTL error", msg);
        }
    }

    #[test]
    fn test_is_not_ptl_error() {
        let err = AgentError::LlmError("connection timeout".to_string());
        assert!(!is_ptl_error(&err));
    }

    // full_compact integration tests

    #[tokio::test]
    async fn test_full_compact_basic() {
        use crate::messages::ToolCallRequest;
        use serde_json::json;
        let msgs = vec![
            BaseMessage::human("帮我写个函数"),
            BaseMessage::ai_with_tool_calls(
                MessageContent::text("using bash"),
                vec![ToolCallRequest::new("tc1", "bash", json!({"command": "echo"}))],
            ),
            BaseMessage::tool_result("tc1", "编译成功"),
        ];
        let model = MockBaseModel::new("## 摘要\n用户请求编写函数");
        let config = CompactConfig::default();
        let result = full_compact(&msgs, &model, &config, "").await.unwrap();
        assert!(result.summary.contains("此会话从之前的对话延续"));
        assert_eq!(result.messages_used, 3);
    }

    #[tokio::test]
    async fn test_full_compact_empty_messages() {
        let model = MockBaseModel::new("summary");
        let config = CompactConfig::default();
        let result = full_compact(&[], &model, &config, "").await.unwrap();
        assert!(result.summary.contains("无有效对话历史"));
        assert_eq!(result.messages_used, 0);
    }

    #[tokio::test]
    async fn test_full_compact_system_only() {
        let msgs = vec![BaseMessage::system("old summary")];
        let model = MockBaseModel::new("summary");
        let config = CompactConfig::default();
        let result = full_compact(&msgs, &model, &config, "").await.unwrap();
        assert!(result.summary.contains("无有效对话历史"));
        assert_eq!(result.messages_used, 1);
    }

    #[tokio::test]
    async fn test_full_compact_with_instructions() {
        let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
        let model = MockBaseModel::new("summary with instructions");
        let config = CompactConfig::default();
        let result = full_compact(&msgs, &model, &config, "请特别关注文件路径信息")
            .await
            .unwrap();
        assert!(result.summary.contains("此会话从之前的对话延续"));
    }

    #[tokio::test]
    async fn test_full_compact_ptl_retry_succeeds() {
        let msgs: Vec<BaseMessage> = (0..5)
            .flat_map(|i| {
                vec![BaseMessage::human(format!("q{}", i)), BaseMessage::ai(format!("a{}", i))]
            })
            .collect();
        let model = MockBaseModel::new_with_ptl_fail("摘要", 2);
        let mut config = CompactConfig::default();
        config.ptl_max_retries = 3;
        let result = full_compact(&msgs, &model, &config, "").await.unwrap();
        assert!(result.summary.contains("摘要"));
        assert!(result.messages_used < msgs.len());
    }

    #[tokio::test]
    async fn test_full_compact_ptl_retry_exhausted() {
        let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
        let model = MockBaseModel::new_with_ptl_fail("摘要", 5);
        let mut config = CompactConfig::default();
        config.ptl_max_retries = 3;
        let result = full_compact(&msgs, &model, &config, "").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("PTL"), "错误消息应提及 PTL，实际: {}", err_msg);
    }

    #[tokio::test]
    async fn test_full_compact_non_ptl_error() {
        let msgs = vec![BaseMessage::human("hello")];
        struct FailModel;
        #[async_trait]
        impl BaseModel for FailModel {
            async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
                Err(AgentError::LlmError("connection refused".to_string()))
            }
            fn provider_name(&self) -> &str { "fail" }
            fn model_id(&self) -> &str { "fail-model" }
        }
        let config = CompactConfig::default();
        let result = full_compact(&msgs, &FailModel, &config, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("connection refused"));
    }
}
