pub const READ_ONLY_TOOLS: &[&str] = &["read_file", "glob_files", "search_files_rg", "ask_user_question"];

pub const MAX_RESULT_LINES: usize = 20;

pub fn should_collapse_by_default(tool_name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&tool_name)
}

pub fn truncate_result(lines: &[String], max: usize) -> (Vec<String>, Option<usize>) {
    if lines.len() <= max {
        return (lines.to_vec(), None);
    }
    (lines[..max].to_vec(), Some(lines.len() - max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_collapse_read_file() {
        assert!(should_collapse_by_default("read_file"));
    }

    #[test]
    fn test_should_not_collapse_bash() {
        assert!(!should_collapse_by_default("bash"));
    }

    #[test]
    fn test_truncate_result_short() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
        let (result, omitted) = truncate_result(&lines, 20);
        assert_eq!(result.len(), 10);
        assert!(omitted.is_none());
    }

    #[test]
    fn test_truncate_result_long() {
        let lines: Vec<String> = (0..30).map(|i| format!("line {}", i)).collect();
        let (result, omitted) = truncate_result(&lines, 20);
        assert_eq!(result.len(), 20);
        assert_eq!(omitted, Some(10));
    }
}
