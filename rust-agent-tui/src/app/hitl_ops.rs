use super::*;

impl App {
    /// 上下移动列表光标
    pub fn hitl_move(&mut self, delta: isize) {
        if let Some(InteractionPrompt::Approval(p)) = self.agent.interaction_prompt.as_mut() {
            p.move_cursor(delta);
        }
    }

    /// 切换当前项批准/拒绝
    pub fn hitl_toggle(&mut self) {
        if let Some(InteractionPrompt::Approval(p)) = self.agent.interaction_prompt.as_mut() {
            p.toggle_current();
        }
    }

    /// 全部批准并提交
    pub fn hitl_approve_all(&mut self) {
        if let Some(InteractionPrompt::Approval(mut p)) = self.agent.interaction_prompt.take() {
            p.approve_all();
            self.agent.pending_hitl_items =
                Some(p.items.iter().map(|item| item.tool_name.clone()).collect());
            p.confirm();
        }
    }

    /// 全部拒绝并提交
    pub fn hitl_reject_all(&mut self) {
        if let Some(InteractionPrompt::Approval(mut p)) = self.agent.interaction_prompt.take() {
            p.reject_all();
            self.agent.pending_hitl_items =
                Some(p.items.iter().map(|item| item.tool_name.clone()).collect());
            p.confirm();
        }
    }

    /// 按当前每项选择确认并提交
    pub fn hitl_confirm(&mut self) {
        if let Some(InteractionPrompt::Approval(p)) = self.agent.interaction_prompt.take() {
            self.agent.pending_hitl_items =
                Some(p.items.iter().map(|item| item.tool_name.clone()).collect());
            p.confirm();
        }
    }
}
