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
        // 等待 TUI 后台任务通过原始 request 的 response_tx 回复
        // oneshot sender drop（App 退出）时返回统一的 "UI 已断开" fallback，与 mpsc 失败路径保持一致
        response_rx.await.unwrap_or_else(|_| vec!["[UI 已断开]".to_string(); n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_agent_middlewares::prelude::AskUserQuestionData;

    /// 验证：approval_rx drop（TUI 退出）时，HITL 请求立即返回 Reject 不阻塞
    #[tokio::test]
    async fn test_hitl_mpsc_rx_drop_returns_reject() {
        let (tx, rx) = mpsc::channel::<ApprovalEvent>(4);
        let handler = TuiHitlHandler { approval_tx: tx };
        let items = vec![BatchItem {
            tool_name: "bash".to_string(),
            input: serde_json::json!({}),
        }];

        let handle = tokio::spawn(async move {
            handler.request_approval_batch(&items).await
        });

        // rx drop → approval_tx.send() 失败 → 立即返回 Reject
        drop(rx);
        let decisions = handle.await.unwrap();
        assert!(matches!(decisions[0], HitlDecision::Reject));
    }

    /// 验证：oneshot response_tx drop（hitl_prompt 被 drop）时，接收端立即返回 Reject 不阻塞
    #[tokio::test]
    async fn test_hitl_response_tx_drop_unblocks_receiver() {
        let (tx, mut rx) = mpsc::channel::<ApprovalEvent>(4);
        let handler = TuiHitlHandler { approval_tx: tx };
        let items = vec![BatchItem {
            tool_name: "bash".to_string(),
            input: serde_json::json!({}),
        }];

        let handle = tokio::spawn(async move {
            handler.request_approval_batch(&items).await
        });

        // 收到请求后 drop response_tx（不发送），模拟 App 退出时 hitl_prompt 被 drop
        if let Some(ApprovalEvent::Batch(req)) = rx.recv().await {
            drop(req.response_tx);
        }
        let decisions = handle.await.unwrap();
        assert!(matches!(decisions[0], HitlDecision::Reject),
            "response_tx drop 应返回 Reject");
    }

    /// 验证：AskUser oneshot response_tx drop 时，ask_batch 立即返回 "UI 已断开" 不阻塞
    #[tokio::test]
    async fn test_ask_user_response_tx_drop_returns_fallback() {
        let (tx, mut rx) = mpsc::channel::<ApprovalEvent>(4);
        let handler = TuiAskUserHandler { tx };
        let questions = vec![AskUserQuestionData {
            tool_call_id: "q1".to_string(),
            description: "test?".to_string(),
            multi_select: false,
            options: vec![],
            allow_custom_input: true,
            placeholder: None,
        }];

        let handle = tokio::spawn(async move { handler.ask_batch(questions).await });

        // 收到请求后 drop response_tx，模拟 AskUserBatchPrompt 被 drop
        if let Some(ApprovalEvent::AskUserBatch(req)) = rx.recv().await {
            drop(req.response_tx);
        }
        let answers = handle.await.unwrap();
        assert_eq!(answers[0], "[UI 已断开]", "response_tx drop 应返回 UI 已断开");
    }
}

