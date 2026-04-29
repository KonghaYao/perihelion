use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::interaction::{
    InteractionContext, InteractionResponse, QuestionItem, QuestionOption, UserInteractionBroker,
};
use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use crate::ask_user::ask_user_tool_definition;

// ─── AskUserTool ──────────────────────────────────────────────────────────────

/// `ask_user_question` 工具的 BaseTool 实现
///
/// 将 ask_user_question LLM 工具调用转化为对 [`UserInteractionBroker`] 的调用，
/// 挂起等待用户通过 UI 提供答案后恢复。支持单次调用传入 1–4 个问题。
pub struct AskUserTool {
    broker: Arc<dyn UserInteractionBroker>,
}

impl AskUserTool {
    pub fn new(broker: Arc<dyn UserInteractionBroker>) -> Self {
        Self { broker }
    }
}

// ─── 解析辅助 ─────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct InputOption {
    label: String,
    description: Option<String>,
}

#[derive(serde::Deserialize)]
struct InputQuestion {
    question: String,
    header: String,
    #[serde(default)]
    multi_select: bool,
    options: Vec<InputOption>,
}

#[derive(serde::Deserialize)]
struct AskUserInput {
    questions: Vec<InputQuestion>,
}

fn parse_questions(input: Value) -> Result<Vec<QuestionItem>, Box<dyn std::error::Error + Send + Sync>> {
    let parsed: AskUserInput = serde_json::from_value(input)
        .map_err(|e| format!("ask_user_question: 参数解析失败: {e}"))?;
    Ok(parsed.questions.into_iter().enumerate().map(|(i, q)| QuestionItem {
        id: format!("ask_user_question_{i}"),
        question: q.question,
        header: q.header,
        options: q.options.into_iter().map(|o| QuestionOption {
            label: o.label,
            description: o.description,
        }).collect(),
        multi_select: q.multi_select,
    }).collect())
}

