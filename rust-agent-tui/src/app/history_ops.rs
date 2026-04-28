use super::*;

impl App {
    /// 记录一条历史（提交时调用）
    pub fn push_input_history(&mut self, text: String) {
        if self.core.input_history.first() == Some(&text) {
            return;
        }
        self.core.input_history.insert(0, text);
        self.core.input_history.truncate(200);
    }

    /// Up 键：向上浏览历史（更早的消息）
    pub fn history_up(&mut self) {
        if self.core.input_history.is_empty() {
            return;
        }
        let lines = self.core.textarea.lines().join("\n");
        match self.core.history_index {
            None => {
                if !lines.trim().is_empty() {
                    self.core.draft_input = Some(lines);
                }
                self.core.history_index = Some(0);
            }
            Some(idx) if idx + 1 < self.core.input_history.len() => {
                self.core.history_index = Some(idx + 1);
            }
            Some(_) => {}
        }
        self.restore_history_to_textarea();
    }

    /// Down 键：向下浏览历史（更新的消息）
    pub fn history_down(&mut self) {
        match self.core.history_index {
            Some(0) => {
                self.core.history_index = None;
                self.restore_draft();
            }
            Some(idx) => {
                self.core.history_index = Some(idx - 1);
                self.restore_history_to_textarea();
            }
            None => {}
        }
    }

    /// 退出历史浏览（任意输入字符时调用）
    pub fn exit_history(&mut self) {
        self.core.history_index = None;
        self.core.draft_input = None;
    }

    fn restore_history_to_textarea(&mut self) {
        if let Some(idx) = self.core.history_index {
            if let Some(text) = self.core.input_history.get(idx).cloned() {
                self.core.textarea = build_textarea(self.core.loading);
                self.core.textarea.insert_str(&text);
            }
        }
    }

    fn restore_draft(&mut self) {
        self.core.textarea = build_textarea(self.core.loading);
        if let Some(draft) = self.core.draft_input.take() {
            self.core.textarea.insert_str(&draft);
        }
    }
}
