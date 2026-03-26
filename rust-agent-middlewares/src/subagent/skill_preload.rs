use std::path::PathBuf;

use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::AgentResult;
use rust_create_agent::messages::{BaseMessage, ContentBlock};
use rust_create_agent::middleware::r#trait::Middleware;

use crate::skills::{list_skills, load_global_skills_dir};

/// SkillPreloadMiddleware - 将指定 skill 全文以 fake read_file 工具调用注入到子 agent state
///
/// 在 `before_agent` 时，根据 `skill_names` 列表找到对应 SKILL.md 文件，
/// 将其内容以 Human → Ai[ToolUse] → Tool[ToolResult] 消息序列注入到 state 前端，
/// 使 LLM 从第一轮推理就能看到完整 skill 内容。
///
/// # 注入消息结构
///
/// ```text
/// [Human] "（系统：预加载 skill 文件）"
/// [Ai]    [ToolUse{read_file, skill_preload_0}, ToolUse{read_file, skill_preload_1}, ...]
/// [Tool]  ToolResult{skill_preload_0, skill_0_content}
/// [Tool]  ToolResult{skill_preload_1, skill_1_content}
/// ...
/// ```
///
/// 找不到的 skill 名称静默跳过，不报错。
pub struct SkillPreloadMiddleware {
    skill_names: Vec<String>,
    cwd: String,
}

impl SkillPreloadMiddleware {
    pub fn new(skill_names: Vec<String>, cwd: &str) -> Self {
        Self {
            skill_names,
            cwd: cwd.to_string(),
        }
    }

    /// 解析 skills 搜索目录：`~/.claude/skills/` → globalConfig → `{cwd}/.claude/skills/`
    fn resolve_dirs(&self) -> Vec<PathBuf> {
        let user_dir = dirs_next::home_dir()
            .map(|h| h.join(".claude").join("skills"))
            .unwrap_or_default();

        let global_dir = load_global_skills_dir();

        let project_dir = PathBuf::from(&self.cwd).join(".claude").join("skills");

        let mut dirs = vec![user_dir];
        if let Some(g) = global_dir {
            dirs.push(g);
        }
        dirs.push(project_dir);
        dirs
    }
}

#[async_trait]
impl<S: State> Middleware<S> for SkillPreloadMiddleware {
    fn name(&self) -> &str {
        "SkillPreloadMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        if self.skill_names.is_empty() {
            return Ok(());
        }

        let dirs = self.resolve_dirs();
        let names_lower: Vec<String> =
            self.skill_names.iter().map(|s| s.to_lowercase()).collect();

