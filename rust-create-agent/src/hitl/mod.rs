// ─── HitlDecision ──────────────────────────────────────────────────────────────

/// 用户对工具调用的审批决策
#[derive(Debug, Clone)]
pub enum HitlDecision {
    /// 批准执行（原始参数）
    Approve,
    /// 编辑后执行（修改工具调用参数）
    Edit(serde_json::Value),
    /// 拒绝执行
    Reject,
    /// 拒绝并向 LLM 回复原因
    Respond(String),
}

// ─── BatchItem ─────────────────────────────────────────────────────────────────

/// 批量审批请求的单项
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub tool_name: String,
    pub input: serde_json::Value,
}
