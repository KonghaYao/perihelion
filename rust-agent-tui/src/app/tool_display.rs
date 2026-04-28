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
        "bash"               => "Shell",
        "read_file"          => "Read",
        "write_file"         => "Write",
        "edit_file"          => "Edit",
        "glob_files"         => "Glob",
        "search_files_rg"    => "Search",
        "folder_operations"  => "Folder",
        "todo_write"         => "Todo",
        "ask_user"           => "Ask",
        "ask_user_question"  => "Ask",
        "launch_agent"       => "Agent",
        other                => return to_pascal(other),
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
        "bash" => input["command"].as_str().map(|s| truncate(s, 60)),
        "read_file" | "write_file" | "edit_file" => input["file_path"]
            .as_str()
            .map(|p| truncate(&strip_cwd(p, cwd), 60)),
        "glob_files" => input["pattern"]
            .as_str()
            .map(|p| truncate(&strip_cwd(p, cwd), 60)),
        "search_files_rg" => input["args"].as_array().map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        }),
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
