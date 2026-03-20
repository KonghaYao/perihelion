use rust_create_agent::agent::react::ToolCall;
use rust_create_agent::error::AgentError;

// 从核心库导入 trait 和数据类型
pub use rust_create_agent::ask_user::{
    AskUserBatchRequest, AskUserInvoker, AskUserOption, AskUserQuestionData,
};

// ─── AskUserHandler trait（TUI 批量请求接口）─────────────────────────────────

/// UI 层实现此 trait，决定如何展示问题并收集答案
pub use rust_create_agent::ask_user::AskUserInvoker as AskUserHandler;

// ─── 解析辅助 ──────────────────────────────────────────────────────────────────

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

fn default_true() -> bool { true }

/// 尝试将一个 ToolCall 解析为 AskUserQuestionData；非 ask_user 工具返回 None。
pub fn parse_ask_user(tool_call: &ToolCall) -> Result<Option<AskUserQuestionData>, AgentError> {
    if tool_call.name != "ask_user" {
        return Ok(None);
    }
    let input: AskUserInput = serde_json::from_value(tool_call.input.clone())
        .map_err(|e| AgentError::ToolExecutionFailed {
            tool: "ask_user".to_string(),
            reason: format!("参数解析失败: {e}"),
        })?;
    Ok(Some(AskUserQuestionData {
        tool_call_id: tool_call.id.clone(),
        description: input.description,
        multi_select: matches!(input.select_type, SelectType::MultiSelect),
        options: input.options.into_iter().map(|o| AskUserOption { label: o.label }).collect(),
        allow_custom_input: input.allow_custom_input,
        placeholder: input.placeholder,
    }))
}

// ─── `ask_user` 工具定义 ───────────────────────────────────────────────────────

/// `ask_user` 工具定义（注入系统提示词或工具列表时使用）
pub fn ask_user_tool_definition() -> rust_create_agent::tools::ToolDefinition {
    rust_create_agent::tools::ToolDefinition {
        name: "ask_user".to_string(),
        description: "向用户提问并提供选项，获取用户的选择或自定义输入。\
                      当任务需要用户提供细节、偏好或做出选择时使用。\
                      同一轮可以调用多次以聚合多个问题一起展示给用户。\
                      提供清晰的选项列表，并可选择是否允许自定义输入。".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "向用户提出的问题，清晰具体，包含必要的上下文"
                },
                "type": {
                    "type": "string",
                    "enum": ["single_select", "multi_select"],
                    "description": "选择类型：single_select（单选）或 multi_select（多选）"
                },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": {
                                "type": "string",
                                "description": "选项显示文本，简洁明了（1-50 字符）"
                            }
                        },
                        "required": ["label"]
                    },
                    "minItems": 2,
                    "maxItems": 6,
                    "description": "选项列表，至少 2 个，最多 6 个"
                },
                "allow_custom_input": {
                    "type": "boolean",
                    "default": true,
                    "description": "是否允许用户输入自定义文本，默认 true"
                },
                "placeholder": {
                    "type": "string",
                    "description": "自定义输入框的占位符文本（可选）"
                }
            },
            "required": ["description", "type", "options"]
        }),
    }
}
