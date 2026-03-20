use async_trait::async_trait;

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

// ─── HitlHandler ──────────────────────────────────────────────────────────────

/// HITL 审批回调 trait — 应用层实现（TUI 弹窗、CLI 提示等）
#[async_trait]
pub trait HitlHandler: Send + Sync {
    /// 判断此工具调用是否需要用户审批
    fn requires_approval(&self, tool_name: &str, input: &serde_json::Value) -> bool;

    /// 请求用户审批单个工具调用，挂起直到用户做出决策
    async fn request_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> HitlDecision;

    /// 批量审批：一次展示多个待审批工具，返回与 items 等长的决策列表
    ///
    /// 默认实现：逐个串行调用 `request_approval`（退化为单次弹窗）。
    /// 应用层可覆盖为一次性弹窗展示所有项。
    async fn request_approval_batch(&self, items: &[BatchItem]) -> Vec<HitlDecision> {
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            results.push(self.request_approval(&item.tool_name, &item.input).await);
        }
        results
    }
}
