use super::*;

impl App {
    pub fn ask_user_next_tab(&mut self) {
        if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.as_mut() {
            p.next_tab();
        }
    }

    pub fn ask_user_prev_tab(&mut self) {
        if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.as_mut() {
            p.prev_tab();
        }
    }

    pub fn ask_user_move(&mut self, delta: isize) {
        if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.as_mut() {
            p.current().move_option_cursor(delta);
            // 光标跟随滚动
            let cursor_row = p.current().option_cursor.max(0) as u16;
            p.scroll_offset = ensure_cursor_visible(cursor_row, p.scroll_offset, 10);
        }
    }

    pub fn ask_user_toggle(&mut self) {
        if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.as_mut() {
            p.current().toggle_current();
        }
    }

    pub fn ask_user_push_char(&mut self, c: char) {
        if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.as_mut() {
            p.current().push_char(c);
        }
    }

    pub fn ask_user_pop_char(&mut self) {
        if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.as_mut() {
            p.current().pop_char();
        }
    }

    /// Enter：确认当前问题。若全部问题均已确认则提交并关闭弹窗。
    /// 若当前问题没有选中任何选项（且不在自定义输入模式），自动选中光标所在选项。
    pub fn ask_user_confirm(&mut self) {
        let all_done = {
            let p = match self.agent.interaction_prompt.as_mut() {
                Some(InteractionPrompt::Questions(p)) => p,
                _ => return,
            };
            let q = &mut p.questions[p.active_tab];
            // 没有选中任何选项且不在自定义输入模式：自动选中当前光标行
            if !q.in_custom_input
                && !q.selected.iter().any(|&v| v)
                && q.custom_input.trim().is_empty()
            {
                q.toggle_current();
            }
            p.confirm_current()
        };

        if all_done {
            self.agent.pending_ask_user = None;
            if let Some(InteractionPrompt::Questions(p)) = self.agent.interaction_prompt.take() {
                p.confirm();
            }
        }
    }
}
