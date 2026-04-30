use rust_create_agent::agent::react::ToolCall;
use rust_create_agent::error::AgentError;

// 从核心库导入 trait 和数据类型
pub use rust_create_agent::ask_user::{AskUserBatchRequest, AskUserOption, AskUserQuestionData};

// ─── 解析辅助 ──────────────────────────────────────────────────────────────────

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

/// 将一个 ToolCall 解析为 AskUserQuestionData 列表；非 ask_user_question 工具返回空 Vec。
pub fn parse_ask_user(tool_call: &ToolCall) -> Result<Vec<AskUserQuestionData>, AgentError> {
    if tool_call.name != "ask_user_question" {
        return Ok(vec![]);
    }
    let input: AskUserInput = serde_json::from_value(tool_call.input.clone()).map_err(|e| {
        AgentError::ToolExecutionFailed {
            tool: "ask_user_question".to_string(),
            reason: format!("参数解析失败: {e}"),
        }
    })?;
    Ok(input
        .questions
        .into_iter()
        .map(|q| AskUserQuestionData {
            tool_call_id: tool_call.id.clone(),
            question: q.question,
            header: q.header,
            multi_select: q.multi_select,
            options: q
                .options
                .into_iter()
                .map(|o| AskUserOption {
                    label: o.label,
                    description: o.description,
                })
                .collect(),
        })
        .collect())
}

// ─── `ask_user_question` 工具定义 ─────────────────────────────────────────────

/// `ask_user_question` 工具定义（对齐 Claude AskUserQuestion）
pub fn ask_user_tool_definition() -> rust_create_agent::tools::ToolDefinition {
    rust_create_agent::tools::ToolDefinition {
        name: "ask_user_question".to_string(),
        description: "向用户批量提问并提供选项，获取用户的选择或自定义输入。\
                      当任务需要用户提供细节、偏好或做出选择时使用。\
                      一次调用支持 1–4 个问题，全部打包展示给用户。\
                      每个问题提供清晰的选项列表，用户始终可以输入自定义内容。"
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "description": "问题列表，1–4 个问题",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "向用户提出的问题，清晰具体，包含必要的上下文"
                            },
                            "header": {
                                "type": "string",
                                "description": "问题短标签（<=12字），用于 UI Tab 显示，例如：颜色偏好、部署方式"
                            },
                            "multi_select": {
                                "type": "boolean",
                                "default": false,
                                "description": "是否允许多选，默认 false（单选）"
                            },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "选项显示文本，简洁明了（1-50 字符）"
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "选项说明，解释该选项的含义或适用场景（可选）"
                                        }
                                    },
                                    "required": ["label"]
                                },
                                "minItems": 2,
                                "maxItems": 4,
                                "description": "选项列表，至少 2 个，最多 4 个"
                            }
                        },
                        "required": ["question", "header", "options"]
                    }
                }
            },
            "required": ["questions"]
        }),
    }
}
