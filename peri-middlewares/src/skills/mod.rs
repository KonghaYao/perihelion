pub mod loader;

pub use loader::{list_skills, load_skill_metadata, SkillMetadata};

use std::path::PathBuf;

use async_trait::async_trait;
use peri_agent::agent::state::State;
use peri_agent::error::AgentResult;
use peri_agent::messages::BaseMessage;
use peri_agent::middleware::r#trait::Middleware;

/// 全局配置文件路径：~/.peri/settings.json
pub fn global_config_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".peri")
        .join("settings.json")
}

/// 从全局配置中加载 skills_dir 路径
pub fn load_global_skills_dir() -> Option<PathBuf> {
    let path = global_config_path();
    if !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // 支持嵌套 { "config": { "skillsDir": "..." } } 或扁平 { "skillsDir": "..." }
    let skills_dir = json
        .get("config")
        .and_then(|c| c.get("skillsDir"))
        .or_else(|| json.get("skillsDir"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    skills_dir.filter(|p| !p.as_os_str().is_empty())
}

/// SkillsMiddleware - 渐进式 Skills 摘要注入
///
/// 在 `before_agent` 时扫描 skills 目录，将所有 skill 的 name + description
/// 生成摘要系统消息前插到消息历史中。
///
/// 搜索路径（按优先级）：
/// 1. `{cwd}/.claude/skills/`（项目级，优先）
/// 2. 全局配置的 `skills_dir`（可配置）
/// 3. `{home}/.claude/code/skills/`（用户级）
pub struct SkillsMiddleware {
    project_skills_dir: Option<PathBuf>,
    global_skills_dir: Option<PathBuf>,
    user_skills_dir: Option<PathBuf>,
    extra_dirs: Vec<PathBuf>,
    /// Frozen skills summary (None = scan each turn from disk).
    frozen_summary: Option<String>,
}

impl SkillsMiddleware {
    pub fn new() -> Self {
        Self {
            project_skills_dir: None,
            global_skills_dir: None,
            user_skills_dir: None,
            extra_dirs: vec![],
            frozen_summary: None,
        }
    }

    /// 覆盖项目级 skills 目录（默认 `{cwd}/.claude/skills/`）
    pub fn with_project_dir(mut self, dir: PathBuf) -> Self {
        self.project_skills_dir = Some(dir);
        self
    }

    /// 设置全局 skills 目录（从配置读取）
    pub fn with_global_dir(mut self, dir: PathBuf) -> Self {
        self.global_skills_dir = Some(dir);
        self
    }

    /// 覆盖用户级 skills 目录（默认 `{home}/.claude/code/skills/`）
    pub fn with_user_dir(mut self, dir: PathBuf) -> Self {
        self.user_skills_dir = Some(dir);
        self
    }

    /// 从全局配置加载 skills 目录（默认从 `~/.peri/settings.json` 读取）
    pub fn with_global_config(mut self) -> Self {
        if let Some(dir) = load_global_skills_dir() {
            self.global_skills_dir = Some(dir);
        }
        self
    }

    /// 追加额外 skills 搜索目录（用于插件 skills 路径注入）
    /// 插件 skills 优先级低于项目级，同名先到先得
    pub fn with_extra_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_dirs = dirs;
        self
    }

    /// 注入冻结的 skills 摘要。设置后 `before_agent` 跳过目录扫描，
    /// 直接使用冻结内容。
    pub fn with_frozen_summary(mut self, summary: String) -> Self {
        self.frozen_summary = Some(summary);
        self
    }

    /// 一次性扫描并构建冻结的 skills 摘要。
    ///
    /// 返回 `None` 表示无 skills 可用。
    /// 供 session 创建时调用。
    pub fn build_frozen_summary(cwd: &str, extra_dirs: &[PathBuf]) -> Option<String> {
        let dirs = Self::resolve_dirs_static(cwd, extra_dirs);
        let skills = list_skills(&dirs);
        if skills.is_empty() {
            return None;
        }
        Some(Self::build_summary(&skills))
    }

    /// 在无 `&self` 时解析 skills 目录列表（供静态 frozen 构造使用）。
    pub fn resolve_dirs_static(cwd: &str, extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
        let user_dir = dirs_next::home_dir()
            .map(|h| h.join(".claude").join("skills"))
            .unwrap_or_default();

        let global_dir = load_global_skills_dir();

        let project_dir = PathBuf::from(cwd).join(".claude").join("skills");

        // 按优先级：~/.claude/skills > globalConfig > ./.claude/skills > 插件
        let mut dirs = vec![user_dir];
        if let Some(global) = global_dir {
            dirs.push(global);
        }
        dirs.push(project_dir);
        for dir in extra_dirs {
            if dir.is_dir() {
                dirs.push(dir.clone());
            }
        }
        dirs
    }

    /// 根据 cwd 解析实际搜索目录列表（用户级优先于项目级）
    fn resolve_dirs(&self, cwd: &str) -> Vec<PathBuf> {
        let user_dir = self.user_skills_dir.clone().unwrap_or_else(|| {
            dirs_next::home_dir()
                .map(|h| h.join(".claude").join("skills"))
                .unwrap_or_default()
        });

        let global_dir = self.global_skills_dir.clone();

        let project_dir = self
            .project_skills_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(cwd).join(".claude").join("skills"));

        // 按优先级：~/.claude/skills > globalConfig > ./.claude/skills > 插件
        let mut dirs = vec![user_dir];
        if let Some(global) = global_dir {
            dirs.push(global);
        }
        dirs.push(project_dir);
        for dir in &self.extra_dirs {
            if dir.is_dir() {
                dirs.push(dir.clone());
            }
        }
        dirs
    }

    /// 生成 skills 摘要系统消息内容
    pub fn build_summary(skills: &[SkillMetadata]) -> String {
        let mut lines = vec![
            "你可以使用以下 Skills（专项能力），在需要时提及其名称：".to_string(),
            String::new(),
        ];

        for skill in skills {
            lines.push(format!(
                "- **{}**: {} {}",
                skill.name,
                skill.path.display(),
                skill.description
            ));
        }

        lines.push(String::new());
        lines.push("如需加载某 skill 的完整内容，在消息中提及其 name 即可。用户一般会使用 '/skill-name' 的形式。".to_string());

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
        // 使用冻结摘要时跳过所有磁盘 I/O
        if let Some(ref summary) = self.frozen_summary {
            if !summary.trim().is_empty() {
                state.prepend_message(BaseMessage::system(summary.clone()));
            }
            return Ok(());
        }

        let dirs = self.resolve_dirs(state.cwd());
        let skills = tokio::task::spawn_blocking(move || list_skills(&dirs))
            .await
            .map_err(|e| peri_agent::error::AgentError::MiddlewareError {
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
    use peri_agent::agent::state::AgentState;
    use tempfile::tempdir;
    include!("mod_test.rs");
}
