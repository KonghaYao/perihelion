use tokio::sync::oneshot;

// ─── AskUserQuestionData ───────────────────────────────────────────────────────

/// 问题选项
#[derive(Debug, Clone)]
pub struct AskUserOption {
    pub label: String,
    pub description: Option<String>,
}

/// 单个问题的纯数据（无 channel，供 agent 层解析并批量聚合）
#[derive(Debug, Clone)]
pub struct AskUserQuestionData {
    pub tool_call_id: String,
    pub question: String,
    pub header: String,
    pub multi_select: bool,
    pub options: Vec<AskUserOption>,
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
        (
            Self {
                questions,
                response_tx,
            },
            response_rx,
        )
    }
}
