use rand::Rng;

pub const DEFAULT_VERBS: &[&str] = &[
    "处理中",
    "分析中",
    "思考中",
    "生成中",
    "搜索中",
    "读取中",
    "编写中",
    "执行中",
    "计算中",
];

pub fn pick_verb(active_form: Option<&str>) -> String {
    active_form
        .map(|s| format!("{}…", s))
        .unwrap_or_else(|| {
            let mut rng = rand::thread_rng();
            DEFAULT_VERBS[rng.gen_range(0..DEFAULT_VERBS.len())].to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_verb_with_active_form() {
        let result = pick_verb(Some("搜索文件"));
        assert!(result.contains("搜索文件…"), "expected '搜索文件…', got '{}'", result);
    }

    #[test]
    fn test_pick_verb_random() {
        let result = pick_verb(None);
        assert!(!result.is_empty(), "verb should not be empty");
        assert!(
            DEFAULT_VERBS.contains(&result.as_str()),
            "'{}' should be in DEFAULT_VERBS",
            result
        );
    }
}
