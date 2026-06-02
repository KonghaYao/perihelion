use super::*;

use agent_client_protocol::schema::{
    RequestPermissionOutcome, RequestPermissionResponse, SelectedPermissionOutcome,
};

impl App {
    /// 上下移动列表光标
    pub fn hitl_move(&mut self, delta: isize) {
        if let Some(InteractionPrompt::Approval(p)) = self
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .as_mut()
        {
            p.move_cursor(delta);
        }
    }

    /// 切换当前项批准/拒绝
    pub fn hitl_toggle(&mut self) {
        if let Some(InteractionPrompt::Approval(p)) = self
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .as_mut()
        {
            p.toggle_current();
        }
    }

    /// 全部批准并提交
    pub fn hitl_approve_all(&mut self) {
        if let Some(InteractionPrompt::Approval(mut p)) = self
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .take()
        {
            p.approve_all();
            self.session_mgr.current_mut().agent.pending_hitl_items =
                Some(p.items.iter().map(|item| item.tool_name.clone()).collect());
            let approved = p.approved.clone();
            p.confirm();
            self.send_acp_hitl_response(&approved);
        }
    }

    /// 全部拒绝并提交
    pub fn hitl_reject_all(&mut self) {
        if let Some(InteractionPrompt::Approval(mut p)) = self
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .take()
        {
            p.reject_all();
            self.session_mgr.current_mut().agent.pending_hitl_items =
                Some(p.items.iter().map(|item| item.tool_name.clone()).collect());
            let approved = p.approved.clone();
            p.confirm();
            self.send_acp_hitl_response(&approved);
        }
    }

    /// 按当前每项选择确认并提交
    pub fn hitl_confirm(&mut self) {
        if let Some(InteractionPrompt::Approval(p)) = self
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .take()
        {
            self.session_mgr.current_mut().agent.pending_hitl_items =
                Some(p.items.iter().map(|item| item.tool_name.clone()).collect());
            let approved = p.approved.clone();
            p.confirm();
            self.send_acp_hitl_response(&approved);
        }
    }

    /// Send the HITL decision back via ACP transport.
    ///
    /// ACP sends one `RequestPermission` per approval item sequentially,
    /// so there's exactly one pending request id and one decision.
    fn send_acp_hitl_response(&mut self, approved: &[bool]) {
        let acp_client = match self.acp_client {
            Some(ref c) => c.clone(),
            None => return,
        };
        let request_id = match self
            .session_mgr
            .current_mut()
            .agent
            .pending_acp_request_id
            .take()
        {
            Some(id) => id,
            None => return,
        };
        // ACP broker sends one item per RequestPermission, so index 0 is the decision.
        let is_approved = approved.first().copied().unwrap_or(false);
        let response = if is_approved {
            RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
                SelectedPermissionOutcome::new("allow_once"),
            ))
        } else {
            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
        };
        let response_value = serde_json::to_value(&response).unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to serialize RequestPermissionResponse");
            serde_json::json!({})
        });
        tokio::spawn(async move {
            if let Err(e) = acp_client
                .send_response(request_id, Ok(response_value))
                .await
            {
                tracing::error!(error = %e, "ACP HITL response send failed");
            }
        });
    }
}
