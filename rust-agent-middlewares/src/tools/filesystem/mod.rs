pub mod edit;
pub mod folder;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

pub use edit::EditFileTool;
pub use folder::FolderOperationsTool;
pub use glob::GlobFilesTool;
pub use grep::SearchFilesRgTool;
pub use read::ReadFileTool;
pub use write::WriteFileTool;

/// 将输入字符串解析为 JSON Value，失败时原样返回为字符串
pub async fn parse_json_input(input: &str) -> serde_json::Value {
    serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()))
}
