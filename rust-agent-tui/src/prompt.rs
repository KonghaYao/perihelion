use rust_agent_middlewares::AgentOverrides;

/// 控制 Feature-gated 提示词段落的注入
pub struct PromptFeatures {
    pub hitl_enabled: bool,
    pub subagent_enabled: bool,
    pub cron_enabled: bool,
    pub skills_enabled: bool,
}

impl PromptFeatures {
    /// 根据运行时环境推断功能开关
    pub fn detect() -> Self {
        Self {
            hitl_enabled: std::env::var("YOLO_MODE").as_deref() == Ok("false"),
            subagent_enabled: true,
            cron_enabled: true,
            skills_enabled: true,
        }
    }

    /// 全部关闭的配置（用于测试）
    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            hitl_enabled: false,
            subagent_enabled: false,
            cron_enabled: false,
            skills_enabled: false,
        }
    }
}

pub struct PromptEnv {
    pub cwd: String,
    pub is_git_repo: bool,
    pub platform: String,
    pub os_version: String,
    pub date: String,
}

impl PromptEnv {
    pub fn detect(cwd: &str) -> Self {
        let is_git_repo = std::path::Path::new(cwd).join(".git").exists();
        let platform = std::env::consts::OS.to_string();
        let os_version = os_version_string();
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        Self {
            cwd: cwd.to_string(),
            is_git_repo,
            platform,
            os_version,
            date,
        }
    }
}

