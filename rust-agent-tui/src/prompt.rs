const SYSTEM_PROMPT_TEMPLATE: &str = include_str!("../prompts/system.md");

/// 将模板中的占位符替换为运行时值
pub fn default_system_prompt(cwd: &str) -> String {
    SYSTEM_PROMPT_TEMPLATE.replace("{{cwd}}", cwd)
}
