use super::*;

/// 统一候选项：命令或 Skill，与渲染侧 hints.rs 保持一致
enum HintItem {
    Cmd { name: String },
    Skill { name: String },
}

impl HintItem {
    fn name(&self) -> &str {
        match self {
            HintItem::Cmd { name } => name,
            HintItem::Skill { name } => name,
        }
    }

    /// 命令优先于 Skill
    fn is_cmd(&self) -> bool {
        matches!(self, HintItem::Cmd { .. })
    }
}

impl App {
    /// 构建统一排序后的候选项列表（与渲染侧一致）
    fn build_hint_items(&self) -> Vec<HintItem> {
        let first_line = self.session_mgr.sessions[self.session_mgr.active]
            .ui
            .textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");
        if !first_line.starts_with('/') {
            return vec![];
        }
        let prefix = first_line.trim_start_matches('/');
        let cmd_candidates: Vec<_> = self.session_mgr.sessions[self.session_mgr.active]
            .commands
            .command_registry
            .match_prefix(prefix, &self.services.lc);
        let skill_candidates: Vec<_> = self.session_mgr.sessions[self.session_mgr.active]
            .commands
            .skills
            .iter()
            .filter(|s| prefix.is_empty() || s.name.contains(prefix))
            .collect();

        let mut items: Vec<HintItem> = Vec::new();
        for (name, _) in &cmd_candidates {
            items.push(HintItem::Cmd { name: name.clone() });
        }
        for skill in &skill_candidates {
            items.push(HintItem::Skill {
                name: skill.name.clone(),
            });
        }
        items.sort_by(|a, b| {
            let a_starts = a.name().starts_with(prefix) as u8;
            let b_starts = b.name().starts_with(prefix) as u8;
            // 前缀匹配优先 > 命令优先于 Skill > 字母序
            b_starts
                .cmp(&a_starts)
                .then_with(|| b.is_cmd().cmp(&a.is_cmd()))
                .then_with(|| a.name().cmp(b.name()))
        });
        items
    }

    /// 获取当前提示浮层的候选数量
    pub fn hint_candidates_count(&self) -> usize {
        self.build_hint_items().len()
    }

    /// Tab 补全：选中当前光标处的候选项，替换输入框内容
    pub fn hint_complete(&mut self) {
        let selected_name = {
            let items = self.build_hint_items();
            let cursor = self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .hint_cursor
                .unwrap_or(0);
            items.get(cursor).map(|item| item.name().to_string())
        };

        if let Some(name) = selected_name {
            self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .textarea = build_textarea(false);
            self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .textarea
                .insert_str(format!("/{} ", name));
            self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .hint_cursor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_agent_middlewares::skills::loader::SkillMetadata;
    include!("hint_ops_test.rs");
}
