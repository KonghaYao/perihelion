use std::path::{Path, PathBuf};

use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::AgentResult;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::middleware::r#trait::Middleware;

/// AgentsMdMiddleware - 注入项目指引文件（AGENTS.md / CLAUDE.md）
///
/// 在 `before_agent` 时，按优先级搜索指引文件并将内容前插为系统消息。
///
/// 搜索优先级：
/// 1. `{cwd}/AGENTS.md`
/// 2. `{cwd}/CLAUDE.md`
/// 3. `{cwd}/.claude/AGENTS.md`
/// 4. `{home}/.claude/AGENTS.md`（用户全局）
pub struct AgentsMdMiddleware {
    extra_search_paths: Vec<PathBuf>,
}

impl AgentsMdMiddleware {
    pub fn new() -> Self {
        Self {
            extra_search_paths: Vec::new(),
        }
    }

    /// 添加额外搜索路径（应用层可注入）
    pub fn with_extra_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.extra_search_paths = paths;
        self
    }

    /// 根据 cwd 构建候选路径列表（含默认路径 + 额外路径）
    fn candidate_paths(&self, cwd: &str) -> Vec<PathBuf> {
        let cwd = Path::new(cwd);
        let mut candidates = vec![
            cwd.join("AGENTS.md"),
            cwd.join("CLAUDE.md"),
            cwd.join(".claude").join("AGENTS.md"),
        ];

        if let Some(home) = dirs_next::home_dir() {
            candidates.push(home.join(".claude").join("AGENTS.md"));
        }

        candidates.extend(self.extra_search_paths.iter().cloned());

        candidates
    }

    /// 按优先级找到第一个存在的文件
    fn find_file(&self, cwd: &str) -> Option<PathBuf> {
        self.candidate_paths(cwd)
            .into_iter()
            .find(|p| p.is_file())
    }
}

impl Default for AgentsMdMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AgentsMdMiddleware {
    fn name(&self) -> &str {
        "AgentsMdMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        let Some(path) = self.find_file(state.cwd()) else {
            return Ok(());
        };

        let path_display = path.display().to_string();
        let content = tokio::task::spawn_blocking(move || std::fs::read_to_string(&path))
            .await
            .map_err(|e| rust_create_agent::error::AgentError::MiddlewareError {
                middleware: "AgentsMdMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?
            .map_err(|e| rust_create_agent::error::AgentError::MiddlewareError {
                middleware: "AgentsMdMiddleware".to_string(),
                reason: format!("读取 {} 失败: {e}", path_display),
            })?;

        if content.trim().is_empty() {
            return Ok(());
        }

        // 前插系统消息（置于消息历史开头，优先于 Human 消息）
        state.prepend_message(BaseMessage::system(content));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::state::AgentState;

    #[tokio::test]
    async fn test_no_file_no_op() {
        let mw = AgentsMdMiddleware::new();
        let mut state = AgentState::new("/nonexistent/path");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_with_file() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let agents_md = dir.path().join("AGENTS.md");
        std::fs::write(&agents_md, "# Project Guide\nDo things correctly.").unwrap();

        let mw = AgentsMdMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].is_system());
        assert!(state.messages()[0].content().contains("Project Guide"));
    }

    #[tokio::test]
    async fn test_priority_agents_over_claude() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "agents content").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "claude content").unwrap();

        let mw = AgentsMdMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("agents content"));
    }

    #[tokio::test]
    async fn test_prepends_before_existing_messages() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "system instructions").unwrap();

        let mw = AgentsMdMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        state.add_message(BaseMessage::human("user question"));
        mw.before_agent(&mut state).await.unwrap();

        // 系统消息应在 human 消息之前
        assert_eq!(state.messages().len(), 2);
        assert!(state.messages()[0].is_system());
        assert!(matches!(state.messages()[1], BaseMessage::Human { .. }));
    }
}
