use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::interaction::{
    InteractionContext, InteractionResponse, UserInteractionBroker,
};
use tokio::sync::{mpsc, oneshot};

use super::AgentEvent;

/// TuiInteractionBroker — 将统一人机交互请求转发给 TUI 事件循环
///
/// 同时取代旧的 `TuiHitlHandler`（审批）和 `TuiAskUserHandler`（问答）。
/// 调用方只需持有一个 broker 实例，对 HITL 和 AskUser 场景统一使用。
pub struct TuiInteractionBroker {
    tx: mpsc::Sender<AgentEvent>,
}

impl TuiInteractionBroker {
    pub fn new(tx: mpsc::Sender<AgentEvent>) -> Arc<Self> {
        Arc::new(Self { tx })
    }
}

#[async_trait]
impl UserInteractionBroker for TuiInteractionBroker {
    async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
        let (response_tx, response_rx) = oneshot::channel();
        if self
            .tx
            .send(AgentEvent::InteractionRequest { ctx, response_tx })
            .await
            .is_err()
        {
            // channel 关闭（TUI 已退出），安全降级：拒绝所有审批，返回空答案
            return InteractionResponse::Decisions(vec![]);
        }
        response_rx
            .await
            .unwrap_or(InteractionResponse::Decisions(vec![]))
    }
}
