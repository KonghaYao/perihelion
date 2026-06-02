use super::*;

/// 统一候选项：命令、Skill 或 Agent 命令，与渲染侧 hints.rs 保持一致
enum HintItem {
    Cmd { name: String },
    Skill { name: String },
    AgentCmd { name: String },
}

impl HintItem {
    fn name(&self) -> &str {
        match self {
            HintItem::Cmd { name } => name,
            HintItem::Skill { name } => name,
            HintItem::AgentCmd { name } => name,
        }
    }
}

impl App {
    /// 构建统一排序后的候选项列表（与渲染侧一致）
    fn build_hint_items(&self) -> Vec<HintItem> {
        let first_line = self
            .session_mgr
            .current()
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
        let cmd_candidates: Vec<_> = self
            .session_mgr
            .current()
            .commands
            .command_registry
            .match_prefix(prefix, &self.services.lc);
        let skill_candidates: Vec<_> = self
            .session_mgr
            .current()
            .commands
            .skills
            .iter()
            .filter(|s| prefix.is_empty() || s.name.contains(prefix))
            .collect();
        // Agent commands from ACP AvailableCommandsUpdate (e.g. /compact)
        let agent_cmd_candidates: Vec<_> = self
            .session_mgr
            .current()
            .commands
            .agent_commands
            .iter()
            .filter(|n| prefix.is_empty() || n.contains(prefix))
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
        for name in &agent_cmd_candidates {
            items.push(HintItem::AgentCmd {
                name: (*name).clone(),
            });
        }
        items.sort_by(|a, b| {
            let a_starts = a.name().starts_with(prefix) as u8;
            let b_starts = b.name().starts_with(prefix) as u8;
            // 前缀匹配优先 > 命令 > Skill > AgentCmd > 字母序
            let a_rank = match a {
                HintItem::Cmd { .. } => 2,
                HintItem::Skill { .. } => 1,
                HintItem::AgentCmd { .. } => 0,
            };
            let b_rank = match b {
                HintItem::Cmd { .. } => 2,
                HintItem::Skill { .. } => 1,
                HintItem::AgentCmd { .. } => 0,
            };
            b_starts
                .cmp(&a_starts)
                .then_with(|| b_rank.cmp(&a_rank))
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
            let cursor = self.session_mgr.current().ui.hint_cursor.unwrap_or(0);
            items.get(cursor).map(|item| item.name().to_string())
        };

        if let Some(name) = selected_name {
            self.session_mgr.current_mut().ui.textarea = build_textarea(false);
            self.session_mgr
                .current_mut()
                .ui
                .textarea
                .insert_str(format!("/{} ", name));
            self.session_mgr.current_mut().ui.hint_cursor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peri_middlewares::skills::loader::SkillMetadata;
    include!("hint_ops_test.rs");
}
