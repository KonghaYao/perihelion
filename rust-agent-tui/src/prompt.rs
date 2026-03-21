use rust_agent_middlewares::AgentOverrides;

const SYSTEM_PROMPT_TEMPLATE: &str = include_str!("../prompts/system.md");

const SYSTEM_PROMPT_DEFAULT_AGENT: &str = include_str!("../prompts/default.md");

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

/// 用默认值构建系统提示词（无 agent 时使用）
pub fn default_system_prompt(cwd: &str) -> String {
    build_system_prompt(None, cwd)
}

/// 构建系统提示词。
///
/// `overrides` 存在时，将 agent.md 中定义的角色/风格/主动性拼成一个覆盖块，
/// 注入到 `{{agent_overrides}}` 占位符；为 `None` 时占位符替换为空字符串。
/// 安全策略、代码规范、任务流程、环境信息等硬约束始终保留。
pub fn build_system_prompt(overrides: Option<&AgentOverrides>, cwd: &str) -> String {
    let env = PromptEnv::detect(cwd);
    let overrides_block = overrides
        .map(build_agent_overrides_block)
        .unwrap_or(SYSTEM_PROMPT_DEFAULT_AGENT.to_string());

    SYSTEM_PROMPT_TEMPLATE
        .replace("{{agent_overrides}}", &overrides_block)
        .replace("{{cwd}}", &env.cwd)
        .replace(
            "{{is_git_repo}}",
            if env.is_git_repo { "Yes" } else { "No" },
        )
        .replace("{{platform}}", &env.platform)
        .replace("{{os_version}}", &env.os_version)
        .replace("{{date}}", &env.date)
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
