use async_trait::async_trait;
use tokio::sync::oneshot;

// ─── AskUserQuestionData ───────────────────────────────────────────────────────

/// 问题选项
#[derive(Debug, Clone)]
pub struct AskUserOption {
    pub label: String,
}

/// 单个问题的纯数据（无 channel，供 agent 层解析并批量聚合）
#[derive(Debug, Clone)]
pub struct AskUserQuestionData {
    pub tool_call_id: String,
    pub description: String,
    pub multi_select: bool,
    pub options: Vec<AskUserOption>,
    pub allow_custom_input: bool,
    pub placeholder: Option<String>,
}

// ─── AskUserBatchRequest ───────────────────────────────────────────────────────

/// 批量问题请求（所有问题打包，带统一回复 channel）
///
/// 通过 [`AskUserBatchRequest::new`] 构建，自动创建 oneshot channel，
/// 返回 `(request, receiver)` 二元组。
pub struct AskUserBatchRequest {
    pub questions: Vec<AskUserQuestionData>,
    pub response_tx: oneshot::Sender<Vec<String>>,
}

impl AskUserBatchRequest {
    pub fn new(questions: Vec<AskUserQuestionData>) -> (Self, oneshot::Receiver<Vec<String>>) {
        let (response_tx, response_rx) = oneshot::channel();
        (Self { questions, response_tx }, response_rx)
    }
}

// ─── AskUserInvoker ────────────────────────────────────────────────────────────

/// 批量 ask_user 调用接口 —— 由应用层（TUI / CLI）实现
///
/// 解耦工具与具体 UI 实现：
/// - TUI 实现：将问题发送到 TUI 弹窗，挂起等待用户输入
/// - CLI 实现：在终端打印问题，读取用户输入
/// - 测试实现：预设答案自动回复
#[async_trait]
pub trait AskUserInvoker: Send + Sync {
    /// 批量提问，返回与 questions 等长的答案列表
    async fn ask_batch(&self, questions: Vec<AskUserQuestionData>) -> Vec<String>;
}
