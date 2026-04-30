use rust_create_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

/// Read tool - 与 TypeScript read_tool 对齐
pub struct ReadFileTool {
    pub cwd: String,
}

impl ReadFileTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

const MAX_LINES: usize = 2000;
/// 最大允许读取的文件大小（32 MB）
const MAX_FILE_SIZE: u64 = 32 * 1024 * 1024;

const READ_FILE_DESCRIPTION: &str = r#"Reads a file from the local filesystem. You can access any file directly by using this tool.
Assume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.

Usage:
- The file_path parameter must be an absolute path, not a relative path
- By default, it reads up to 2000 lines starting from the beginning of the file
- You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters
- Any lines longer than 65536 characters will be truncated
- Results are returned using cat -n format, with line numbers starting at 1
- This tool reads files from the local filesystem; it cannot handle URLs
- You can call multiple tools in a single response. It is always better to speculatively read multiple files before making edits
- You should prefer using the Read tool over the Bash tool with commands like cat, head, tail, or sed to read files. This provides better output formatting and filtering
- For open-ended searches that may require multiple rounds of globbing and grepping, use the Agent tool instead

Error handling:
- File not found: returns an error message indicating the path does not exist
- Binary files: detected by extension and returns a message indicating the file cannot be displayed as text
- Files exceeding 32 MB: returns an error suggesting use of offset/limit parameters
- Offset exceeds file length: returns an error indicating the line range is invalid"#;

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "ico"
            | "webp"
            | "tiff"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "zip"
            | "rar"
            | "7z"
            | "tar"
            | "gz"
            | "mp3"
            | "wav"
            | "ogg"
            | "flac"
            | "mp4"
            | "avi"
            | "mkv"
            | "mov"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "class"
    )
}

#[async_trait::async_trait]
impl BaseTool for ReadFileTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        READ_FILE_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "The line number to start reading from. Only provide if the file is too large to read in a single call. Not providing this parameter reads the whole file (recommended)"
                },
                "limit": {
                    "type": "number",
                    "description": "The number of lines to read. Only provide if the file is too large to read in a single call. Not providing this parameter reads the whole file (recommended)"
                },
                "pages": {
                    "type": "string",
                    "description": "For PDF files, the page range to read, e.g. '1-5', '3', '10-20'. Only applies to PDF files"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or("Missing file_path parameter")?;

        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(MAX_LINES as u64) as usize;

        let resolved = resolve_path(&self.cwd, file_path);

        let pages = input["pages"].as_str().map(|s| s.to_string());

        // PDF + pages: 返回占位提示
        if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("pdf") {
                if pages.is_some() {
                    return Ok(format!(
                        "[PDF READING NOT YET SUPPORTED]\n\nFile path: {}\nPDF reading with page selection is not yet implemented. Use the Bash tool with a PDF reader command as a workaround.",
                        resolved.display()
                    ));
                }
                // PDF 但未提供 pages → 继续走到下面的二进制检测，返回 BINARY FILE DETECTED
            }
        }

        if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
            if is_binary_extension(&ext.to_lowercase()) {
                return Ok(format!(
                    "[BINARY FILE DETECTED]\n\nFile type: .{ext}\nFile path: {}\n\nThis is a binary file and cannot be displayed as text.",
                    resolved.display()
                ));
            }
        }

        let content = match std::fs::metadata(&resolved) {
            Ok(meta) if meta.len() > MAX_FILE_SIZE => {
                return Ok(format!(
                    "Error: File too large ({} bytes, max {} bytes). Use offset/limit to read portions.",
                    meta.len(),
                    MAX_FILE_SIZE
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(format!("Error: File not found at {file_path}"));
            }
            Err(e) => return Err(e.into()),
            _ => match std::fs::read_to_string(&resolved) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(format!("Error: File not found at {file_path}"));
                }
                Err(e) => return Err(e.into()),
            },
        };

        let lines: Vec<&str> = content.split('\n').collect();
        if offset >= lines.len() {
            return Ok(format!(
                "Error: offset {} exceeds file length ({} lines)",
                offset,
                lines.len()
            ));
        }
        let start = offset;
        let end = (start + limit).min(lines.len());
        let selected = &lines[start..end];

        let numbered: Vec<String> = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        Ok(numbered.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_file_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "hello\nworld").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "file.txt"}))
            .await
            .unwrap();
        assert!(
            result.contains("1\thello"),
            "should contain line 1: {result}"
        );
        assert!(
            result.contains("2\tworld"),
            "should contain line 2: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "nonexistent.txt"}))
            .await
            .unwrap();
        assert!(
            result.contains("File not found"),
            "should report not found: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, "L1\nL2\nL3\nL4\nL5").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "lines.txt", "offset": 2, "limit": 2}))
            .await
            .unwrap();
        // offset=2 → starts at index 2 (L3), limit=2 → L3 and L4
        assert!(result.contains("3\tL3"), "should contain line 3: {result}");
        assert!(result.contains("4\tL4"), "should contain line 4: {result}");
        assert!(!result.contains("L1"), "should not contain L1");
        assert!(!result.contains("L5"), "should not contain L5");
    }

    #[tokio::test]
    async fn test_read_file_binary_extension() {
        let dir = tempfile::tempdir().unwrap();
        // Binary extension check happens before file read, no need to create the file
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "image.png"}))
            .await
            .unwrap();
        assert!(
            result.contains("BINARY FILE DETECTED"),
            "should detect binary: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abs.txt");
        std::fs::write(&path, "absolute").unwrap();
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(
            result.contains("absolute"),
            "should read via absolute path: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_offset_exceeds_length() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("short.txt"), "one\ntwo").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "short.txt", "offset": 999}))
            .await
            .unwrap();
        assert!(
            result.contains("exceeds file length"),
            "offset 超出文件长度应返回错误而非 panic: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_too_large() {
        let dir = tempfile::tempdir().unwrap();
        // 创建一个超过 MAX_FILE_SIZE 的稀疏文件
        let large_path = dir.path().join("huge.txt");
        let f = std::fs::File::create(&large_path).unwrap();
        f.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(f);
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "huge.txt"}))
            .await
            .unwrap();
        assert!(
            result.contains("File too large"),
            "超大文件应返回 File too large 错误: {result}"
        );
    }

    #[test]
    fn test_description_extended() {
        let tool = ReadFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(
            desc.contains("Error handling:"),
            "description 应包含 Error handling 段落"
        );
        assert!(desc.contains("line numbers"), "description 应提及行号格式");
        assert!(
            desc.len() > 200,
            "description 应为扩展后的多段落文本，长度 > 200 字符"
        );
    }

    #[test]
    fn test_tool_name_is_Read() {
        let tool = ReadFileTool::new("/tmp");
        assert_eq!(tool.name(), "Read");
    }

    #[tokio::test]
    async fn test_pdf_with_pages_returns_placeholder() {
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": "test.pdf", "pages": "1-5"}))
            .await
            .unwrap();
        assert!(
            result.contains("PDF READING NOT YET SUPPORTED"),
            "should return placeholder: {result}"
        );
    }

    #[tokio::test]
    async fn test_pdf_without_pages_returns_binary() {
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": "test.pdf"}))
            .await
            .unwrap();
        assert!(
            result.contains("BINARY FILE DETECTED"),
            "should return binary: {result}"
        );
    }
}
