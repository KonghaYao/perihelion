pub mod loader;

pub use loader::{list_skills, load_skill_metadata, SkillMetadata};

use std::path::PathBuf;

use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::AgentResult;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::middleware::r#trait::Middleware;

/// SkillsMiddleware - 渐进式 Skills 摘要注入
///
/// 在 `before_agent` 时扫描 skills 目录，将所有 skill 的 name + description
/// 生成摘要系统消息前插到消息历史中。
///
/// 搜索路径：
/// 1. `{cwd}/.claude/skills/`（项目级，优先）
/// 2. `{home}/.claude/code/skills/`（用户级）
pub struct SkillsMiddleware {
    project_skills_dir: Option<PathBuf>,
    user_skills_dir: Option<PathBuf>,
}

impl SkillsMiddleware {
    pub fn new() -> Self {
        Self {
            project_skills_dir: None,
            user_skills_dir: None,
        }
    }

    /// 覆盖项目级 skills 目录（默认 `{cwd}/.claude/skills/`）
    pub fn with_project_dir(mut self, dir: PathBuf) -> Self {
        self.project_skills_dir = Some(dir);
        self
    }

    /// 覆盖用户级 skills 目录（默认 `{home}/.claude/code/skills/`）
    pub fn with_user_dir(mut self, dir: PathBuf) -> Self {
        self.user_skills_dir = Some(dir);
        self
    }

    /// 根据 cwd 解析实际搜索目录列表（项目级优先）
    fn resolve_dirs(&self, cwd: &str) -> Vec<PathBuf> {
        let project_dir = self
            .project_skills_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(cwd).join(".claude").join("skills"));

        let user_dir = self.user_skills_dir.clone().unwrap_or_else(|| {
            dirs_next::home_dir()
                .map(|h| h.join(".claude").join("code").join("skills"))
                .unwrap_or_default()
        });

        vec![project_dir, user_dir]
    }

    /// 生成 skills 摘要系统消息内容
    fn build_summary(skills: &[SkillMetadata]) -> String {
        let mut lines = vec!["你可以使用以下 Skills（专项能力），在需要时提及其名称：".to_string(), String::new()];

        for skill in skills {
            lines.push(format!("- **{}**: {}", skill.name, skill.description));
        }

        lines.push(String::new());
        lines.push("如需加载某 skill 的完整内容，在消息中提及其 name 即可。".to_string());

        lines.join("\n")
    }
}

impl Default for SkillsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for SkillsMiddleware {
    fn name(&self) -> &str {
        "SkillsMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        let dirs = self.resolve_dirs(state.cwd());
        let skills = tokio::task::spawn_blocking(move || list_skills(&dirs))
            .await
            .map_err(|e| rust_create_agent::error::AgentError::MiddlewareError {
                middleware: "SkillsMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?;

        if skills.is_empty() {
            return Ok(());
        }

        let summary = Self::build_summary(&skills);
        state.prepend_message(BaseMessage::system(summary));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::state::AgentState;
    use tempfile::tempdir;

    fn write_skill(dir: &std::path::Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            "---\nname: '{}'\ndescription: '{}'\n---\n\n# {}\n",
            name, desc, name
        );
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[tokio::test]
    async fn test_no_skills_no_op() {
        let mw = SkillsMiddleware::new();
        let mut state = AgentState::new("/nonexistent/path");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_injects_summary() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "tui-dev", "构建 TUI 应用");
        write_skill(&skills_dir, "codebase-exploration", "深度代码搜索");

        let mw = SkillsMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        let msg = &state.messages()[0];
        assert!(msg.is_system());
        let content = msg.content();
        assert!(content.contains("tui-dev"));
        assert!(content.contains("codebase-exploration"));
        assert!(content.contains("Skills"));
    }

    #[tokio::test]
    async fn test_custom_project_dir() {
        let dir = tempdir().unwrap();
        write_skill(dir.path(), "custom-skill", "自定义技能");

        let mw = SkillsMiddleware::new().with_project_dir(dir.path().to_path_buf());
        let mut state = AgentState::new("/any/cwd");
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("custom-skill"));
    }
}
