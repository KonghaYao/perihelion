use super::*;

impl App {
    /// 获取当前提示浮层的候选数量（命令 + Skills 统一计数）
    pub fn hint_candidates_count(&self) -> usize {
        let first_line = self
            .core.textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");
        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            let cmd_count = self.core.command_registry.match_prefix(prefix)
                .into_iter()
                .take(6)
                .count();
            let skill_count = self.core.skills.iter()
                .filter(|s| prefix.is_empty() || s.name.contains(prefix))
                .take(4)
                .count();
            cmd_count + skill_count
        } else {
            0
        }
    }

    /// Tab 补全：选中当前光标处的候选项，替换输入框内容
    pub fn hint_complete(&mut self) {
        let first_line = self
            .core.textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let cursor = self.core.hint_cursor.unwrap_or(0);

        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            let cmd_candidates: Vec<_> = self.core.command_registry
                .match_prefix(prefix)
                .into_iter()
                .take(6)
                .collect();
            let cmd_count = cmd_candidates.len();

            let skill_candidates: Vec<_> = self.core.skills.iter()
                .filter(|s| prefix.is_empty() || s.name.contains(prefix))
                .take(4)
                .collect();

            if cursor < cmd_count {
                // 命令组
                if let Some((name, _)) = cmd_candidates.get(cursor) {
                    self.core.textarea = build_textarea(false);
                    self.core.textarea.insert_str(format!("/{} ", name));
                    self.core.hint_cursor = None;
                }
            } else {
                // Skills 组
                let skill_index = cursor - cmd_count;
                if let Some(skill) = skill_candidates.get(skill_index) {
                    self.core.textarea = build_textarea(false);
                    self.core.textarea.insert_str(format!("/{} ", skill.name));
                    self.core.hint_cursor = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_agent_middlewares::skills::loader::SkillMetadata;

    fn make_skill(name: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: format!("{} skill", name),
            path: std::path::PathBuf::from(format!("/tmp/{}.md", name)),
        }
    }

    #[tokio::test]
    async fn test_candidates_count_slash_prefix_returns_cmd_plus_skills() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("/");
        app.core.skills.push(make_skill("commit"));
        app.core.skills.push(make_skill("review"));

        let count = app.hint_candidates_count();
        let cmd_count = app.core.command_registry.match_prefix("").into_iter().take(6).count();
        assert_eq!(count, cmd_count + 2, "/ 前缀应返回命令数 + Skills 数");
    }

    #[tokio::test]
    async fn test_candidates_count_slash_prefix_filters_both() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("/mo");
        app.core.skills.push(make_skill("commit"));
        app.core.skills.push(make_skill("model-skill"));

        let count = app.hint_candidates_count();
        // "mo" 匹配命令 "model"，也匹配 skill "model-skill"
        assert_eq!(count, 2, "/mo 前缀应返回匹配的命令 + Skills 数");
    }

    #[tokio::test]
    async fn test_candidates_count_hash_prefix_returns_zero() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("#skill");
        app.core.skills.push(make_skill("skill"));

        let count = app.hint_candidates_count();
        assert_eq!(count, 0, "# 前缀不再产生候选");
    }

    #[tokio::test]
    async fn test_candidates_count_no_prefix_returns_zero() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("hello");

        let count = app.hint_candidates_count();
        assert_eq!(count, 0, "无前缀应返回 0");
    }

    #[tokio::test]
    async fn test_hint_complete_command_at_cursor_0() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("/m");
        app.core.hint_cursor = Some(0);

        app.hint_complete();
        let text: String = app.core.textarea.lines().iter().map(|s| s.as_str()).collect();
        assert!(text.starts_with("/model "), "cursor 0 应补全为第一个匹配命令 model，实际: {}", text);
        assert!(app.core.hint_cursor.is_none(), "补全后 hint_cursor 应为 None");
    }

    #[tokio::test]
    async fn test_hint_complete_skill_after_commands() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("/");
        app.core.skills.push(make_skill("commit"));

        // 设置 cursor 跳过所有命令（capped at 6）
        let cmd_count = app.core.command_registry.match_prefix("").into_iter().take(6).count();
        app.core.hint_cursor = Some(cmd_count); // 指向第一个 Skill

        app.hint_complete();
        let text: String = app.core.textarea.lines().iter().map(|s| s.as_str()).collect();
        assert!(text.starts_with("/commit "), "cursor 跳过命令组后应补全 Skill commit，实际: {}", text);
    }

    #[tokio::test]
    async fn test_hint_complete_clears_hint_cursor() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.core.textarea = build_textarea(false);
        app.core.textarea.insert_str("/m");
        app.core.hint_cursor = Some(0);

        app.hint_complete();
        assert_eq!(app.core.hint_cursor, None, "补全后 hint_cursor 应为 None");
    }
}
