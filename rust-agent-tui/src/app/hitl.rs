use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use rust_agent_middlewares::prelude::{
    default_requires_approval, AskUserBatchRequest, AskUserInvoker, AskUserQuestionData, BatchItem,
    HitlDecision, HitlHandler,
};

// ─── ApprovalRequest ──────────────────────────────────────────────────────────

/// 批量审批请求：一次展示多项，等待用户统一确认
pub struct BatchApprovalRequest {
    pub items: Vec<BatchItem>,
    pub response_tx: oneshot::Sender<Vec<HitlDecision>>,
}

// ─── ApprovalEvent ────────────────────────────────────────────────────────────

pub enum ApprovalEvent {
    Batch(BatchApprovalRequest),
    AskUserBatch(AskUserBatchRequest),
}

// ─── TuiHitlHandler ───────────────────────────────────────────────────────────

pub struct TuiHitlHandler {
    approval_tx: mpsc::Sender<ApprovalEvent>,
}

impl TuiHitlHandler {
    pub fn new(approval_tx: mpsc::Sender<ApprovalEvent>) -> Arc<Self> {
        Arc::new(Self { approval_tx })
    }
}

#[async_trait]
impl HitlHandler for TuiHitlHandler {
    fn requires_approval(&self, tool_name: &str, _input: &serde_json::Value) -> bool {
        default_requires_approval(tool_name)
    }

    async fn request_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> HitlDecision {
        let mut results = self
            .request_approval_batch(&[BatchItem {
                tool_name: tool_name.to_string(),
                input: input.clone(),
            }])
            .await;
        results.pop().unwrap_or(HitlDecision::Reject)
    }

    async fn request_approval_batch(&self, items: &[BatchItem]) -> Vec<HitlDecision> {
        let (response_tx, response_rx) = oneshot::channel();
        let req = BatchApprovalRequest { items: items.to_vec(), response_tx };
        if self.approval_tx.send(ApprovalEvent::Batch(req)).await.is_err() {
            return vec![HitlDecision::Reject; items.len()];
        }
        response_rx
            .await
            .unwrap_or_else(|_| vec![HitlDecision::Reject; items.len()])
    }
}

// ─── TuiAskUserHandler ────────────────────────────────────────────────────────

/// 将批量 ask_user 请求转发给 TUI 事件循环
pub struct TuiAskUserHandler {
    tx: mpsc::Sender<ApprovalEvent>,
}

impl TuiAskUserHandler {
    pub fn new(tx: mpsc::Sender<ApprovalEvent>) -> Arc<Self> {
        Arc::new(Self { tx })
    }
}

#[async_trait]
impl AskUserInvoker for TuiAskUserHandler {
    async fn ask_batch(&self, questions: Vec<AskUserQuestionData>) -> Vec<String> {
        let n = questions.len();
        let (req, response_rx) = AskUserBatchRequest::new(questions);
        if self.tx.send(ApprovalEvent::AskUserBatch(req)).await.is_err() {
            return vec!["[UI 已断开]".to_string(); n];
        }
        response_rx
            .await
            .unwrap_or_else(|_| vec!["[等待超时]".to_string(); n])
    }
}
