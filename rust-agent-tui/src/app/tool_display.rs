/// 将绝对路径剥离 cwd 前缀，返回相对路径；失败则取末段文件名
fn strip_cwd(path: &str, cwd: Option<&str>) -> String {
    if let Some(cwd) = cwd {
        let base = if cwd.ends_with('/') {
            cwd.to_string()
        } else {
            format!("{}/", cwd)
        };
        if let Some(rel) = path.strip_prefix(&base) {
            return rel.to_string();
        }
    }
    // fallback：取最后一段文件名
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// 返回简短 display name，控制在 3-6 字符以保持 UI 对齐
pub fn format_tool_name(tool: &str) -> String {
    match tool {
        "Bash" => "Shell",
        "Read" => "Read",
        "Write" => "Write",
        "Edit" => "Edit",
        "Glob" => "Glob",
        "Grep" => "Grep",
        "folder_operations" => "Folder",
        "TodoWrite" => "Todo",
        "AskUserQuestion" => "Ask",
        "Agent" => "Agent",
        other => return to_pascal(other),
    }
    .to_string()
}

/// 返回参数摘要（含路径缩短逻辑）
pub fn format_tool_args(
    tool: &str,
    input: &serde_json::Value,
    cwd: Option<&str>,
) -> Option<String> {
    match tool {
        "Bash" => input["command"].as_str().map(|s| truncate(s, 60)),
        "Read" | "Write" | "Edit" => input["file_path"]
            .as_str()
            .map(|p| truncate(&strip_cwd(p, cwd), 60)),
        "Glob" => input["pattern"]
            .as_str()
            .map(|p| truncate(&strip_cwd(p, cwd), 60)),
        "Grep" => input["pattern"].as_str().map(|s| truncate(s, 60)),
        "folder_operations" => {
            let op = input["operation"].as_str().unwrap_or("?");
            let path = input["folder_path"].as_str().unwrap_or("?");
            Some(format!("{} {}", op, strip_cwd(path, cwd)))
        }
        _ => None,
    }
}

pub fn to_pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tool_name_new_names() {
        assert_eq!(format_tool_name("Read"), "Read");
        assert_eq!(format_tool_name("Write"), "Write");
        assert_eq!(format_tool_name("Edit"), "Edit");
        assert_eq!(format_tool_name("Glob"), "Glob");
        assert_eq!(format_tool_name("Grep"), "Grep");
        assert_eq!(format_tool_name("Bash"), "Shell");
        assert_eq!(format_tool_name("TodoWrite"), "Todo");
        assert_eq!(format_tool_name("AskUserQuestion"), "Ask");
        assert_eq!(format_tool_name("Agent"), "Agent");
    }

    #[test]
    fn test_format_tool_args_grep_uses_pattern() {
        let input = serde_json::json!({"pattern": "needle", "output_mode": "content"});
        let result = format_tool_args("Grep", &input, None);
        assert!(result.is_some(), "Grep 工具应返回 pattern 摘要");
        assert!(result.unwrap().contains("needle"), "应包含 pattern 内容");
    }

    #[test]
    fn test_format_tool_args_bash_uses_command() {
        let input = serde_json::json!({"command": "cargo test"});
        let result = format_tool_args("Bash", &input, None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("cargo test"));
    }

    #[test]
    fn test_old_tool_names_not_matched() {
        // 验证旧工具名不再被匹配（fallback 到 to_pascal）
        assert_eq!(format_tool_name("bash"), "Bash"); // fallback
        assert_eq!(format_tool_name("read_file"), "ReadFile"); // fallback to_pascal
        assert_eq!(format_tool_name("write_file"), "WriteFile"); // fallback to_pascal
        assert_eq!(format_tool_name("search_files_rg"), "SearchFilesRg"); // fallback to_pascal
        assert_eq!(format_tool_name("launch_agent"), "LaunchAgent"); // fallback to_pascal
    }
}
