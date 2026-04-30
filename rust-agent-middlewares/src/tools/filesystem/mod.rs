pub mod edit;
pub mod folder;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

pub use edit::EditFileTool;
pub use folder::FolderOperationsTool;
pub use glob::GlobFilesTool;
pub use grep::GrepTool;
pub use read::ReadFileTool;
pub use write::WriteFileTool;

use std::path::{Path, PathBuf};

/// 统一路径解析：相对路径基于 cwd，绝对路径直接使用。
///
/// 通过 `canonicalize` 解析 `..`、`.`、symlinks，使 HITL 审批时显示真实路径，
/// 防止路径遍历攻击被用户忽略。对于尚不存在的文件，规范化父目录 + 文件名。
pub fn resolve_path(cwd: &str, file_path: &str) -> PathBuf {
    let raw = if Path::new(file_path).is_absolute() {
        Path::new(file_path).to_path_buf()
    } else {
        Path::new(cwd).join(file_path)
    };
    // 路径已存在：完整 canonicalize（解析 symlinks + ..）
    if raw.exists() {
        return raw.canonicalize().unwrap_or(raw);
    }
    // 路径不存在（新文件）：canonicalize 父目录 + 保留文件名
    if let (Some(parent), Some(file_name)) = (raw.parent(), raw.file_name()) {
        if let Ok(canon_parent) = parent.canonicalize() {
            return canon_parent.join(file_name);
        }
    }
    raw
}

/// 将输入字符串解析为 JSON Value，失败时原样返回为字符串
pub async fn parse_json_input(input: &str) -> serde_json::Value {
    serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_resolve_path_relative() {
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        let result = resolve_path(&cwd, "test.txt");
        // 父目录存在，应被规范化
        assert!(result.is_absolute());
        assert!(result.to_string_lossy().ends_with("test.txt"));
        // 不应包含未解析的 ..
        assert!(!result.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_resolve_path_absolute() {
        let result = resolve_path("/tmp", "/etc/hosts");
        // /etc/hosts 存在，应被 canonicalize
        if Path::new("/etc/hosts").exists() {
            assert_eq!(result, Path::new("/etc/hosts").canonicalize().unwrap());
        }
    }

    #[test]
    fn test_resolve_path_traversal_canonicalized() {
        // 在 cwd 中创建子目录，然后用 .. 遍历出去
        let tmp = std::env::temp_dir();
        let sub = tmp.join("resolve_test_sub");
        let _ = fs::create_dir_all(&sub);

        let cwd = sub.to_string_lossy().to_string();
        let result = resolve_path(&cwd, "../resolve_test_file.txt");
        // 结果不应包含 ..，应该是 tmp/resolve_test_file.txt
        assert!(!result.to_string_lossy().contains(".."));
        assert!(result.to_string_lossy().contains("resolve_test_file.txt"));

        let _ = fs::remove_dir(&sub);
    }
}
