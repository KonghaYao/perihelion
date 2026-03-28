use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::interaction::{
    InteractionContext, InteractionResponse, QuestionItem, QuestionOption, UserInteractionBroker,
};
use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use crate::ask_user::ask_user_tool_definition;

// ─── AskUserTool ──────────────────────────────────────────────────────────────

/// `ask_user` 工具的 BaseTool 实现
///
/// 将 ask_user LLM 工具调用转化为对 [`UserInteractionBroker`] 的调用，
/// 挂起等待用户通过 UI 提供答案后恢复。
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
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SelectType {
    SingleSelect,
    MultiSelect,
}

#[derive(serde::Deserialize)]
struct AskUserInput {
    description: String,
    #[serde(rename = "type")]
    select_type: SelectType,
    options: Vec<InputOption>,
    #[serde(default = "default_true")]
    allow_custom_input: bool,
    placeholder: Option<String>,
}

fn default_true() -> bool {
    true
}

fn parse_question(input: Value) -> Result<QuestionItem, Box<dyn std::error::Error + Send + Sync>> {
    let parsed: AskUserInput = serde_json::from_value(input)
        .map_err(|e| format!("ask_user: 参数解析失败: {e}"))?;
    Ok(QuestionItem {
        id: "ask_user".to_string(),
        question: parsed.description,
        options: parsed
            .options
            .into_iter()
            .map(|o| QuestionOption { label: o.label })
            .collect(),
        multi_select: matches!(parsed.select_type, SelectType::MultiSelect),
        allow_custom_input: parsed.allow_custom_input,
        placeholder: parsed.placeholder,
    })
}

#[async_trait]
impl BaseTool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        ask_user_tool_definition().description.leak()
    }

    fn parameters(&self) -> Value {
        ask_user_tool_definition().parameters
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let question = parse_question(input)?;

        let ctx = InteractionContext::Questions { requests: vec![question] };
        let response = self.broker.request(ctx).await;

        match response {
            InteractionResponse::Answers(mut answers) => {
                let answer = answers.pop().unwrap_or_else(|| rust_create_agent::interaction::QuestionAnswer {
                    id: String::new(),
                    selected: vec![],
                    text: None,
                });
                // 优先返回自定义文本，否则返回选中项（逗号拼接）
                if let Some(text) = answer.text.filter(|t| !t.is_empty()) {
                    Ok(text)
                } else {
                    Ok(answer.selected.join(", "))
                }
            }
            _ => Err("ask_user: unexpected response type".into()),
        }
    }
}
