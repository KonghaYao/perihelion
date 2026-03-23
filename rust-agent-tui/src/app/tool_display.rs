pub fn format_tool_call_display(tool: &str, input: &serde_json::Value) -> String {
    let name = to_pascal(tool);
    let arg = extract_display_arg(tool, input);
    match arg {
        Some(a) => format!("{}({})", name, truncate(&a, 60)),
        None => name,
    }
}

pub fn extract_display_arg(tool: &str, input: &serde_json::Value) -> Option<String> {
    let key = match tool {
        "bash" => "command",
        "read_file" => "file_path",
        "write_file" => "file_path",
        "edit_file" => "file_path",
        "glob_files" => "pattern",
        "search_files_rg" => {
            return input["args"].as_array().map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
        }
        "folder_operations" => {
            return Some(format!(
                "{} {}",
                input["operation"].as_str().unwrap_or("?"),
                input["folder_path"].as_str().unwrap_or("?")
            ));
        }
        _ => return None,
    };
    input[key].as_str().map(|s| s.to_string())
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
