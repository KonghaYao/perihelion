use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// BashTool - 终端命令执行工具，与 TypeScript TerminalMiddleware 对齐
pub struct BashTool {
    pub cwd: String,
}

impl BashTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 输出最大字节数
const MAX_OUTPUT_CHARS: usize = 100_000;
/// 输出最大行数（在第 N 行截断后，若还有行数超过上限再截字节）
const MAX_OUTPUT_LINES: usize = 2_000;

/// 按字节截断字符串，确保不拆分 UTF-8 字符
fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn truncate_output(output: &str) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    if lines.len() > MAX_OUTPUT_LINES {
        let total_lines = lines.len();
        let truncated: Vec<&str> = lines.into_iter().take(MAX_OUTPUT_LINES).collect();
        let mut result = truncated.join("\n");
        result.push_str(&format!(
            "\n\n[Output truncated: {} lines total, showing first {}]",
            total_lines,
            MAX_OUTPUT_LINES
        ));
        // 再检查字节数（使用字节截断，保留 UTF-8 字符边界）
        if result.len() > MAX_OUTPUT_CHARS {
            let truncated = truncate_bytes(&result, MAX_OUTPUT_CHARS);
            return format!("{}\n\n[Output truncated: exceeds {} byte limit]", truncated, MAX_OUTPUT_CHARS);
        }
        return result;
    }
    if output.len() > MAX_OUTPUT_CHARS {
        let truncated = truncate_bytes(output, MAX_OUTPUT_CHARS);
        return format!("{}\n\n[Output truncated: exceeds {} byte limit]", truncated, MAX_OUTPUT_CHARS);
    }
    output.to_string()
}

#[async_trait::async_trait]
impl BaseTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands in a persistent working directory context. Parameters (JSON): command: string (required), timeout_secs: number (optional, default 120)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command":      { "type": "string", "description": "The shell command to execute" },
                "timeout_secs": { "type": "number", "description": "Command timeout in seconds (default 120)" }
            },
            "required": ["command"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let command = input["command"]
            .as_str()
            .ok_or("Missing command parameter")?;

        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(120).min(300);

        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("bash", "-c")
        };

        let result = timeout(
            Duration::from_secs(timeout_secs),
            Command::new(shell)
                .arg(flag)
                .arg(command)
                .current_dir(&self.cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // 超时时 future 被 drop → Child 被 drop → 自动 SIGKILL 终止子进程
                .kill_on_drop(true)
                .output(),
        )
        .await;

        match result {
            Err(_) => Ok(format!(
                "Error: Command timed out after {timeout_secs} seconds.\nCommand: {command}"
            )),
            Ok(Err(e)) => Ok(format!("Error executing command: {e}")),
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let exit_code = out.status.code().unwrap_or(-1);

                let mut output = String::new();

                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr]\n");
                    output.push_str(&stderr);
                }
                if exit_code != 0 {
                    output.push_str(&format!("\n[Exit code: {exit_code}]"));
                }

                if output.is_empty() {
                    output = format!("[Command completed with exit code {exit_code}]");
                }

                // 截断过长输出，防止撑爆 LLM context window
                Ok(truncate_output(&output))
            }
        }
    }
}

/// TerminalMiddleware - 与 TypeScript TerminalMiddleware 对齐
pub struct TerminalMiddleware;

impl TerminalMiddleware {
    pub fn new() -> Self {
        Self
    }

    pub fn build_tools(cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(BashTool::new(cwd))]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec!["bash"]
    }
}

impl Default for TerminalMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for TerminalMiddleware {
    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools(cwd)
    }

    fn name(&self) -> &str {
        "TerminalMiddleware"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::tools::BaseTool;
    use std::time::Instant;

    #[tokio::test]
    async fn test_bash_normal_command() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_nonzero_exit_code() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"command": "exit 42"}))
            .await
            .unwrap();
        assert!(result.contains("42"), "应包含退出码: {result}");
    }

    /// 验证超时后在合理时间内返回，且 kill_on_drop 确保子进程被清理
    #[tokio::test]
    async fn test_bash_timeout_returns_quickly() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let start = Instant::now();

        // Windows 用 ping 模拟 sleep，Unix 用 sleep
        let (sleep_cmd, timeout_secs) = if cfg!(target_os = "windows") {
            ("ping -n 60 127.0.0.1", 1)
        } else {
            ("sleep 60", 1)
        };

        let result = tool
            .invoke(serde_json::json!({
                "command": sleep_cmd,
                "timeout_secs": timeout_secs
            }))
            .await
            .unwrap();
        let elapsed = start.elapsed();

        // 应在约 1 秒内返回（不超过 3 秒），不等待 sleep 60 完成
        assert!(
            elapsed.as_secs() < 3,
            "超时后应快速返回，实际耗时 {:?}",
            elapsed
        );
        assert!(
            result.contains("timed out"),
            "返回值应包含超时提示: {result}"
        );
    }

    #[tokio::test]
    async fn test_bash_stderr_captured() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"command": "echo err >&2"}))
            .await
            .unwrap();
        assert!(result.contains("err"), "stderr 应被捕获: {result}");
    }

    #[test]
    fn test_truncate_output_line_count_accurate() {
        // 生成不含末尾换行的多行文本，避免 split('\n') 产生额外空行
        let lines: Vec<String> = (0..3000).map(|i| format!("line {}", i)).collect();
        let input = lines.join("\n");
        assert_eq!(input.split('\n').count(), 3000);
        let result = truncate_output(&input);
        assert!(result.contains("3000 lines total"), "应显示正确的总行数: {result}");
        assert!(result.contains(&format!("showing first {}", MAX_OUTPUT_LINES)));
    }

    #[test]
    fn test_truncate_output_no_truncation_when_small() {
        let result = truncate_output("hello\nworld");
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn test_truncate_output_char_limit() {
        let long_line = "x".repeat(200_000);
        let result = truncate_output(&long_line);
        assert!(result.contains("byte limit"), "应截断超长输出: {result}");
    }
}
