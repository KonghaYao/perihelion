use std::path::{Path, PathBuf};

use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::AgentResult;
use rust_create_agent::middleware::r#trait::Middleware;

use crate::parse_agent_file;

/// agent.md 中可覆盖 system prompt 的部分
///
/// 所有字段均为 `Option`，`None` 表示使用默认值。
#[derive(Debug, Clone, Default)]
pub struct AgentOverrides {
    /// 角色定位（替换 `{{persona}}`）
    pub persona: Option<String>,
    /// 输出风格（替换 `{{tone_and_style}}`）
    pub tone: Option<String>,
    /// 主动性（替换 `{{proactiveness}}`）
    pub proactiveness: Option<String>,
}

impl AgentOverrides {
    pub fn is_empty(&self) -> bool {
        self.persona.is_none() && self.tone.is_none() && self.proactiveness.is_none()
    }
}

/// AgentDefineMiddleware - 根据 agent_id 注入 Claude Code Agent 定义文件
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
/// tone: |
///   Be thorough and explain your reasoning in detail.
/// proactiveness: |
///   Proactively review related files and suggest improvements.
/// ---
///
/// You are a code reviewer. Focus on code quality and best practices.
/// ```
pub struct AgentDefineMiddleware;

impl AgentDefineMiddleware {
    pub fn new() -> Self {
        Self
    }

    /// 根据 cwd 和 agent_id 构建候选路径列表
    ///
    /// 如果 agent_id 包含路径分隔符或 `..`，返回空列表以防止路径遍历。
    pub fn candidate_paths(cwd: &str, agent_id: &str) -> Vec<PathBuf> {
        if agent_id.is_empty()
            || agent_id.contains('/')
            || agent_id.contains('\\')
            || agent_id.contains("..")
        {
            return Vec::new();
        }
        let cwd = Path::new(cwd);
        vec![
            cwd.join(".claude").join("agents").join(agent_id).join("agent.md"),
            cwd.join(".claude").join("agents").join(format!("{}.md", agent_id)),
            cwd.join("agents").join(agent_id).join("agent.md"),
            cwd.join("agents").join(format!("{}.md", agent_id)),
        ]
    }

    /// 按优先级找到第一个存在的文件
    fn find_file(cwd: &str, agent_id: &str) -> Option<PathBuf> {
        Self::candidate_paths(cwd, agent_id)
            .into_iter()
            .find(|p| p.is_file())
    }

    /// 同步读取 agent.md，返回可覆盖 system prompt 的各个部分。
    ///
    /// 供 TUI 层在构建 LLM 前提前获取覆盖内容，传入 `build_system_prompt`。
    /// 返回 `None` 表示文件不存在或无有效内容。
    pub fn load_overrides(cwd: &str, agent_id: &str) -> Option<AgentOverrides> {
        let path = Self::find_file(cwd, agent_id)?;
        let content = std::fs::read_to_string(&path).ok()?;
        if content.trim().is_empty() {
            return None;
        }

        if let Some(agent) = parse_agent_file(&content) {
            let persona = if agent.system_prompt.is_empty() {
                None
            } else {
                Some(agent.system_prompt)
            };
            let overrides = AgentOverrides {
                persona,
                tone: agent.frontmatter.tone,
                proactiveness: agent.frontmatter.proactiveness,
            };
            if overrides.is_empty() {
                return None;
            }
            return Some(overrides);
        }

        // 没有有效 frontmatter，把整个文件内容当作 persona
        let text = content.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(AgentOverrides { persona: Some(text), ..Default::default() })
        }
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

    async fn before_agent(&self, _state: &mut S) -> AgentResult<()> {
        // 覆盖注入已在构建 LLM 时通过 build_system_prompt(overrides, cwd) 完成，
        // 中间件层无需再操作消息列表。
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_overrides_persona_only() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("code-reviewer.md"),
            "---\nname: code-reviewer\ndescription: Reviews code\n---\n\nYou are a code reviewer.\n",
        )
        .unwrap();

        let ov = AgentDefineMiddleware::load_overrides(dir.path().to_str().unwrap(), "code-reviewer").unwrap();
        assert_eq!(ov.persona.as_deref().unwrap().trim(), "You are a code reviewer.");
        assert!(ov.tone.is_none());
        assert!(ov.proactiveness.is_none());
    }

    #[test]
    fn test_load_overrides_with_tone_and_proactiveness() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("analyst.md"),
            "---\nname: analyst\ndescription: Data analyst\ntone: Be thorough and detailed.\nproactiveness: Proactively explore related data.\n---\n\nYou are a data analyst.\n",
        )
        .unwrap();

        let ov = AgentDefineMiddleware::load_overrides(dir.path().to_str().unwrap(), "analyst").unwrap();
        assert!(ov.persona.is_some());
        assert_eq!(ov.tone.as_deref().unwrap().trim(), "Be thorough and detailed.");
        assert_eq!(ov.proactiveness.as_deref().unwrap().trim(), "Proactively explore related data.");
    }

    #[test]
    fn test_load_overrides_nested_dir() {
        let dir = tempdir().unwrap();
        let agent_dir = dir.path().join(".claude").join("agents").join("security-auditor");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("agent.md"),
            "---\nname: security-auditor\ndescription: Audit\n---\n\nYou are a security auditor.\n",
        )
        .unwrap();

        let ov = AgentDefineMiddleware::load_overrides(dir.path().to_str().unwrap(), "security-auditor").unwrap();
        assert_eq!(ov.persona.as_deref().unwrap().trim(), "You are a security auditor.");
    }

    #[test]
    fn test_load_overrides_no_file_returns_none() {
        let ov = AgentDefineMiddleware::load_overrides("/nonexistent", "unknown");
        assert!(ov.is_none());
    }

    #[test]
    fn test_candidate_paths_rejects_traversal() {
        assert!(AgentDefineMiddleware::candidate_paths("/tmp", "../etc/passwd").is_empty());
        assert!(AgentDefineMiddleware::candidate_paths("/tmp", "foo/../../bar").is_empty());
        assert!(AgentDefineMiddleware::candidate_paths("/tmp", "a\\b").is_empty());
        assert!(AgentDefineMiddleware::candidate_paths("/tmp", "").is_empty());
        // 正常 agent_id 应产生 4 条候选路径
        assert_eq!(AgentDefineMiddleware::candidate_paths("/tmp", "my-agent").len(), 4);
    }

    #[test]
    fn test_load_overrides_plain_markdown() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("plain.md"), "Just a plain persona.").unwrap();

        let ov = AgentDefineMiddleware::load_overrides(dir.path().to_str().unwrap(), "plain").unwrap();
        assert_eq!(ov.persona.as_deref().unwrap(), "Just a plain persona.");
        assert!(ov.tone.is_none());
    }
}