        // 在 blocking 线程中扫描目录 + 读取文件内容
        let skill_contents = tokio::task::spawn_blocking(move || {
            let all_skills = list_skills(&dirs);
            all_skills
                .into_iter()
                .filter(|s| names_lower.contains(&s.name.to_lowercase()))
                .filter_map(|s| {
                    let content = std::fs::read_to_string(&s.path).ok()?;
                    Some((s.path.to_string_lossy().to_string(), content))
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| rust_create_agent::error::AgentError::MiddlewareError {
            middleware: "SkillPreloadMiddleware".to_string(),
            reason: format!("spawn_blocking 失败: {e}"),
        })?;

        if skill_contents.is_empty() {
            return Ok(());
        }

        // 构造 Ai 消息的 ToolUse ContentBlock 列表
        let tool_use_blocks: Vec<ContentBlock> = skill_contents
            .iter()
            .enumerate()
            .map(|(i, (path, _))| {
                ContentBlock::tool_use(
                    format!("skill_preload_{}", i),
                    "read_file",
                    serde_json::json!({ "path": path }),
                )
            })
            .collect();

        // 逆序 prepend Tool 消息，保证最终顺序 Tool[0] → Tool[1] → ...
        for (i, (_, content)) in skill_contents.iter().enumerate().rev() {
            state.prepend_message(BaseMessage::tool_result(
                format!("skill_preload_{}", i),
                content.clone(),
            ));
        }

        // Prepend Ai 消息（ai_from_blocks 自动双写 tool_calls）
        state.prepend_message(BaseMessage::ai_from_blocks(tool_use_blocks));

        // Prepend Human 初始化消息（最后 prepend → 排最前）
        state.prepend_message(BaseMessage::human("（系统：预加载 skill 文件）"));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::state::AgentState;
    use rust_create_agent::middleware::r#trait::Middleware;
    use tempfile::tempdir;

    fn write_skill(dir: &std::path::Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            "---\nname: '{}'\ndescription: '{}'\n---\n\n# {}\n\nSkill content for {}.\n",
            name, desc, name, name
        );
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[tokio::test]
    async fn test_no_op_when_empty_names() {
        let dir = tempdir().unwrap();
        let mw = SkillPreloadMiddleware::new(vec![], dir.path().to_str().unwrap());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_inject_single_skill() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "api-guide", "API 开发指南");

        let mw = SkillPreloadMiddleware::new(
            vec!["api-guide".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // Human + Ai + Tool = 3 条消息
        assert_eq!(state.messages().len(), 3, "应注入 3 条消息");
    }

    #[tokio::test]
    async fn test_inject_multiple_skills() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "skill-a", "技能 A");
        write_skill(&skills_dir, "skill-b", "技能 B");
        write_skill(&skills_dir, "skill-c", "技能 C");

        let mw = SkillPreloadMiddleware::new(
            vec!["skill-a".to_string(), "skill-b".to_string(), "skill-c".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // Human + Ai + Tool × 3 = 5 条消息
        assert_eq!(state.messages().len(), 5, "3 个 skill 应注入 5 条消息");
    }

    #[tokio::test]
    async fn test_skip_missing_skill() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "exists", "存在的 skill");

        let mw = SkillPreloadMiddleware::new(
            vec!["exists".to_string(), "nonexistent".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // 只有 "exists" 被注入：Human + Ai + Tool = 3 条
        assert_eq!(state.messages().len(), 3, "不存在的 skill 应静默跳过");
    }

    #[tokio::test]
    async fn test_no_op_when_all_skills_missing() {
        let dir = tempdir().unwrap();
        let mw = SkillPreloadMiddleware::new(
            vec!["nonexistent".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();
        assert_eq!(state.messages().len(), 0, "全部找不到时应 no-op");
    }

    #[tokio::test]
    async fn test_message_order() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "skill-x", "技能 X");
        write_skill(&skills_dir, "skill-y", "技能 Y");

        let mw = SkillPreloadMiddleware::new(
            vec!["skill-x".to_string(), "skill-y".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let msgs = state.messages();
        // [0] Human
        assert!(
            matches!(&msgs[0], BaseMessage::Human { .. }),
            "messages[0] 应为 Human，实际为 {:?}",
            &msgs[0]
        );
        assert!(
            msgs[0].content().contains("预加载 skill 文件"),
            "Human 消息内容应包含 '预加载 skill 文件'"
        );
        // [1] Ai
        assert!(
            matches!(&msgs[1], BaseMessage::Ai { .. }),
            "messages[1] 应为 Ai"
        );
        assert!(msgs[1].has_tool_calls(), "Ai 消息应包含工具调用");
        // [2] Tool
        assert!(
            matches!(&msgs[2], BaseMessage::Tool { .. }),
            "messages[2] 应为 Tool"
        );
        // [3] Tool
        assert!(
            matches!(&msgs[3], BaseMessage::Tool { .. }),
            "messages[3] 应为 Tool"
        );
    }

    #[tokio::test]
    async fn test_ai_message_has_tool_calls() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "skill-alpha", "Alpha");
        write_skill(&skills_dir, "skill-beta", "Beta");

        let mw = SkillPreloadMiddleware::new(
            vec!["skill-alpha".to_string(), "skill-beta".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let ai_msg = &state.messages()[1];
        let tool_calls = ai_msg.tool_calls();
        assert_eq!(tool_calls.len(), 2, "Ai 消息应有 2 个工具调用");
        assert_eq!(tool_calls[0].name, "read_file");
        assert_eq!(tool_calls[0].id, "skill_preload_0");
        assert_eq!(tool_calls[1].id, "skill_preload_1");
    }

    #[tokio::test]
    async fn test_tool_call_ids_match() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "my-skill", "My skill");

        let mw = SkillPreloadMiddleware::new(
            vec!["my-skill".to_string()],
            dir.path().to_str().unwrap(),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let msgs = state.messages();
        let ai_id = &msgs[1].tool_calls()[0].id;
        if let BaseMessage::Tool { tool_call_id, .. } = &msgs[2] {
            assert_eq!(tool_call_id, ai_id, "Tool 消息的 tool_call_id 应与 Ai 消息的 id 一致");
        } else {
            panic!("messages[2] 应为 Tool");
        }
    }
}
