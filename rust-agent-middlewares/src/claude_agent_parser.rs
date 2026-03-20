//! Claude Code Agent 文件解析器
//!
//! 解析 Claude Code 格式的 agent 定义文件（Markdown with YAML frontmatter）
//!
//! 文件格式示例：
//! ```markdown
//! ---
//! name: code-reviewer
//! description: Reviews code for quality and best practices
//! tools: Read, Glob, Grep
//! model: sonnet
//! ---
//!
//! You are a code reviewer...
//! ```

use serde::Deserialize;

/// Claude Code Agent YAML frontmatter 定义
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAgentFrontmatter {
    /// 使用小写字母和连字符的唯一标识符
    pub name: String,
    /// Claude 何时应委托给此 subagent
    pub description: String,
    /// subagent 可以使用的工具列表（逗号分隔字符串或数组）
    #[serde(default)]
    pub tools: ToolsValue,
    /// 要拒绝的工具列表
    #[serde(default)]
    pub disallowed_tools: ToolsValue,
    /// 使用的模型：sonnet、opus、haiku 或 inherit
    #[serde(default)]
    pub model: Option<String>,
    /// 权限模式：default、acceptEdits、dontAsk、bypassPermissions 或 plan
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// subagent 停止前的最大代理轮数
    #[serde(default)]
    pub max_turns: Option<u32>,
    /// 在启动时加载的 skills 列表
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP servers 配置
    #[serde(default)]
    pub mcp_servers: Vec<serde_yaml::Value>,
    /// Hooks 配置
    #[serde(default)]
    pub hooks: serde_yaml::Value,
    /// 持久内存范围：user、project 或 local
    #[serde(default)]
    pub memory: Option<String>,
    /// 是否始终在后台运行
    #[serde(default)]
    pub background: bool,
    /// Git worktree 隔离模式
    #[serde(default)]
    pub isolation: Option<String>,
}

/// 工具列表，可以是逗号分隔字符串或数组
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ToolsValue {
    #[default]
    Empty,
    List(Vec<String>),
}

impl<'de> Deserialize<'de> for ToolsValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        match value {
            serde_yaml::Value::String(s) => {
                // 解析逗号分隔的字符串
                let tools: Vec<String> = s
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                Ok(ToolsValue::List(tools))
            }
            serde_yaml::Value::Sequence(arr) => {
                let tools: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|t| !t.is_empty())
                    .collect();
                Ok(ToolsValue::List(tools))
            }
            _ => Ok(ToolsValue::Empty),
        }
    }
}

impl ToolsValue {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            ToolsValue::Empty => Vec::new(),
            ToolsValue::List(v) => v.clone(),
        }
    }
}

impl ClaudeAgent {
    /// 获取工具列表
    pub fn tools(&self) -> Vec<String> {
        self.frontmatter.tools.to_vec()
    }

    /// 获取被拒绝的工具列表
    pub fn disallowed_tools(&self) -> Vec<String> {
        self.frontmatter.disallowed_tools.to_vec()
    }
}

/// Claude Code Agent 定义
#[derive(Debug, Clone)]
pub struct ClaudeAgent {
    /// Frontmatter 配置
    pub frontmatter: ClaudeAgentFrontmatter,
    /// Markdown 正文（系统提示）
    pub system_prompt: String,
}

/// 将 agent_id（kebab-case 或 snake_case）格式化为友好显示名称
///
/// 例：`"code-reviewer"` → `"Code Reviewer"`，`"security_auditor"` → `"Security Auditor"`
pub fn format_agent_id(id: &str) -> String {
    id.split(|c: char| c == '-' || c == '_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 解析 Claude Code agent 文件内容
///
/// 返回 frontmatter 和 markdown 正文
pub fn parse_agent_file(content: &str) -> Option<ClaudeAgent> {
    // 检查是否有 YAML frontmatter
    let content = content.trim();

    if !content.starts_with("---") {
        return None;
    }

    // 找到第二个 "---" 的位置
    let first_end = content[3..].find("---")?;
    let yaml_content = &content[3..first_end + 3];
    let markdown_content = content[first_end + 6..].trim();

    // 解析 YAML frontmatter
    let frontmatter: ClaudeAgentFrontmatter = serde_yaml::from_str(yaml_content).ok()?;

    Some(ClaudeAgent {
        frontmatter,
        system_prompt: markdown_content.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_agent_file() {
        let content = r#"---
name: code-reviewer
description: Reviews code for quality
tools: Read, Grep, Glob
model: sonnet
---

You are a code reviewer. Focus on quality and best practices.
"#;

        let agent = parse_agent_file(content).unwrap();
        assert_eq!(agent.frontmatter.name, "code-reviewer");
        assert_eq!(agent.frontmatter.description, "Reviews code for quality");
        assert_eq!(agent.tools(), vec!["Read", "Grep", "Glob"]);
        assert_eq!(agent.frontmatter.model, Some("sonnet".to_string()));
        assert_eq!(
            agent.system_prompt,
            "You are a code reviewer. Focus on quality and best practices."
        );
    }

    #[test]
    fn test_parse_agent_with_optional_fields() {
        let content = r#"---
name: safe-researcher
description: Research with restrictions
tools: Read, Grep
disallowedTools: Write, Edit
maxTurns: 10
background: true
---

You are a researcher with restricted capabilities.
"#;

        let agent = parse_agent_file(content).unwrap();
        assert_eq!(agent.frontmatter.name, "safe-researcher");
        assert_eq!(agent.disallowed_tools(), vec!["Write", "Edit"]);
        assert_eq!(agent.frontmatter.max_turns, Some(10));
        assert!(agent.frontmatter.background);
    }

    #[test]
    fn test_parse_minimal_agent() {
        let content = r#"---
name: minimal-agent
description: A minimal agent
---

Basic system prompt.
"#;

        let agent = parse_agent_file(content).unwrap();
        assert_eq!(agent.frontmatter.name, "minimal-agent");
        assert!(agent.tools().is_empty());
        assert!(agent.frontmatter.model.is_none());
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "Just plain markdown without frontmatter.";
        assert!(parse_agent_file(content).is_none());
    }
}