/// 扫描 `.claude/agents/` 目录，格式化为 agent 列表字符串。
///
/// 格式：`- {agent_id}: {description}`
/// agent_id 即 subagent_type 参数值（文件名去掉 .md），作为主标识符。
/// 无 agent 时返回提示信息。
fn format_available_agents(cwd: &str, extra_agent_dirs: &[std::path::PathBuf]) -> String {
    let agents = rust_agent_middlewares::scan_agents_with_extra_dirs(cwd, extra_agent_dirs);
    if agents.is_empty() {
        return "No agents currently configured. You can add agent definitions in `.claude/agents/`.".to_string();
    }
    agents
        .iter()
        .map(|(agent_id, _name, description)| format!("- {}: {}", agent_id, description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 构建系统提示词。
///
/// 从 `prompts/sections/` 目录加载静态段落（01-07），根据 `PromptFeatures`
/// 条件注入 feature-gated 段落（10-13），将环境占位符替换为运行时值。
///
/// `overrides` 存在时，将 agent.md 中定义的角色/风格/主动性拼成一个覆盖块，
/// 注入到提示词最前面；为 `None` 时覆盖块为空（默认行为已由静态段落覆盖）。
pub fn build_system_prompt(
    overrides: Option<&AgentOverrides>,
    cwd: &str,
    features: PromptFeatures,
    extra_agent_dirs: &[std::path::PathBuf],
) -> String {
    let env = PromptEnv::detect(cwd);

    // 静态段落（编译时嵌入，按编号顺序）—— 01-06 为缓存稳定内容
    let static_sections: &[&str] = &[
        include_str!("../prompts/sections/01_intro.md"),
        include_str!("../prompts/sections/02_system.md"),
        include_str!("../prompts/sections/03_doing_tasks.md"),
        include_str!("../prompts/sections/04_actions.md"),
        include_str!("../prompts/sections/05_using_tools.md"),
        include_str!("../prompts/sections/06_tone_style.md"),
    ];

    // 动态段落（含环境变量占位符、feature-gated 段落）—— 边界标记之后，不参与缓存
    let mut dynamic_sections: Vec<&str> = Vec::new();
    dynamic_sections.push(include_str!("../prompts/sections/07_env.md"));
    if features.hitl_enabled {
        dynamic_sections.push(include_str!("../prompts/sections/10_hitl.md"));
    }
    if features.subagent_enabled {
        dynamic_sections.push(include_str!("../prompts/sections/11_subagent.md"));
    }
    if features.cron_enabled {
        dynamic_sections.push(include_str!("../prompts/sections/12_cron.md"));
    }
    if features.skills_enabled {
        dynamic_sections.push(include_str!("../prompts/sections/13_skills.md"));
    }

    let overrides_block = overrides
        .map(build_agent_overrides_block)
        .unwrap_or_default();

    // 合成：静态段落 + 边界标记 + 覆盖块 + 动态段落
    // 边界标记之前的全部内容可被 Anthropic prompt cache 命中；
    // 边界标记之后的内容（overrides、日期、cwd 等）变化不会破坏前缀缓存。
    // overrides_block 放在边界之后——不同 SubAgent 的 persona/tone 不同，
    // 若放在静态段之前会导致每个 agent 的缓存前缀完全失效。
    let mut result = String::new();
    for (i, section) in static_sections.iter().enumerate() {
        if i > 0 {
            result.push_str("\n\n");
        }
        result.push_str(section);
    }
    result.push_str("\n\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__");
    if !overrides_block.is_empty() {
        result.push_str("\n\n");
        result.push_str(&overrides_block);
    }
    for section in &dynamic_sections {
        result.push_str("\n\n");
        result.push_str(section);
    }

    result
        .replace("{{cwd}}", &env.cwd)
        .replace(
            "{{is_git_repo}}",
            if env.is_git_repo { "Yes" } else { "No" },
        )
        .replace("{{platform}}", &env.platform)
        .replace("{{os_version}}", &env.os_version)
        .replace("{{date}}", &env.date)
        .replace(
            "{{available_agents}}",
            &format_available_agents(&env.cwd, extra_agent_dirs),
        )
}

/// 将 `AgentOverrides` 拼成注入到提示词顶部的覆盖块。
///
/// 只包含非空字段，末尾加两个换行使其与后续默认内容自然分隔。
fn build_agent_overrides_block(ov: &AgentOverrides) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(persona) = &ov.persona {
        parts.push(persona.trim().to_string());
    }
    if let Some(tone) = &ov.tone {
        parts.push(format!("# Tone and style\n{}", tone.trim()));
    }
    if let Some(proactiveness) = &ov.proactiveness {
        parts.push(format!("# Proactiveness\n{}", proactiveness.trim()));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", parts.join("\n\n"))
    }
}

fn os_version_string() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
        {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !v.is_empty() {
                return format!("macOS {v}");
            }
        }
        "macOS".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/etc/os-release") {
            for line in s.lines() {
                if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                    return v.trim_matches('"').to_string();
                }
            }
        }
        "Linux".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_overrides_contains_all_sections() {
        let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[]);
        assert!(
            result.contains("Following conventions"),
            "应包含 02_system 段落"
        );
        assert!(result.contains("Doing tasks"), "应包含 03_doing_tasks 段落");
        assert!(result.contains("<env>"), "应包含 07_env 段落");
        assert!(
            result.contains("Working directory"),
            "应包含 08_env 替换后结果"
        );
    }

    #[test]
    fn test_no_overrides_no_duplicate_tone_proactiveness() {
        let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[]);
        // "# Tone and style" 仅出现 1 次（来自 06_tone_style.md 静态段落，不来自覆盖块）
        assert_eq!(
            result.matches("# Tone and style").count(),
            1,
            "无 overrides 时 # Tone and style 应仅出现 1 次（来自静态段落）"
        );
        // "# Proactiveness" 仅出现 1 次（来自 02_system.md 静态段落）
        assert_eq!(
            result.matches("# Proactiveness").count(),
            1,
            "无 overrides 时 # Proactiveness 应仅出现 1 次（来自静态段落）"
        );
        // "Simplicity" 出现在 04_actions.md
        assert!(
            result.contains("Simplicity"),
            "应包含 04_actions Simplicity 段落"
        );
    }

    #[test]
    fn test_no_overrides_no_leading_newlines() {
        let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[]);
        assert!(
            !result.starts_with("\n\n"),
            "无 overrides 时提示词不应以空行开头"
        );
    }

    #[test]
    fn test_with_overrides_uses_override_block() {
        let overrides = AgentOverrides {
            persona: Some("test persona".into()),
            tone: None,
            proactiveness: None,
        };
        let result = build_system_prompt(Some(&overrides), "/tmp", PromptFeatures::none(), &[]);
        // overrides 现在在边界标记之后，不再以 persona 开头
        let boundary = result.find("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__").unwrap();
        assert!(
            result[boundary..].contains("test persona"),
            "有 overrides 时边界之后应包含 persona 内容"
        );
        // 静态段应在 persona 之前（边界标记之前）
        assert!(
            !result[..boundary].contains("test persona"),
            "persona 不应在缓存段内"
        );
    }

    #[test]
    fn test_placeholders_replaced() {
        let result = build_system_prompt(None, "/custom/path", PromptFeatures::none(), &[]);
        assert!(!result.contains("{{"), "不应包含未替换的占位符");
        assert!(result.contains("/custom/path"), "cwd 占位符应被替换");
    }

    #[test]
    fn test_env_contains_cwd() {
        let result = build_system_prompt(None, "/custom/path", PromptFeatures::none(), &[]);
        assert!(result.contains("/custom/path"), "环境信息应包含 cwd");
    }

    #[test]
    fn test_features_none_excludes_all_gated_sections() {
        let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[]);
        assert!(
            !result.contains("Human-in-the-Loop"),
            "全关闭时不应包含 HITL 段落"
        );
        assert!(
            !result.contains("SubAgent Delegation"),
            "全关闭时不应包含 SubAgent 段落"
        );
        assert!(
            !result.contains("Scheduled Tasks"),
            "全关闭时不应包含 Cron 段落"
        );
        // 13_skills.md 以 "# Skills\n" 开头，检查标题
        assert!(
            !result.contains("\n# Skills\n") && !result.starts_with("# Skills\n"),
            "全关闭时不应包含 Skills 标题段落"
        );
    }

    #[test]
    fn test_hitl_enabled_includes_hitl_section() {
        let features = PromptFeatures {
            hitl_enabled: true,
            ..PromptFeatures::none()
        };
        let result = build_system_prompt(None, "/tmp", features, &[]);
        assert!(
            result.contains("Human-in-the-Loop"),
            "hitl_enabled 时应包含 HITL 段落"
        );
    }

    #[test]
    fn test_subagent_enabled_includes_subagent_section() {
        let features = PromptFeatures {
            subagent_enabled: true,
            ..PromptFeatures::none()
        };
        let result = build_system_prompt(None, "/tmp", features, &[]);
        assert!(
            result.contains("SubAgent Delegation"),
            "subagent_enabled 时应包含 SubAgent 段落"
        );
    }

    #[test]
    fn test_cron_enabled_includes_cron_section() {
        let features = PromptFeatures {
            cron_enabled: true,
            ..PromptFeatures::none()
        };
        let result = build_system_prompt(None, "/tmp", features, &[]);
        assert!(
            result.contains("Scheduled Tasks"),
            "cron_enabled 时应包含 Cron 段落"
        );
    }

    #[test]
    fn test_skills_enabled_includes_skills_section() {
        let features = PromptFeatures {
            skills_enabled: true,
            ..PromptFeatures::none()
        };
        let result = build_system_prompt(None, "/tmp", features, &[]);
        assert!(
            result.contains("# Skills"),
            "skills_enabled 时应包含 Skills 段落标题"
        );
    }

    #[test]
    fn test_all_features_enabled_includes_all() {
        let features = PromptFeatures {
            hitl_enabled: true,
            subagent_enabled: true,
            cron_enabled: true,
            skills_enabled: true,
        };
        let result = build_system_prompt(None, "/tmp", features, &[]);
        assert!(result.contains("Human-in-the-Loop"), "应包含 HITL 段落");
        assert!(
            result.contains("SubAgent Delegation"),
            "应包含 SubAgent 段落"
        );
        assert!(result.contains("Scheduled Tasks"), "应包含 Cron 段落");
        assert!(result.contains("# Skills"), "应包含 Skills 段落标题");
    }

    #[test]
    fn test_detect_default_values() {
        let features = PromptFeatures::detect();
        // 默认环境（无 YOLO_MODE 或 YOLO_MODE=true）下 hitl_enabled 为 false
        // 注意：测试环境中 YOLO_MODE 可能未设置
        assert!(features.subagent_enabled);
        assert!(features.cron_enabled);
        assert!(features.skills_enabled);
    }

    // ─── boundary marker tests ──────────────────────────────────────────────

    #[test]
    fn test_boundary_marker_present() {
        let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[]);
        assert!(
            result.contains("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__"),
            "system prompt 应包含边界标记"
        );
    }

    #[test]
    fn test_boundary_marker_before_dynamic_content() {
        let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[]);
        let boundary_pos = result.find("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__").unwrap();
        // 06_tone_style 在边界之前
        assert!(
            result[..boundary_pos].contains("# Tone and style"),
            "06_tone_style 应在边界标记之前"
        );
        // 07_env 在边界之后
        assert!(
            result[boundary_pos..].contains("Working directory"),
            "07_env 应在边界标记之后"
        );
    }

    #[test]
    fn test_boundary_marker_with_all_features() {
        let features = PromptFeatures {
            hitl_enabled: true,
            subagent_enabled: true,
            cron_enabled: true,
            skills_enabled: true,
        };
        let result = build_system_prompt(None, "/tmp", features, &[]);
        let boundary_pos = result.find("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__").unwrap();
        // feature-gated 段落都应在边界之后
        assert!(
            result[boundary_pos..].contains("Human-in-the-Loop"),
            "HITL 段落应在边界标记之后"
        );
        assert!(
            result[boundary_pos..].contains("SubAgent Delegation"),
            "SubAgent 段落应在边界标记之后"
        );
    }

    #[test]
    fn test_overrides_after_boundary_marker() {
        let overrides = AgentOverrides {
            persona: Some("test persona".into()),
            tone: Some("concise".into()),
            proactiveness: None,
        };
        let result = build_system_prompt(Some(&overrides), "/tmp", PromptFeatures::none(), &[]);
        let boundary_pos = result.find("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__").unwrap();
        // overrides 应在边界之后，不破坏缓存前缀
        assert!(
            result[boundary_pos..].contains("test persona"),
            "persona 应在边界标记之后"
        );
        assert!(
            result[boundary_pos..].contains("concise"),
            "tone 应在边界标记之后"
        );
        // 边界之前不应包含 overrides 内容
        assert!(
            !result[..boundary_pos].contains("test persona"),
            "persona 不应在边界标记之前（会破坏缓存前缀）"
        );
    }

    // ─── available_agents tests ──────────────────────────────────────────────

    /// Helper: create a unique temp directory under /tmp
    fn tmp_dir(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("{}_{}", prefix, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_available_agents_placeholder_replaced() {
        let dir = tmp_dir("prompt_test_agent_replaced");
        let agents_dir = dir.join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("tester.md"),
            "---\nname: tester\ndescription: A test agent\n---\n\nYou are a test agent.\n",
        )
        .unwrap();

        let features = PromptFeatures {
            subagent_enabled: true,
            ..PromptFeatures::none()
        };
        let result = build_system_prompt(None, dir.to_str().unwrap(), features, &[]);
        assert!(
            result.contains("- tester: A test agent"),
            "Should contain formatted agent entry, got: {}",
            result
        );
        assert!(
            !result.contains("{{available_agents}}"),
            "Placeholder should be replaced"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_available_agents_placeholder_empty_dir() {
        let dir = tmp_dir("prompt_test_agent_empty");
        // No .claude/agents/ directory at all
        let features = PromptFeatures {
            subagent_enabled: true,
            ..PromptFeatures::none()
        };
        let result = build_system_prompt(None, dir.to_str().unwrap(), features, &[]);
        assert!(
            result.contains("- explore:"),
            "Should contain built-in agents even without .claude/agents/ directory"
        );
        assert!(
            !result.contains("No agents currently configured"),
            "Should NOT show no-agents message when built-in agents exist"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_available_agents_not_replaced_when_subagent_disabled() {
        let dir = tmp_dir("prompt_test_agent_disabled");
        let features = PromptFeatures::none();
        let result = build_system_prompt(None, dir.to_str().unwrap(), features, &[]);
        assert!(
            !result.contains("SubAgent Delegation"),
            "SubAgent section should not be included when disabled"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_format_available_agents_with_agents() {
        let dir = tmp_dir("prompt_test_format_agents");
        let agents_dir = dir.join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("reviewer.md"),
            "---\nname: code-reviewer\ndescription: Reviews code\n---\n\nReview code.\n",
        )
        .unwrap();
        std::fs::write(
            agents_dir.join("analyst.md"),
            "---\nname: data-analyst\ndescription: Analyzes data\n---\n\nAnalyze data.\n",
        )
        .unwrap();

        let result = format_available_agents(dir.to_str().unwrap(), &[]);
        assert!(
            result.contains("- reviewer: Reviews code"),
            "Should contain reviewer entry"
        );
        assert!(
            result.contains("- analyst: Analyzes data"),
            "Should contain analyst entry"
        );
        // Should also contain built-in agents (explore, general-purpose, plan, verification)
        assert!(
            result.contains("- explore:"),
            "Should contain built-in explore agent"
        );
        // Verify project agents + built-in agents
        let lines: Vec<&str> = result.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(
            lines.len(),
            6,
            "Should have 2 project + 4 built-in agent entries"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_format_available_agents_empty_dir() {
        let result = format_available_agents("/nonexistent/path/that/does/not/exist", &[]);
        // Built-in agents are always available
        assert!(
            result.contains("- explore:"),
            "Should contain built-in agents even without .claude/agents/ directory"
        );
        assert!(
            !result.contains("No agents currently configured"),
            "Should NOT show no-agents message when built-in agents exist"
        );
    }
}
