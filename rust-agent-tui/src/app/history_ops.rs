use super::*;

impl App {
    /// 记录一条历史（提交时调用）
    pub fn push_input_history(&mut self, text: String) {
        if self.sessions[self.active].core.input_history.first() == Some(&text) {
            return;
        }
        self.sessions[self.active].core.input_history.insert(0, text);
        self.sessions[self.active].core.input_history.truncate(200);
    }

    /// Up 键：向上浏览历史（更早的消息）
    pub fn history_up(&mut self) {
        if self.sessions[self.active].core.input_history.is_empty() {
            return;
        }
        let lines = self.sessions[self.active].core.textarea.lines().join("\n");
        match self.sessions[self.active].core.history_index {
            None => {
                if !lines.trim().is_empty() {
                    self.sessions[self.active].core.draft_input = Some(lines);
                }
                self.sessions[self.active].core.history_index = Some(0);
            }
            Some(idx) if idx + 1 < self.sessions[self.active].core.input_history.len() => {
                self.sessions[self.active].core.history_index = Some(idx + 1);
            }
            Some(_) => {}
        }
        self.restore_history_to_textarea();
    }

    /// Down 键：向下浏览历史（更新的消息）
    pub fn history_down(&mut self) {
        match self.sessions[self.active].core.history_index {
            Some(0) => {
                self.sessions[self.active].core.history_index = None;
                self.restore_draft();
            }
            Some(idx) => {
                self.sessions[self.active].core.history_index = Some(idx - 1);
                self.restore_history_to_textarea();
            }
            None => {}
        }
    }

    /// 退出历史浏览（任意输入字符时调用）
    pub fn exit_history(&mut self) {
        self.sessions[self.active].core.history_index = None;
        self.sessions[self.active].core.draft_input = None;
    }

    fn restore_history_to_textarea(&mut self) {
        if let Some(idx) = self.sessions[self.active].core.history_index {
            if let Some(text) = self.sessions[self.active].core.input_history.get(idx).cloned() {
                self.sessions[self.active].core.textarea = build_textarea(self.sessions[self.active].core.loading);
                self.sessions[self.active].core.textarea.insert_str(&text);
            }
        }
    }

    fn restore_draft(&mut self) {
        self.sessions[self.active].core.textarea = build_textarea(self.sessions[self.active].core.loading);
        if let Some(draft) = self.sessions[self.active].core.draft_input.take() {
            self.sessions[self.active].core.textarea.insert_str(&draft);
        }
    }
}
