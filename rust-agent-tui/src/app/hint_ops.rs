use super::*;

impl App {
    /// 获取当前提示浮层的候选数量和类型
    /// 返回 (候选总数, 选中的文本) — 用于 Tab 补全
    pub fn hint_candidates_count(&self) -> usize {
        let first_line = self
            .textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");
        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            self.command_registry.match_prefix(prefix).len()
        } else if first_line.starts_with('#') {
            let prefix = first_line.trim_start_matches('#');
            self.skills
                .iter()
                .filter(|s| prefix.is_empty() || s.name.contains(prefix))
                .take(8)
                .count()
        } else {
            0
        }
    }

    /// Tab 补全：选中当前光标处的候选项，替换输入框内容
    pub fn hint_complete(&mut self) {
        let first_line = self
            .textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let cursor = self.hint_cursor.unwrap_or(0);

        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            let candidates = self.command_registry.match_prefix(prefix);
            if let Some((name, _)) = candidates.get(cursor) {
                self.textarea = build_textarea(false, 0);
                self.textarea.insert_str(format!("/{} ", name));
                self.hint_cursor = None;
            }
        } else if first_line.starts_with('#') {
            let prefix = first_line.trim_start_matches('#').to_string();
            let candidates: Vec<_> = self
                .skills
                .iter()
                .filter(|s| prefix.is_empty() || s.name.contains(&prefix))
                .take(8)
                .collect();
            if let Some(skill) = candidates.get(cursor) {
                self.textarea = build_textarea(false, 0);
                self.textarea.insert_str(format!("#{} ", skill.name));
                self.hint_cursor = None;
            }
        }
    }
}
