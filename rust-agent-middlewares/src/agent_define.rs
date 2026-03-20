use std::path::{Path, PathBuf};

use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::{AgentError, AgentResult};
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::middleware::r#trait::Middleware;

use crate::{format_agent_id, parse_agent_file};

/// AgentDefineMiddleware - 根据 agent_id 注入 Claude Code Agent 定义文件
///
/// 在 `before_agent` 时，根据 state 中的 agent_id 查找对应的 md 文件并注入为系统消息。
///
/// Agent 定义文件搜索路径（按优先级）：
/// 1. `{cwd}/.claude/agents/{agent_id}/agent.md`
/// 2. `{cwd}/.claude/agents/{agent_id}.md`
/// 3. `{cwd}/agents/{agent_id}/agent.md`
/// 4. `{cwd}/agents/{agent_id}.md`
///
/// Agent 定义文件格式（Claude Code YAML frontmatter）：
/// ```markdown
/// ---
/// name: code-reviewer
/// description: Reviews code for quality and best practices
/// tools: Read, Glob, Grep
/// model: sonnet
/// ---
///
/// You are a code reviewer. Focus on code quality and best practices.
/// ```
pub struct AgentDefineMiddleware;

impl AgentDefineMiddleware {
    pub fn new() -> Self {
        Self
    }

    /// 根据 cwd 和 agent_id 构建候选路径列表（仅 Claude Code 路径）
    fn candidate_paths(&self, cwd: &str, agent_id: &str) -> Vec<PathBuf> {
        let cwd = Path::new(cwd);
        vec![
            // Claude Code 标准路径: .claude/agents/{id}/agent.md
            cwd.join(".claude")
                .join("agents")
                .join(agent_id)
                .join("agent.md"),
            cwd.join(".claude")
                .join("agents")
                .join(format!("{}.md", agent_id)),
            // Claude Code 标准路径: agents/{id}/agent.md
            cwd.join("agents").join(agent_id).join("agent.md"),
            cwd.join("agents").join(format!("{}.md", agent_id)),
        ]
    }

    /// 按优先级找到第一个存在的文件
    fn find_file(&self, cwd: &str, agent_id: &str) -> Option<PathBuf> {
        self.candidate_paths(cwd, agent_id)
            .into_iter()
            .find(|p| p.is_file())
    }

}

impl Default for AgentDefineMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AgentDefineMiddleware {
    fn name(&self) -> &str {
        "AgentDefineMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 从 state.context 获取 agent_id
        let agent_id = match state.get_context("agent_id") {
            Some(id) => id,
            None => return Ok(()), // 没有 agent_id，静默跳过
        };

        let Some(path) = self.find_file(state.cwd(), agent_id) else {
            return Ok(()); // 文件不存在，静默跳过
        };

        let path_display = path.display().to_string();
        let content = tokio::task::spawn_blocking(move || std::fs::read_to_string(&path))
            .await
            .map_err(|e| AgentError::MiddlewareError {
                middleware: "AgentDefineMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?
            .map_err(|e| AgentError::MiddlewareError {
                middleware: "AgentDefineMiddleware".to_string(),
                reason: format!("读取 {} 失败: {e}", path_display),
            })?;

        if content.trim().is_empty() {
            return Ok(());
        }

        // 解析 Claude Code agent 文件（YAML frontmatter + markdown）
        let agent = match parse_agent_file(&content) {
            Some(agent) => agent,
            None => {
                // 没有有效的 frontmatter，当作纯 markdown 处理
                let agent_name = format_agent_id(agent_id);
                let system_content = format!("[Agent: {}]\n\n{}", agent_name, content.trim());
                state.prepend_message(BaseMessage::system(system_content));
                return Ok(());
            }
        };

        // 使用 frontmatter 中的 name 或 agent_id 作为 agent 名称
        let agent_name = &agent.frontmatter.name;
        let system_content = format!("[Agent: {}]\n\n{}", agent_name, agent.system_prompt);

        // 前插系统消息
        state.prepend_message(BaseMessage::system(system_content));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::state::AgentState;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_no_agent_id_no_op() {
        let mw = AgentDefineMiddleware::new();
        let mut state = AgentState::new("/tmp");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_with_agent_file() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("code-reviewer.md"),
            r#"---
name: code-reviewer
description: Reviews code
tools: Read, Grep
---

You are a code reviewer.
"#,
        )
        .unwrap();

        let mw = AgentDefineMiddleware::new();
        let mut state =
            AgentState::new(dir.path().to_str().unwrap()).with_context("agent_id", "code-reviewer");
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].is_system());
        assert!(state.messages()[0].content().contains("code-reviewer"));
        assert!(state.messages()[0].content().contains("You are a code reviewer"));
    }

    #[tokio::test]
    async fn test_with_nested_agent_file() {
        let dir = tempdir().unwrap();
        let agent_dir = dir
            .path()
            .join(".claude")
            .join("agents")
            .join("security-auditor");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("agent.md"),
            r#"---
name: security-auditor
description: Audit security
---

You are a security auditor.
"#,
        )
        .unwrap();

        let mw = AgentDefineMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap())
            .with_context("agent_id", "security-auditor");
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("security-auditor"));
    }

    #[tokio::test]
    async fn test_file_not_found_silent_skip() {
        let mw = AgentDefineMiddleware::new();
        let mut state = AgentState::new("/nonexistent").with_context("agent_id", "unknown-agent");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_prepends_before_existing_messages() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("developer.md"),
            r#"---
name: developer
description: Developer agent
---

You are a developer.
"#,
        )
        .unwrap();

        let mw = AgentDefineMiddleware::new();
        let mut state =
            AgentState::new(dir.path().to_str().unwrap()).with_context("agent_id", "developer");
        state.add_message(BaseMessage::human("Hello"));

        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 2);
        assert!(state.messages()[0].is_system());
        assert!(state.messages()[0].content().contains("developer"));
        assert!(matches!(state.messages()[1], BaseMessage::Human { .. }));
    }

    #[test]
    fn test_format_agent_name() {
        assert_eq!(format_agent_id("code-reviewer"), "Code Reviewer");
        assert_eq!(format_agent_id("security_auditor"), "Security Auditor");
        assert_eq!(format_agent_id("devops"), "Devops");
    }
}