#[async_trait]
impl BaseTool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user_question"
    }

    fn description(&self) -> &str {
        ask_user_tool_definition().description.leak()
    }

    fn parameters(&self) -> Value {
        ask_user_tool_definition().parameters
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let questions = parse_questions(input)?;
        let headers: Vec<String> = questions.iter().map(|q| q.header.clone()).collect();
        let single = questions.len() == 1;

        let ctx = InteractionContext::Questions { requests: questions };
        let response = self.broker.request(ctx).await;

        match response {
            InteractionResponse::Answers(answers) => {
                if single {
                    let answer = answers.into_iter().next().unwrap_or_else(|| rust_create_agent::interaction::QuestionAnswer {
                        id: String::new(),
                        selected: vec![],
                        text: None,
                    });
                    if let Some(text) = answer.text.filter(|t| !t.is_empty()) {
                        Ok(text)
                    } else if !answer.selected.is_empty() {
                        Ok(answer.selected.join(", "))
                    } else {
                        Ok("(用户未提供回答)".to_string())
                    }
                } else {
                    let parts: Vec<String> = headers.iter().zip(answers.iter()).map(|(header, answer)| {
                        let val = if let Some(ref text) = answer.text.as_ref().filter(|t| !t.is_empty()) {
                            text.to_string()
                        } else {
                            answer.selected.join(", ")
                        };
                        format!("[问: {header}]\n回答: {val}")
                    }).collect();
                    Ok(parts.join("\n\n"))
                }
            }
            _ => Err("ask_user_question: unexpected response type".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rust_create_agent::interaction::{
        InteractionContext, InteractionResponse, QuestionAnswer, UserInteractionBroker,
    };

    use super::*;

    struct MockBroker(InteractionResponse);

    #[async_trait::async_trait]
    impl UserInteractionBroker for MockBroker {
        async fn request(&self, _ctx: InteractionContext) -> InteractionResponse {
            self.0.clone()
        }
    }

    fn make_answer(selected: &[&str], text: Option<&str>) -> InteractionResponse {
        InteractionResponse::Answers(vec![QuestionAnswer {
            id: "ask_user_question_0".to_string(),
            selected: selected.iter().map(|s| s.to_string()).collect(),
            text: text.map(|s| s.to_string()),
        }])
    }

    fn make_tool(response: InteractionResponse) -> AskUserTool {
        AskUserTool::new(Arc::new(MockBroker(response)))
    }

    fn single_question_input() -> serde_json::Value {
        serde_json::json!({
            "questions": [{
                "question": "What is your choice?",
                "header": "H1",
                "multi_select": false,
                "options": [{"label": "选项A"}, {"label": "选项B"}]
            }]
        })
    }

    // ── 参数解析测试 ──

    #[tokio::test]
    async fn test_invalid_json_returns_err() {
        let tool = make_tool(make_answer(&[], None));
        let result = tool.invoke(serde_json::Value::Null).await;
        assert!(result.is_err(), "null input should return Err");
    }

    #[tokio::test]
    async fn test_missing_questions_key_returns_err() {
        let tool = make_tool(make_answer(&[], None));
        let result = tool.invoke(serde_json::json!({})).await;
        assert!(result.is_err(), "missing questions key should return Err");
    }

    #[tokio::test]
    async fn test_valid_single_question_parsed() {
        let tool = make_tool(make_answer(&["选项A"], None));
        let result = tool.invoke(single_question_input()).await.unwrap();
        assert_eq!(result, "选项A");
    }

    // ── 单问题返回格式 ──

    #[tokio::test]
    async fn test_single_question_selected_answer() {
        let tool = make_tool(make_answer(&["选项A"], None));
        let result = tool.invoke(single_question_input()).await.unwrap();
        assert_eq!(result, "选项A");
    }

    #[tokio::test]
    async fn test_single_question_text_input() {
        let tool = make_tool(make_answer(&[], Some("自定义输入")));
        let result = tool.invoke(single_question_input()).await.unwrap();
        assert_eq!(result, "自定义输入");
    }

    #[tokio::test]
    async fn test_single_question_text_priority_over_selected() {
        let tool = make_tool(make_answer(&["选项A"], Some("自定义")));
        let result = tool.invoke(single_question_input()).await.unwrap();
        assert_eq!(result, "自定义", "non-empty text should take priority over selected");
    }

    #[tokio::test]
    async fn test_single_question_empty_selected() {
        let tool = make_tool(make_answer(&[], None));
        let result = tool.invoke(single_question_input()).await.unwrap();
        assert_eq!(result, "(用户未提供回答)", "empty selected and no text should return meaningful message");
    }

    // ── 多问题返回格式 ──

    #[tokio::test]
    async fn test_multi_question_format() {
        let response = InteractionResponse::Answers(vec![
            QuestionAnswer { id: "ask_user_question_0".into(), selected: vec!["v1".into()], text: None },
            QuestionAnswer { id: "ask_user_question_1".into(), selected: vec!["v2".into()], text: None },
        ]);
        let tool = make_tool(response);
        let result = tool
            .invoke(serde_json::json!({
                "questions": [
                    {"question": "Q1?", "header": "H1", "options": [{"label": "v1"}]},
                    {"question": "Q2?", "header": "H2", "options": [{"label": "v2"}]}
                ]
            }))
            .await
            .unwrap();
        assert_eq!(result, "[问: H1]\n回答: v1\n\n[问: H2]\n回答: v2");
    }

    #[tokio::test]
    async fn test_multi_question_multi_select_join() {
        // Single question with multi_select, multiple selected options
        let response = InteractionResponse::Answers(vec![QuestionAnswer {
            id: "ask_user_question_0".into(),
            selected: vec!["A".into(), "B".into()],
            text: None,
        }]);
        let tool = make_tool(response);
        let result = tool
            .invoke(serde_json::json!({
                "questions": [{
                    "question": "Pick all?",
                    "header": "H1",
                    "multi_select": true,
                    "options": [{"label": "A"}, {"label": "B"}]
                }]
            }))
            .await
            .unwrap();
        assert_eq!(result, "A, B");
    }

    // ── 异常响应测试 ──

    #[tokio::test]
    async fn test_unexpected_response_type() {
        use rust_create_agent::interaction::ApprovalDecision;
        let response = InteractionResponse::Decisions(vec![ApprovalDecision::Approve]);
        let tool = make_tool(response);
        let result = tool.invoke(single_question_input()).await;
        assert!(result.is_err(), "non-Answers response should return Err");
    }
}
