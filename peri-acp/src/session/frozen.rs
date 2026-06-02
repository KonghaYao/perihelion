//! Shared frozen-data construction for session/new.
//!
//! Both TUI and Stdio paths build identical frozen data at session creation.
//! This module provides a single entry point to eliminate duplication.

use std::path::PathBuf;

use crate::session::executor::FrozenSessionData;

/// Build frozen session data from the given parameters.
///
/// Called once at session/new, capturing date/language/CLAUDE.md/skills/system_prompt.
/// `language` should be the user's configured language (`None` → auto-detect).
pub fn build_frozen_session_data(
    cwd: &str,
    language: Option<&str>,
    plugin_skill_dirs: &[PathBuf],
    plugin_agent_dirs: &[PathBuf],
    frozen_date: &str,
) -> FrozenSessionData {
    let (frozen_claude_md, frozen_claude_local_md) =
        peri_middlewares::AgentsMdMiddleware::read_frozen_content(cwd);

    let frozen_skill_summary =
        peri_middlewares::SkillsMiddleware::build_frozen_summary(cwd, plugin_skill_dirs);

    let features = crate::prompt::PromptFeatures::detect();
    let frozen_system_prompt = crate::prompt::build_system_prompt(
        None,
        cwd,
        features,
        plugin_agent_dirs,
        Some(frozen_date),
        language,
    );

    let is_git_repo = std::path::Path::new(cwd).join(".git").exists();

    FrozenSessionData {
        system_prompt: frozen_system_prompt,
        claude_md: frozen_claude_md,
        claude_local_md: frozen_claude_local_md,
        skill_summary: frozen_skill_summary,
        date: frozen_date.to_string(),
        is_git_repo,
        language: language.map(|s| s.to_string()),
    }
}
